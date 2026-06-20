use super::client::GtfsIngestClient;
use crate::gtfs::postgres;
use crate::model::SeedFile;
use anyhow::Result;

impl GtfsIngestClient {
    /// Upserts feed sources from a seed file model.
    pub async fn upsert_feed_sources_seed(
        &self,
        seed: &SeedFile,
        delete_existing: bool,
    ) -> Result<()> {
        postgres::upsert_feed_sources_seed(&self.pool, seed, delete_existing).await
    }
}
