use crate::gtfs::artifact_store::{
    ArtifactStore, ArtifactStoreConfig as InternalArtifactStoreConfig, ArtifactStoreConfig,
};
use crate::gtfs::postgres::{
    self, FeedVersionInfo, GTFS_TILING_ZOOM, PromoteVersionOutcome, SyncTilingSourceOutcome,
};
use crate::model::SeedFile;
use anyhow::{Context, Result, bail};
use futures_util::TryStreamExt;
use pmtiles::{PmTilesWriter, TileCoord, TileType};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::fs::File;
use std::path::{Path, PathBuf};
use tracing::info;

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
    /// SHA-256 hash of the stored GTFS ZIP artifact.
    pub content_sha256: String,
    pub file_bytes: i64,
    /// Object-store key for the stored GTFS ZIP artifact.
    pub file_path: String,
    /// Lifecycle state: downloaded, import_failed, imported, or active.
    pub status: String,
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
pub struct ExportTilingOutcome {
    pub source_slug: Option<String>,
    pub tile_count: i64,
    pub output_file: PathBuf,
}

#[derive(Debug, Clone)]
pub struct GtfsIngestClient {
    pool: PgPool,
    artifact_store: ArtifactStore,
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

        let feed_bytes = download_feed(download_url).await?;
        let content_sha256 = sha256_hex(&feed_bytes);
        let file_bytes: i64 = feed_bytes
            .len()
            .try_into()
            .context("GTFS feed artifact is too large to record byte length")?;

        if let Some(active_content_sha256) =
            postgres::fetch_active_version_content_hash(&self.pool, source.id).await?
            && active_content_sha256 == content_sha256
        {
            info!(
                source_slug = %source.slug,
                %content_sha256,
                "latest GTFS feed is already active"
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
            &source,
            download_url,
            &content_sha256,
            file_bytes,
            &file_path,
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
    pub async fn import_feed_version(&self, version_id: i64) -> Result<ImportFeedVersionOutcome> {
        let version = postgres::fetch_version_info(&self.pool, version_id).await?;

        if matches!(version.status.as_str(), "imported" | "active") {
            info!(
                version_id,
                status = %version.status,
                "GTFS feed version import is already stable"
            );
            return Ok(ImportFeedVersionOutcome::AlreadyStable {
                version: version.into(),
            });
        }

        info!(
            version_id,
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
                let version = postgres::fetch_version_info(&self.pool, version_id).await?;
                if matches!(version.status.as_str(), "imported") {
                    info!(version_id, "imported GTFS feed version");
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
        version_id: i64,
    ) -> Result<PromoteFeedVersionOutcome> {
        match postgres::promote_feed_version(&self.pool, version_id).await? {
            PromoteVersionOutcome::AlreadyActive(version) => {
                info!(version_id, "GTFS feed version is already active");
                Ok(PromoteFeedVersionOutcome::AlreadyActive {
                    version: version.into(),
                })
            }
            PromoteVersionOutcome::CurrentActiveIsNewer(version) => {
                info!(
                    version_id,
                    "skipped GTFS promotion because current active version is newer"
                );
                Ok(PromoteFeedVersionOutcome::CurrentActiveIsNewer {
                    version: version.into(),
                })
            }
            PromoteVersionOutcome::Promoted(version) => {
                info!(version_id, "promoted GTFS feed version");
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
            Some(self.import_feed_version(prepared_version.id).await?)
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
                self.try_promote_latest_feed_version(candidate_version.id)
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
    pub async fn sync_sources(&self) -> Result<Vec<SyncSourceOutcome>> {
        let source_slugs = postgres::list_feed_source_slugs(&self.pool).await?;
        let mut outcomes = Vec::with_capacity(source_slugs.len());

        for source_slug in source_slugs {
            info!(%source_slug, "syncing GTFS feed source");
            outcomes.push(self.sync_source(&source_slug).await?);
        }

        Ok(outcomes)
    }
}

/// Upserts feed sources from a seed file model.
pub async fn upsert_feed_sources_seed(database_url: &str, seed: &SeedFile) -> Result<()> {
    let pool = PgPool::connect(database_url)
        .await
        .context("failed to connect to GTFS Postgres database")?;

    postgres::upsert_feed_sources_seed(&pool, seed).await
}

pub async fn sync_tiling(database_url: &str) -> Result<Vec<SyncTilingSourceOutcome>> {
    let pool = PgPool::connect(database_url)
        .await
        .context("failed to connect to GTFS Postgres database")?;

    let source_slugs = postgres::list_feed_source_slugs(&pool).await?;
    let mut outcomes = Vec::with_capacity(source_slugs.len());

    for source_slug in source_slugs {
        info!(%source_slug, "syncing GTFS tiling for feed source");
        outcomes.push(postgres::sync_tiling_for_source(&pool, &source_slug).await?);
    }

    Ok(outcomes)
}

pub async fn export_tiling(
    database_url: &str,
    source_slug: Option<&str>,
    output_file: &Path,
) -> Result<ExportTilingOutcome> {
    let pool = PgPool::connect(database_url)
        .await
        .context("failed to connect to GTFS Postgres database")?;

    let tile_count = write_pmtiles(&pool, source_slug, output_file).await?;

    Ok(ExportTilingOutcome {
        source_slug: source_slug.map(str::to_owned),
        tile_count,
        output_file: output_file.to_owned(),
    })
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

async fn download_feed(download_url: &str) -> Result<Vec<u8>> {
    let response = reqwest::Client::new()
        .get(download_url)
        .header(
            reqwest::header::USER_AGENT,
            concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await
        .with_context(|| format!("failed to request GTFS feed {}", download_url))?
        .error_for_status()
        .with_context(|| format!("GTFS feed request failed for {}", download_url))?;

    let body = response
        .bytes()
        .await
        .with_context(|| format!("failed to read GTFS feed {}", download_url))?;

    Ok(body.to_vec())
}

fn sha256_hex(body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body);
    format!("{:x}", hasher.finalize())
}

fn feed_artifact_path(source_slug: &str, content_sha256: &str) -> String {
    format!("feed-sources/{source_slug}/versions/{content_sha256}.zip")
}

async fn write_pmtiles(
    pool: &PgPool,
    source_slug: Option<&str>,
    output_file: &Path,
) -> Result<i64> {
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

    let mut tile_stream = postgres::stream_tiling_export_tiles(pool, source_slug)?;
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

    Ok(tile_count)
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

impl From<FeedVersionInfo> for FeedVersion {
    fn from(record: FeedVersionInfo) -> Self {
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
