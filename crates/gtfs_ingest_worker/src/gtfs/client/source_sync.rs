use super::client::GtfsIngestClient;
use super::models::FeedVersion;
use super::progress::{SyncCommandOutcome, SyncFailure, SyncLogCounters, partition_sync_results};
use crate::gtfs::postgres::{self, PromoteVersionOutcome};
use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use futures_util::stream;
use reqwest::StatusCode;
use reqwest::header::{
    ETAG, HeaderMap, HeaderName, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED, USER_AGENT,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::{error, info};

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

#[derive(Debug, Clone)]
pub struct SyncSourceOutcome {
    pub source_slug: String,
    pub prepared: PrepareLatestFeedVersionOutcome,
    pub imported: Option<ImportFeedVersionOutcome>,
    pub promoted: Option<PromoteFeedVersionOutcome>,
}

pub type SyncSourcesOutcome = SyncCommandOutcome<SyncSourceOutcome>;

struct DownloadedFeed {
    body: Vec<u8>,
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

        let feed_bytes = downloaded_feed.body;
        let content_sha256 = sha256_hex(&feed_bytes);
        let file_bytes: i64 = feed_bytes
            .len()
            .try_into()
            .context("GTFS feed artifact is too large to record byte length")?;

        if let Some(active_download_cache) = active_version_info.as_ref()
            && active_download_cache.content_sha256 == content_sha256
        {
            postgres::update_version_http_download_info(
                &self.pool,
                active_download_cache.id,
                downloaded_feed.http_etag.as_deref(),
                downloaded_feed.http_last_modified.as_deref(),
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

        self.artifact_store
            .put_feed_artifact(&file_path, &feed_bytes)
            .await?;

        let version = postgres::create_downloaded_version(
            &self.pool,
            postgres::CreateDownloadVersionInput {
                source: &source,
                download_url,
                content_sha256: &content_sha256,
                file_bytes,
                file_path: &file_path,
                http_etag: downloaded_feed.http_etag.as_deref(),
                http_last_modified: downloaded_feed.http_last_modified.as_deref(),
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
    ) -> Result<ImportFeedVersionOutcome> {
        let version = postgres::fetch_version_import_info(&self.pool, version_id).await?;

        if matches!(version.status.as_str(), "imported" | "active") {
            info!(
                source_slug = %source_slug,
                status = %version.status,
                "GTFS feed version import is already stable"
            );
            return Ok(ImportFeedVersionOutcome::AlreadyStable {
                version: version.into(),
            });
        }

        info!(
            source_slug = %source_slug,
            file_path = %version.file_path,
            "downloading GTFS artifact for import"
        );

        let zip_body = match self
            .artifact_store
            .get_feed_artifact(&version.file_path)
            .await
        {
            Ok(zip_body) => zip_body,
            Err(error) => {
                let _ =
                    postgres::mark_import_failed(&self.pool, version_id, &error.to_string()).await;
                return Err(error);
            }
        };

        match postgres::import_feed_version_from_zip(&self.pool, version_id, zip_body).await {
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

    /// Runs prepare, import, and promotion for one feed source.
    pub async fn sync_source(&self, source_slug: &str) -> Result<SyncSourceOutcome> {
        let prepared = self.prepare_latest_feed_version(source_slug).await?;

        let Some(prepared_version) = prepared_version(&prepared) else {
            return Ok(SyncSourceOutcome {
                source_slug: source_slug.to_owned(),
                prepared,
                imported: None,
                promoted: None,
            });
        };

        let imported = if matches!(
            prepared_version.status.as_str(),
            "downloaded" | "import_failed"
        ) {
            Some(
                self.import_feed_version(source_slug, prepared_version.id)
                    .await?,
            )
        } else {
            None
        };

        let candidate_version = imported
            .as_ref()
            .map(imported_version)
            .unwrap_or(prepared_version);

        let promoted = if candidate_version.status == "active" {
            None
        } else if candidate_version.status == "imported" {
            Some(
                self.try_promote_latest_feed_version(source_slug, candidate_version.id)
                    .await?,
            )
        } else {
            bail!(
                "GTFS version {} stopped in non-promotable status {}",
                candidate_version.id,
                candidate_version.status
            );
        };

        Ok(SyncSourceOutcome {
            source_slug: source_slug.to_owned(),
            prepared,
            imported,
            promoted,
        })
    }

    /// Runs sync for every configured feed source.
    pub async fn sync_sources(&self, parallelism: usize) -> Result<SyncSourcesOutcome> {
        let parallelism = parallelism.max(1);

        let source_slugs = postgres::list_feed_source_slugs(&self.pool).await?;

        info!(
            source_count = source_slugs.len(),
            parallelism, "syncing GTFS feed sources"
        );

        let sync_log_counters = Arc::new(SyncLogCounters::new(source_slugs.len()));

        let results = stream::iter(source_slugs)
            .map(|source_slug| {
                let sync_log_counters = Arc::clone(&sync_log_counters);

                async move {
                    info!(
                        source_slug = %source_slug,
                        "syncing GTFS feed source"
                    );
                    match self.sync_source(&source_slug).await {
                        Ok(outcome) => {
                            let progress = sync_log_counters.record_success();
                            info!(
                                source_slug = %source_slug,
                                ?outcome,
                                ?progress,
                                "completed GTFS feed source sync"
                            );
                            Ok(outcome)
                        }
                        Err(error) => {
                            let progress = sync_log_counters.record_failure();
                            let error = format!("{error:#}");
                            error!(
                                source_slug = %source_slug,
                                error = %error,
                                ?progress,
                                "failed GTFS feed source sync"
                            );
                            Err(SyncFailure { source_slug, error })
                        }
                    }
                }
            })
            .buffer_unordered(parallelism)
            .collect::<Vec<_>>()
            .await;

        Ok(partition_sync_results(results))
    }
}

async fn download_feed(
    client: &reqwest::Client,
    download_url: &str,
    active_download_cache: Option<&postgres::FeedVersionDownloadInfo>,
) -> anyhow::Result<DownloadFeedOutcome> {
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

    let response = response
        .error_for_status()
        .with_context(|| format!("GTFS feed request failed for {}", download_url))?;

    let headers = response.headers().clone();

    let body = response
        .bytes()
        .await
        .with_context(|| format!("failed to read GTFS feed {}", download_url))?;

    Ok(DownloadFeedOutcome::Downloaded(DownloadedFeed {
        body: body.to_vec(),
        http_etag: header_to_string(&headers, ETAG),
        http_last_modified: header_to_string(&headers, LAST_MODIFIED),
    }))
}

fn header_to_string(headers: &HeaderMap, header_name: HeaderName) -> Option<String> {
    headers
        .get(header_name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn sha256_hex(body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body);
    format!("{:x}", hasher.finalize())
}

fn feed_artifact_path(source_slug: &str, content_sha256: &str) -> String {
    format!("feed-sources/{source_slug}/versions/{content_sha256}.zip")
}

fn prepared_version(outcome: &PrepareLatestFeedVersionOutcome) -> Option<&FeedVersion> {
    match outcome {
        PrepareLatestFeedVersionOutcome::AlreadyActive { .. } => None,
        PrepareLatestFeedVersionOutcome::Prepared { version } => Some(version),
    }
}

fn imported_version(outcome: &ImportFeedVersionOutcome) -> &FeedVersion {
    match outcome {
        ImportFeedVersionOutcome::AlreadyStable { version } => version,
        ImportFeedVersionOutcome::Imported { version } => version,
    }
}
