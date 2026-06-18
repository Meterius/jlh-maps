use anyhow::Context;
use sqlx::{Postgres, Transaction};

pub async fn lock_feed_source(tx: &mut Transaction<'_, Postgres>, source_id: i64) -> anyhow::Result<()> {
    lock_name(tx, &format!("gtfs_feed_source:{source_id}")).await
}

pub async fn lock_feed_version(tx: &mut Transaction<'_, Postgres>, version_id: i64) -> anyhow::Result<()> {
    lock_name(tx, &format!("gtfs_feed_version:{version_id}")).await
}

pub async fn lock_name(tx: &mut Transaction<'_, Postgres>, lock_name: &str) -> anyhow::Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(lock_name)
        .execute(&mut **tx)
        .await
        .with_context(|| format!("failed to acquire advisory lock {}", lock_name))?;

    Ok(())
}
