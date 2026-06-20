use super::client::GtfsIngestClient;
use super::models::FeedVersion;
use crate::gtfs::postgres::{self, PromoteVersionOutcome};
use anyhow::{Context, Result, bail};
use reqwest::StatusCode;
use reqwest::header::{
    CONTENT_TYPE, ETAG, HeaderMap, HeaderName, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED,
    USER_AGENT,
};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use tokio::io::AsyncWriteExt;
use tracing::info;
use zip::ZipArchive;

#[derive(Debug, Clone)]
pub enum PrepareLatestFeedVersionOutcome {
    AlreadyActive { content_sha256: String },
    Prepared { version: FeedVersion },
}

#[derive(Debug, Clone)]
pub enum ImportFeedVersionOutcome {
    AlreadyStable { version: FeedVersion },
    Imported { version: FeedVersion },
}

#[derive(Debug, Clone)]
pub enum PromoteFeedVersionOutcome {
    AlreadyActive { version: FeedVersion },
    CurrentActiveIsNewer { version: FeedVersion },
    Promoted { version: FeedVersion },
}

struct DownloadedFeed {
    artifact_file: NamedTempFile,
    content_sha256: String,
    file_bytes: i64,
    http_etag: Option<String>,
    http_last_modified: Option<String>,
}

enum DownloadFeedOutcome {
    NotModified,
    Downloaded(DownloadedFeed),
}

impl GtfsIngestClient {
    /// Downloads a source feed and creates a downloaded version if content has changed.
    pub async fn prepare_latest_feed_version(
        &self,
        source_slug: &str,
    ) -> Result<PrepareLatestFeedVersionOutcome> {
        let source = postgres::fetch_feed_source_download_info(&self.pool, source_slug)
            .await?
            .with_context(|| format!("GTFS feed source {} does not exist", source_slug))?;

        let download_url = source.direct_download_url.as_str();

        info!(
            source_slug = %source.slug,
            %download_url,
            "downloading latest GTFS feed"
        );

        let active_version_info =
            postgres::fetch_active_version_download_info(&self.pool, source.id).await?;

        let downloaded_feed = match download_feed(
            &self.http_client,
            download_url,
            active_version_info.as_ref(),
        )
        .await?
        {
            DownloadFeedOutcome::NotModified => {
                let active_download_cache = active_version_info
                    .context("GTFS feed returned 304 without an active cached version")?;

                info!(
                    source_slug = %source.slug,
                    content_sha256 = %active_download_cache.content_sha256,
                    active_version_id = active_download_cache.id,
                    "latest GTFS feed is already active by HTTP cache validators"
                );

                return Ok(PrepareLatestFeedVersionOutcome::AlreadyActive {
                    content_sha256: active_download_cache.content_sha256,
                });
            }
            DownloadFeedOutcome::Downloaded(downloaded_feed) => downloaded_feed,
        };

        let DownloadedFeed {
            artifact_file,
            content_sha256,
            file_bytes,
            http_etag,
            http_last_modified,
        } = downloaded_feed;

        if let Some(active_download_cache) = active_version_info.as_ref()
            && active_download_cache.content_sha256 == content_sha256
        {
            postgres::update_version_http_download_info(
                &self.pool,
                active_download_cache.id,
                http_etag.as_deref(),
                http_last_modified.as_deref(),
            )
            .await?;

            info!(
                source_slug = %source.slug,
                %content_sha256,
                active_version_id = active_download_cache.id,
                "latest GTFS feed is already active after content hash comparison"
            );

            return Ok(PrepareLatestFeedVersionOutcome::AlreadyActive { content_sha256 });
        }

        let file_path = feed_artifact_path(&source.slug, &content_sha256);

        info!(
            source_slug = %source.slug,
            %content_sha256,
            file_bytes,
            %file_path,
            "uploading GTFS feed artifact"
        );

        let mut artifact_reader =
            tokio::fs::File::from_std(artifact_file.reopen().with_context(|| {
                format!(
                    "failed to reopen temporary GTFS feed artifact {}",
                    artifact_file.path().display()
                )
            })?);

        self.artifact_store
            .put_feed_artifact_stream(&file_path, &mut artifact_reader)
            .await?;

        let version = postgres::create_downloaded_version(
            &self.pool,
            postgres::CreateDownloadVersionInput {
                source: &source,
                download_url,
                content_sha256: &content_sha256,
                file_bytes,
                file_path: &file_path,
                http_etag: http_etag.as_deref(),
                http_last_modified: http_last_modified.as_deref(),
            },
        )
        .await?;

        info!(
            source_slug = %source.slug,
            version_id = version.id,
            status = %version.status,
            "prepared GTFS feed version"
        );

        Ok(PrepareLatestFeedVersionOutcome::Prepared {
            version: version.into(),
        })
    }

