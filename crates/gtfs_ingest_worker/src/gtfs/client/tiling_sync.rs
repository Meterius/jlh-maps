use super::client::GtfsIngestClient;
use super::utils::sync::{
    SyncCommandOutcome, SyncFailure, SyncLogCounters, partition_sync_results,
};
use crate::gtfs::postgres::{self, SyncTilingSourceOutcome};
use anyhow::Result;
use futures_util::StreamExt;
use futures_util::stream;
use std::sync::Arc;
use tracing::{error, info};

#[derive(Debug, Clone)]
pub struct SyncTilingSourceResult {
    pub source_slug: String,
    pub outcome: SyncTilingSourceOutcome,
}

pub type SyncTilingOutcome = SyncCommandOutcome<SyncTilingSourceResult>;

impl GtfsIngestClient {
    pub async fn sync_tiling(&self, parallelism: usize) -> Result<SyncTilingOutcome> {
        let parallelism = parallelism.max(1);

        let source_slugs = postgres::list_feed_source_slugs(&self.pool).await?;

        info!(
            source_count = source_slugs.len(),
            parallelism, "syncing GTFS tiling sources"
        );

        let sync_log_counters = Arc::new(SyncLogCounters::new(source_slugs.len()));

        let results = stream::iter(source_slugs)
            .map(|source_slug| {
                let pool = &self.pool;
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
}
