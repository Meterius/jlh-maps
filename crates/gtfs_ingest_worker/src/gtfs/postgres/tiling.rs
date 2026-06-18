use anyhow::{Context, Result};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use tracing::info;
use crate::gtfs::postgres::locking::lock_feed_source;

const MAX_WEB_MERCATOR_LATITUDE: f64 = 85.051_128_78;

#[derive(Debug, FromRow)]
struct TilingSourceState {
    source_id: i64,
    source_slug: String,
    active_version_id: Option<i64>,
    tiled_version_id: Option<i64>,
}

// Syncing

#[derive(Debug, Clone)]
pub enum SyncTilingStatus {
    NoActiveVersion,
    AlreadyCurrent,
    Synced,
}

#[derive(Debug, Clone)]
pub struct SyncTilingSourceOutcome {
    pub previous_tiled_version_id: Option<i64>,
    pub tiled_version_id: Option<i64>,
    pub status: SyncTilingStatus,
}

pub async fn sync_tiling_for_source(
    pool: &PgPool,
    source_slug: &str,
) -> Result<SyncTilingSourceOutcome> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start GTFS tiling transaction")?;

    let source = fetch_tiling_source_state_for_update(&mut tx, source_slug).await?;
    lock_feed_source(&mut tx, source.source_id).await?;

    let previous_tiled_version_id = source.tiled_version_id;
    let Some(active_version_id) = source.active_version_id else {
        delete_source_tiling(&mut tx, source.source_id).await?;
        
        tx.commit()
            .await
            .context("failed to commit GTFS tiling no-active transaction")?;

        return Ok(SyncTilingSourceOutcome {
            previous_tiled_version_id,
            tiled_version_id: None,
            status: SyncTilingStatus::NoActiveVersion,
        });
    };

    if source.tiled_version_id == Some(active_version_id) {
        tx.commit()
            .await
            .context("failed to commit GTFS tiling already-current transaction")?;

        return Ok(SyncTilingSourceOutcome {
            previous_tiled_version_id,
            tiled_version_id: Some(active_version_id),
            status: SyncTilingStatus::AlreadyCurrent,
        });
    }

    delete_source_tiling(&mut tx, source.source_id).await?;
    insert_source_tiling(&mut tx, source.source_id, active_version_id).await?;

    import_source_tiling_data(&mut tx, source.source_id, active_version_id).await?;

    tx.commit()
        .await
        .context("failed to commit GTFS tiling transaction")?;

    info!(
        source_slug = %source.source_slug,
        version_id = active_version_id,
        "synced GTFS stop tiling geometries"
    );

    Ok(SyncTilingSourceOutcome {
        previous_tiled_version_id,
        tiled_version_id: Some(active_version_id),
        status: SyncTilingStatus::Synced,
    })
}

async fn fetch_tiling_source_state_for_update(
    tx: &mut Transaction<'_, Postgres>,
    source_slug: &str,
) -> Result<TilingSourceState> {
    sqlx::query_as::<_, TilingSourceState>(
        r#"
        SELECT
            source.id AS source_id,
            source.slug AS source_slug,
            active_version.id AS active_version_id,
            tiling.version_id AS tiled_version_id
        FROM gtfs_meta.feed_sources source
        LEFT JOIN gtfs_meta.feed_versions active_version
          ON active_version.id = source.active_version_id
         AND active_version.status = 'active'
        LEFT JOIN gtfs_tiling.source_tilings tiling
          ON tiling.source_id = source.id
        WHERE source.slug = $1
        FOR UPDATE OF source
        "#,
    )
    .bind(source_slug)
    .fetch_optional(&mut **tx)
    .await
    .with_context(|| {
        format!(
            "failed to fetch GTFS tiling state for source {}",
            source_slug
        )
    })?
    .with_context(|| format!("GTFS feed source {} does not exist", source_slug))
}

//

async fn delete_source_tiling(tx: &mut Transaction<'_, Postgres>, source_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM gtfs_tiling.source_tilings WHERE source_id = $1")
        .bind(source_id)
        .execute(&mut **tx)
        .await
        .context("failed to delete previous GTFS tiling rows")?;

    Ok(())
}

async fn insert_source_tiling(
    tx: &mut Transaction<'_, Postgres>,
    source_id: i64,
    version_id: i64,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO gtfs_tiling.source_tilings (
            source_id,
            version_id,
            generated_at
        )
        VALUES ($1, $2, now())
        "#,
    )
    .bind(source_id)
    .bind(version_id)
    .execute(&mut **tx)
    .await
    .context("failed to create GTFS tiling state row")?;

    Ok(())
}

// Importing

async fn import_source_tiling_data(
    tx: &mut Transaction<'_, Postgres>,
    source_id: i64,
    version_id: i64,
) -> Result<()> {
    import_source_tiling_stop_points(tx, source_id, version_id).await?;
    Ok(())
}

async fn import_source_tiling_stop_points(
    tx: &mut Transaction<'_, Postgres>,
    source_id: i64,
    version_id: i64,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO gtfs_tiling.stop_points (
            source_id,
            version_id,
            stop_id,
            stop_code,
            stop_name,
            stop_desc,
            location_type,
            wheelchair_boarding,
            platform_code,
            geom
        )
        SELECT
            $1,
            $2,
            stop_id,
            stop_code,
            stop_name,
            stop_desc,
            location_type,
            wheelchair_boarding,
            platform_code,
            ST_SetSRID(ST_MakePoint(stop_lon, stop_lat), 4326)
        FROM gtfs.stops
        WHERE version_id = $2
          AND stop_lon BETWEEN -180.0 AND 180.0
          AND stop_lat BETWEEN $3 AND $4
        "#,
    )
    .bind(source_id)
    .bind(version_id)
    .bind(-MAX_WEB_MERCATOR_LATITUDE)
    .bind(MAX_WEB_MERCATOR_LATITUDE)
    .execute(&mut **tx)
    .await
    .context("failed to materialize GTFS stop points for tiling")?;
    
    Ok(())
}
