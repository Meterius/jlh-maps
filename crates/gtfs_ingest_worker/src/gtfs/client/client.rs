use crate::gtfs::artifact_store::{
    ArtifactStore, ArtifactStoreConfig as InternalArtifactStoreConfig, ArtifactStoreConfig,
};
use crate::gtfs::client::models::{AggregatedStop, Route};
use crate::gtfs::postgres;
use anyhow::{Context, Result};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

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
        let pool = PgPoolOptions::new()
            .max_connections(32)
            .connect(&config.database_url)
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
            http_client: reqwest::Client::builder()
                .cookie_store(true)
                .build()
                .context("failed to build GTFS HTTP client")?,
        })
    }
}

/// Runtime dependencies for the GTFS client.
#[derive(Debug, Clone)]
pub struct GtfsConfig {
    /// Postgres connection string for GTFS metadata and schedule rows.
    pub database_url: String,
}

#[derive(Debug, Clone)]
pub struct GtfsClient {
    pub(super) pool: PgPool,
}

impl GtfsClient {
    pub async fn connect(config: GtfsConfig) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(32)
            .connect(&config.database_url)
            .await
            .context("failed to connect to GTFS Postgres database")?;

        Ok(Self { pool })
    }

    pub async fn fetch_aggregated_stop(
        &self,
        version_id: i64,
        stop_id: &str,
    ) -> Result<Option<AggregatedStop>> {
        let rows = postgres::fetch_aggregated_stop(&self.pool, version_id, stop_id).await?;
        Ok(AggregatedStop::from_postgres_rows(rows))
    }

    pub async fn fetch_route(&self, version_id: i64, route_id: &str) -> Result<Option<Route>> {
        postgres::fetch_route(&self.pool, version_id, route_id)
            .await
            .map(|route| route.map(Route::from))
    }
}