    /// Imports a downloaded or failed version into durable GTFS tables.
    pub async fn import_feed_version(
        &self,
        source_slug: &str,
        version_id: i64,
        version_info: Option<&FeedVersion>,
    ) -> Result<ImportFeedVersionOutcome> {
        let version = match version_info {
            Some(version) if version.id == version_id => version.clone(),
            Some(version) => bail!(
                "GTFS version info id {} does not match requested version {}",
                version.id,
                version_id
            ),
            None => postgres::fetch_version_import_info(&self.pool, version_id)
                .await?
                .into(),
        };

        if matches!(version.status.as_str(), "imported" | "active") {
            info!(
                source_slug = %source_slug,
                status = %version.status,
                "GTFS feed version import is already stable"
            );
            return Ok(ImportFeedVersionOutcome::AlreadyStable { version });
        }

        info!(
            source_slug = %source_slug,
            file_path = %version.file_path,
            "downloading GTFS artifact for import"
        );

        let import_result = async {
            let artifact_file =
                NamedTempFile::new().context("failed to create temporary GTFS artifact file")?;

            // stream artifact into temporary file
            {
                let mut artifact_writer =
                    tokio::fs::File::from_std(artifact_file.reopen().with_context(|| {
                        format!(
                            "failed to reopen temporary GTFS artifact {} for writing",
                            artifact_file.path().display()
                        )
                    })?);

                self.artifact_store
                    .get_feed_artifact_stream(&version.file_path, &mut artifact_writer)
                    .await?;
            }

            let zip_file = artifact_file.reopen().with_context(|| {
                format!(
                    "failed to reopen temporary GTFS artifact {} for import",
                    artifact_file.path().display()
                )
            })?;

            let zip_archive = ZipArchive::new(zip_file).with_context(|| {
                format!(
                    "failed to open GTFS artifact {} as ZIP",
                    artifact_file.path().display()
                )
            })?;

            postgres::import_feed_version_from_zip(&self.pool, version_id, zip_archive).await
        }
        .await;

        match import_result {
            Ok(()) => {
                let version = postgres::fetch_version_import_info(&self.pool, version_id).await?;
                if matches!(version.status.as_str(), "imported") {
                    info!(
                        source_slug = %source_slug,
                        "imported GTFS feed version"
                    );
                    Ok(ImportFeedVersionOutcome::Imported {
                        version: version.into(),
                    })
                } else {
                    Ok(ImportFeedVersionOutcome::AlreadyStable {
                        version: version.into(),
                    })
                }
            }
            Err(error) => {
                let _ =
                    postgres::mark_import_failed(&self.pool, version_id, &error.to_string()).await;
                Err(error)
            }
        }
    }

    /// Promotes an imported version if it is not older than the active version.
    pub async fn try_promote_latest_feed_version(
        &self,
        source_slug: &str,
        version_id: i64,
    ) -> Result<PromoteFeedVersionOutcome> {
        match postgres::promote_feed_version(&self.pool, version_id).await? {
            PromoteVersionOutcome::AlreadyActive(version) => {
                info!(
                    source_slug = %source_slug,
                    status = %version.status,
                    "GTFS feed version is already active"
                );
                Ok(PromoteFeedVersionOutcome::AlreadyActive {
                    version: version.into(),
                })
            }
            PromoteVersionOutcome::CurrentActiveIsNewer(version) => {
                info!(
                    source_slug = %source_slug,
                    status = %version.status,
                    "skipped GTFS promotion because current active version is newer"
                );
                Ok(PromoteFeedVersionOutcome::CurrentActiveIsNewer {
                    version: version.into(),
                })
            }
            PromoteVersionOutcome::Promoted(version) => {
                info!(
                    source_slug = %source_slug,
                    status = %version.status,
                    "promoted GTFS feed version"
                );
                Ok(PromoteFeedVersionOutcome::Promoted {
                    version: version.into(),
                })
            }
        }
    }
}

