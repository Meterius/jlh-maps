use crate::gtfs::artifact_store::{
    ArtifactStore, ArtifactStoreConfig as InternalArtifactStoreConfig, ArtifactStoreConfig,
};
use crate::gtfs::postgres::{
    self, FeedVersionImportInfo, GTFS_TILING_EXPORT_CHUNK_ZOOM, GTFS_TILING_ZOOM,
    PromoteVersionOutcome, SyncTilingSourceOutcome,
};
use crate::model::SeedFile;
use anyhow::{Context, Result, bail};
use futures_util::stream::{self, BoxStream};
use futures_util::{StreamExt, TryStreamExt};
use pmtiles::{PmTilesWriter, TileCoord, TileType};
use reqwest::StatusCode;
use reqwest::header::{
    ETAG, HeaderMap, HeaderName, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED, USER_AGENT,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tracing::{error, info};

/// Runtime dependencies for the GTFS ingestion client.
#[derive(Debug, Clone)]
pub struct GtfsIngestConfig {
    /// Postgres connection string for GTFS metadata and schedule rows.
    pub database_url: String,
    /// Object-store settings for immutable feed ZIP artifacts.
    pub artifact_store: ArtifactStoreConfig,
}

/// Client-facing view of a GTFS feed version lifecycle row.
#[derive(Debug, Clone)]
pub struct FeedVersion {
    pub id: i64,
    pub source_id: i64,
    pub download_url: String,
    pub content_sha256: String,
    pub file_bytes: i64,
    pub file_path: String,
    pub status: String,
}

impl From<FeedVersionImportInfo> for FeedVersion {
    fn from(record: FeedVersionImportInfo) -> Self {
        Self {
            id: record.id,
            source_id: record.source_id,
            download_url: record.download_url,
            content_sha256: record.content_sha256,
            file_bytes: record.file_bytes,
            file_path: record.file_path,
            status: record.status,
        }
    }
}

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

#[derive(Debug, Clone)]
pub struct SyncFailure {
    pub source_slug: String,
    pub error: String,
}

#[derive(Debug, Clone)]
pub struct SyncCommandOutcome<T> {
    pub succeeded: Vec<T>,
    pub failed: Vec<SyncFailure>,
}

impl<T> SyncCommandOutcome<T> {
    pub fn has_failures(&self) -> bool {
        !self.failed.is_empty()
    }

    pub fn total_count(&self) -> usize {
        self.succeeded.len() + self.failed.len()
    }
}

pub type SyncSourcesOutcome = SyncCommandOutcome<SyncSourceOutcome>;

#[derive(Debug, Clone)]
pub struct SyncTilingSourceResult {
    pub source_slug: String,
    pub outcome: SyncTilingSourceOutcome,
}

pub type SyncTilingOutcome = SyncCommandOutcome<SyncTilingSourceResult>;

#[derive(Debug, Clone)]
pub struct ExportTilingOutcome {
    pub source_slug: Option<String>,
    pub tile_count: i64,
    pub output_file: PathBuf,
}

#[derive(Debug, Clone)]
pub struct GtfsIngestClient {
    pool: PgPool,
    artifact_store: ArtifactStore,
    http_client: reqwest::Client,
}

impl GtfsIngestClient {
    pub async fn connect(config: GtfsIngestConfig) -> Result<Self> {
        let pool = PgPool::connect(&config.database_url)
            .await
            .context("failed to connect to GTFS Postgres database")?;

        let artifact_store = ArtifactStore::new(&InternalArtifactStoreConfig {
            endpoint: config.artifact_store.endpoint,
            region: config.artifact_store.region,
            bucket: config.artifact_store.bucket,
            access_key_id: config.artifact_store.access_key_id,
            secret_access_key: config.artifact_store.secret_access_key,
        })?;

        Ok(Self {
            pool,
            artifact_store,
            http_client: reqwest::Client::new(),
        })
    }

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

/// Upserts feed sources from a seed file model.
pub async fn upsert_feed_sources_seed(
    database_url: &str,
    seed: &SeedFile,
    delete_existing: bool,
) -> Result<()> {
    let pool = PgPool::connect(database_url)
        .await
        .context("failed to connect to GTFS Postgres database")?;

    postgres::upsert_feed_sources_seed(&pool, seed, delete_existing).await
}

// Source Sync

struct DownloadedFeed {
    body: Vec<u8>,
    http_etag: Option<String>,
    http_last_modified: Option<String>,
}

enum DownloadFeedOutcome {
    NotModified,
    Downloaded(DownloadedFeed),
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

// Tiling Sync

pub async fn sync_tiling(database_url: &str, parallelism: usize) -> Result<SyncTilingOutcome> {
    let parallelism = parallelism.max(1);

    let pool = PgPool::connect(database_url)
        .await
        .context("failed to connect to GTFS Postgres database")?;

    let source_slugs = postgres::list_feed_source_slugs(&pool).await?;

    info!(
        source_count = source_slugs.len(),
        parallelism, "syncing GTFS tiling sources"
    );

    let sync_log_counters = Arc::new(SyncLogCounters::new(source_slugs.len()));

    let results = stream::iter(source_slugs)
        .map(|source_slug| {
            let pool = &pool;
            let sync_log_counters = Arc::clone(&sync_log_counters);

            async move {
                info!(
                    source_slug = %source_slug,
                    "syncing GTFS tiling for feed source"
                );
                match postgres::sync_tiling_for_source(pool, &source_slug).await {
                    Ok(outcome) => {
                        let progress = sync_log_counters.record_success();
                        info!(
                            source_slug = %source_slug,
                            ?outcome,
                            ?progress,
                            "completed GTFS tiling source sync"
                        );
                        Ok(SyncTilingSourceResult {
                            source_slug,
                            outcome,
                        })
                    }
                    Err(error) => {
                        let progress = sync_log_counters.record_failure();
                        let error = format!("{error:#}");
                        error!(
                            source_slug = %source_slug,
                            error = %error,
                            ?progress,
                            "failed GTFS tiling source sync"
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

// Tiling Export

pub async fn export_tiling(
    database_url: &str,
    source_slug: Option<&str>,
    output_file: &Path,
    parallelism: usize,
) -> Result<ExportTilingOutcome> {
    let pool = PgPool::connect(database_url)
        .await
        .context("failed to connect to GTFS Postgres database")?;

    let tile_count = write_pmtiles(&pool, source_slug, output_file, parallelism).await?;

    Ok(ExportTilingOutcome {
        source_slug: source_slug.map(str::to_owned),
        tile_count,
        output_file: output_file.to_owned(),
    })
}

async fn write_pmtiles(
    pool: &PgPool,
    source_slug: Option<&str>,
    output_file: &Path,
    parallelism: usize,
) -> Result<i64> {
    let parallelism = parallelism.max(1);

    if let Some(parent) = output_file.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create GTFS PMTiles output directory {}",
                parent.display()
            )
        })?;
    }

    let temp_file = temp_output_path(output_file);
    let file = File::create(&temp_file).with_context(|| {
        format!(
            "failed to create temporary GTFS PMTiles file {}",
            temp_file.display()
        )
    })?;

    let metadata = tiling_metadata(source_slug)?;

    fn to_zoom_u8(zoom: i32) -> Result<u8> {
        zoom.try_into()
            .with_context(|| format!("invalid PMTiles zoom {}", zoom))
    }

    fn to_tile_u32(value: i32) -> Result<u32> {
        value
            .try_into()
            .with_context(|| format!("invalid PMTiles tile coordinate {}", value))
    }

    let mut writer = PmTilesWriter::new(TileType::Mvt)
        .min_zoom(to_zoom_u8(GTFS_TILING_ZOOM)?)
        .max_zoom(to_zoom_u8(GTFS_TILING_ZOOM)?)
        .center_zoom(to_zoom_u8(GTFS_TILING_ZOOM)?)
        .center(0.0, 0.0)
        .bounds(-180.0, -85.051_128_78, 180.0, 85.051_128_78)
        .metadata(&metadata)
        .create(file)
        .context("failed to initialize GTFS PMTiles writer")?;

    info!(
        source_slug = %source_slug.unwrap_or("<all>"),
        output_file = %output_file.display(),
        parallelism,
        chunk_zoom = GTFS_TILING_EXPORT_CHUNK_ZOOM,
        export_zoom = GTFS_TILING_ZOOM,
        "streaming GTFS PMTiles export tiles"
    );

    let mut tile_stream = stream_tiling_export_tiles(pool, source_slug, parallelism);
    let mut tile_count = 0_i64;

    while let Some(tile) = tile_stream
        .try_next()
        .await
        .context("failed to stream GTFS MVT tiles for export")?
    {
        let coord = TileCoord::new(
            to_zoom_u8(tile.z)?,
            to_tile_u32(tile.x)?,
            to_tile_u32(tile.y)?,
        )
        .context("failed to create GTFS PMTiles tile coordinate")?;

        writer.add_tile(coord, &tile.tile).with_context(|| {
            format!(
                "failed to write GTFS PMTiles tile {}/{}/{}",
                tile.z, tile.x, tile.y
            )
        })?;

        tile_count += 1;
    }

    writer
        .finalize()
        .context("failed to finalize GTFS PMTiles archive")?;

    if tile_count == 0 {
        let _ = std::fs::remove_file(&temp_file);
        match source_slug {
            Some(source_slug) => bail!(
                "GTFS tiling for source {} has no tiles to export",
                source_slug
            ),
            None => bail!("GTFS tiling has no tiles to export"),
        }
    }

    replace_file(&temp_file, output_file)?;

    info!(
        source_slug = %source_slug.unwrap_or("<all>"),
        output_file = %output_file.display(),
        tile_count,
        "finished GTFS PMTiles export"
    );

    Ok(tile_count)
}

fn stream_tiling_export_tiles<'a>(
    pool: &'a PgPool,
    source_slug: Option<&'a str>,
    parallelism: usize,
) -> BoxStream<'a, std::result::Result<postgres::TilingExportTile, sqlx::Error>> {
    let max_processed_chunk_index = Arc::new(AtomicUsize::new(0));
    let chunk_stream = postgres::stream_tile_ids_intersecting_geometry(
        pool,
        source_slug,
        GTFS_TILING_EXPORT_CHUNK_ZOOM,
    )
    .map_ok({
        let max_processed_chunk_index = Arc::clone(&max_processed_chunk_index);
        move |tile_id| {
            let (chunk_index, total_chunks) = chunk_progress_index(&tile_id);
            let previous_max = max_processed_chunk_index.fetch_max(chunk_index, Ordering::AcqRel);
            let processed_chunks = previous_max.max(chunk_index);
            let remaining_chunks = total_chunks.saturating_sub(processed_chunks);

            info!(
                source_slug = %source_slug.unwrap_or("<all>"),
                chunk_z = tile_id.z,
                chunk_x = tile_id.x,
                chunk_y = tile_id.y,
                processed_chunks,
                remaining_chunks,
                total_chunks,
                progress = %format_args!("{processed_chunks}/{total_chunks}"),
                "processing GTFS PMTiles export chunk"
            );

            postgres::stream_export_tiles(pool, source_slug, tile_id).boxed()
        }
    });

    if parallelism <= 1 {
        chunk_stream.try_flatten().boxed()
    } else {
        chunk_stream
            .try_flatten_unordered(Some(parallelism))
            .boxed()
    }
}

fn chunk_progress_index(tile_id: &postgres::TilingExportTileId) -> (usize, usize) {
    let z = tile_id.z.max(0) as u32;
    let tile_axis_count = 1_usize << z;
    let chunk_index = tile_id.y.max(0) as usize * tile_axis_count + tile_id.x.max(0) as usize + 1;
    let total_chunks = tile_axis_count * tile_axis_count;

    (chunk_index, total_chunks)
}

fn tiling_metadata(source_slug: Option<&str>) -> Result<String> {
    Ok(json!({
        "name": "gtfs",
        "description": "GTFS schedule stop vector tiles",
        "version": source_slug.unwrap_or("gtfs"),
        "vector_layers": [
            {
                "id": "stops",
                "description": "GTFS stop and station points",
                "fields": {
                    "source_slug": "String",
                    "stop_id": "String",
                    "stop_code": "String",
                    "stop_name": "String",
                    "location_type": "Number",
                    "wheelchair_boarding": "Number",
                    "platform_code": "String"
                }
            }
        ]
    })
    .to_string())
}

fn temp_output_path(output_file: &Path) -> PathBuf {
    let file_name = output_file
        .file_name()
        .map(|file_name| file_name.to_string_lossy())
        .unwrap_or_else(|| "tiles.pmtiles".into());
    output_file.with_file_name(format!(".{file_name}.tmp"))
}

fn replace_file(temp_file: &Path, output_file: &Path) -> Result<()> {
    match std::fs::rename(temp_file, output_file) {
        Ok(()) => Ok(()),
        Err(error) if output_file.exists() => {
            std::fs::remove_file(output_file).with_context(|| {
                format!(
                    "failed to remove previous GTFS PMTiles file {}",
                    output_file.display()
                )
            })?;
            std::fs::rename(temp_file, output_file).with_context(|| {
                format!(
                    "failed to move GTFS PMTiles file from {} to {} after removing previous output: {}",
                    temp_file.display(),
                    output_file.display(),
                    error
                )
            })
        }
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to move GTFS PMTiles file from {} to {}",
                temp_file.display(),
                output_file.display()
            )
        }),
    }
}

// Helpers

fn partition_sync_results<T>(
    results: Vec<std::result::Result<T, SyncFailure>>,
) -> SyncCommandOutcome<T> {
    let mut succeeded = Vec::new();
    let mut failed = Vec::new();

    for result in results {
        match result {
            Ok(outcome) => succeeded.push(outcome),
            Err(failure) => failed.push(failure),
        }
    }

    SyncCommandOutcome { succeeded, failed }
}

struct SyncLogCounters {
    total_count: usize,
    succeeded_count: AtomicUsize,
    failed_count: AtomicUsize,
}

#[allow(dead_code)]
#[derive(Debug)]
struct SyncLogProgress {
    succeeded_count: usize,
    failed_count: usize,
    remaining_count: usize,
}

impl SyncLogCounters {
    fn new(total_count: usize) -> Self {
        Self {
            total_count,
            succeeded_count: AtomicUsize::new(0),
            failed_count: AtomicUsize::new(0),
        }
    }

    fn record_success(&self) -> SyncLogProgress {
        let succeeded_count = self.succeeded_count.fetch_add(1, Ordering::AcqRel) + 1;
        let failed_count = self.failed_count.load(Ordering::Acquire);
        self.progress(succeeded_count, failed_count)
    }

    fn record_failure(&self) -> SyncLogProgress {
        let failed_count = self.failed_count.fetch_add(1, Ordering::AcqRel) + 1;
        let succeeded_count = self.succeeded_count.load(Ordering::Acquire);
        self.progress(succeeded_count, failed_count)
    }

    fn progress(&self, succeeded_count: usize, failed_count: usize) -> SyncLogProgress {
        SyncLogProgress {
            succeeded_count,
            failed_count,
            remaining_count: self
                .total_count
                .saturating_sub(succeeded_count + failed_count),
        }
    }
}
