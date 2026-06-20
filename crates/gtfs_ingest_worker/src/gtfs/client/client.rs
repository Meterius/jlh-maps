use crate::gtfs::artifact_store::{
    ArtifactStore, ArtifactStoreConfig as InternalArtifactStoreConfig, ArtifactStoreConfig,
};
use anyhow::{Context, Result};
use sqlx::PgPool;

/// Runtime dependencies for the GTFS ingestion client.
#[derive(Debug, Clone)]
pub struct GtfsIngestConfig {
    /// Postgres connection string for GTFS metadata and schedule rows.
    pub database_url: String,
    /// Object-store settings for immutable feed ZIP artifacts.
    pub artifact_store: ArtifactStoreConfig,
}

/// Public facade for GTFS source ingestion and tiling workflows.
#[derive(Debug, Clone)]
pub struct GtfsIngestClient {
    pub(super) pool: PgPool,
    pub(super) artifact_store: ArtifactStore,
    pub(super) http_client: reqwest::Client,
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
}
