use super::client::GtfsIngestClient;
use super::utils::sync::{
    SyncCommandOutcome, SyncFailure, SyncLogCounters, partition_sync_results,
};
use super::version_lifecycle::{
    ImportFeedVersionOutcome, PrepareLatestFeedVersionOutcome, PromoteFeedVersionOutcome,
    imported_version, prepared_version,
};
use crate::gtfs::postgres;
use anyhow::{Result, bail};
use futures_util::StreamExt;
use futures_util::stream;
use std::sync::Arc;
use tracing::{error, info};

#[derive(Debug, Clone)]
pub struct SyncSourceOutcome {
    pub source_slug: String,
    pub prepared: PrepareLatestFeedVersionOutcome,
    pub imported: Option<ImportFeedVersionOutcome>,
    pub promoted: Option<PromoteFeedVersionOutcome>,
}

pub type SyncSourcesOutcome = SyncCommandOutcome<SyncSourceOutcome>;

impl GtfsIngestClient {
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

        let imported = self
            .import_feed_version(source_slug, prepared_version.id, Some(prepared_version))
            .await?;

        let candidate_version = imported_version(&imported);
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
            imported: Some(imported),
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