async fn download_feed(
    client: &reqwest::Client,
    download_url: &str,
    active_download_cache: Option<&postgres::FeedVersionDownloadInfo>,
) -> Result<DownloadFeedOutcome> {
    let mut request = client.get(download_url).header(
        USER_AGENT,
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
    );

    if let Some(active_download_cache) = active_download_cache {
        if let Some(http_etag) = active_download_cache.http_etag.as_deref() {
            request = request.header(IF_NONE_MATCH, http_etag);
        }

        if let Some(http_last_modified) = active_download_cache.http_last_modified.as_deref() {
            request = request.header(IF_MODIFIED_SINCE, http_last_modified);
        }
    }

    let response = request
        .send()
        .await
        .with_context(|| format!("failed to request GTFS feed {}", download_url))?;

    if response.status() == StatusCode::NOT_MODIFIED {
        return Ok(DownloadFeedOutcome::NotModified);
    }

    let mut response = response
        .error_for_status()
        .with_context(|| format!("GTFS feed request failed for {}", download_url))?;

    let headers = response.headers().clone();

    ensure_gtfs_feed_content_type(download_url, response.url().as_str(), &headers)?;

    let artifact_file =
        NamedTempFile::new().context("failed to create temporary GTFS feed artifact file")?;

    // stream response body into temporary file while hashing and counting file bytes
    let (content_sha256, file_bytes) = {
        let mut hasher = Sha256::new();
        let mut file_bytes = 0_u64;

        let mut artifact_writer =
            tokio::fs::File::from_std(artifact_file.reopen().with_context(|| {
                format!(
                    "failed to reopen temporary GTFS feed artifact {} for writing",
                    artifact_file.path().display()
                )
            })?);

        while let Some(chunk) = response
            .chunk()
            .await
            .with_context(|| format!("failed to read GTFS feed {}", download_url))?
        {
            hasher.update(&chunk);

            file_bytes = file_bytes
                .checked_add(chunk.len() as u64)
                .context("GTFS feed artifact byte length overflowed")?;

            artifact_writer.write_all(&chunk).await.with_context(|| {
                format!(
                    "failed to write GTFS feed chunk to {}",
                    artifact_file.path().display()
                )
            })?;
        }

        artifact_writer.flush().await.with_context(|| {
            format!(
                "failed to flush temporary GTFS feed artifact {}",
                artifact_file.path().display()
            )
        })?;

        (
            format!("{:x}", hasher.finalize()),
            file_bytes
                .try_into()
                .context("GTFS feed artifact is too large to record byte length")?,
        )
    };

    Ok(DownloadFeedOutcome::Downloaded(DownloadedFeed {
        artifact_file,
        content_sha256,
        file_bytes,
        http_etag: header_to_string(&headers, ETAG),
        http_last_modified: header_to_string(&headers, LAST_MODIFIED),
    }))
}

fn ensure_gtfs_feed_content_type(
    download_url: &str,
    final_url: &str,
    headers: &HeaderMap,
) -> Result<()> {
    let Some(content_type) = headers.get(CONTENT_TYPE) else {
        return Ok(());
    };

    let content_type = content_type.to_str().with_context(|| {
        format!(
            "GTFS feed {} resolved to {} with an invalid Content-Type header",
            download_url, final_url
        )
    })?;

    let media_type = content_type
        .split(';')
        .next()
        .unwrap_or(content_type)
        .trim()
        .to_ascii_lowercase();

    if matches!(
        media_type.as_str(),
        "application/zip"
            | "application/x-zip"
            | "application/x-zip-compressed"
            | "application/octet-stream"
            | "binary/octet-stream"
    ) {
        return Ok(());
    }

    bail!(
        "GTFS feed {} resolved to {} with unsupported Content-Type {}; expected a ZIP response",
        download_url,
        final_url,
        content_type
    );
}

fn header_to_string(headers: &HeaderMap, header_name: HeaderName) -> Option<String> {
    headers
        .get(header_name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn feed_artifact_path(source_slug: &str, content_sha256: &str) -> String {
    format!("feed-sources/{source_slug}/versions/{content_sha256}.zip")
}

pub fn prepared_version(outcome: &PrepareLatestFeedVersionOutcome) -> Option<&FeedVersion> {
    match outcome {
        PrepareLatestFeedVersionOutcome::AlreadyActive { .. } => None,
        PrepareLatestFeedVersionOutcome::Prepared { version } => Some(version),
    }
}

pub fn imported_version(outcome: &ImportFeedVersionOutcome) -> &FeedVersion {
    match outcome {
        ImportFeedVersionOutcome::AlreadyStable { version } => version,
        ImportFeedVersionOutcome::Imported { version } => version,
    }
}
