use super::{
    importer,
    model::{
        FeedAggregatedStop, FeedRoute, FeedSourceDownloadInfo, FeedVersionDownloadInfo,
        FeedVersionImportInfo,
    },
};
use crate::gtfs::postgres::locking::{lock_feed_source, lock_feed_version};
use crate::model::SeedFile;
use anyhow::{Context, Result, bail};
use sqlx::{PgPool, Postgres, Transaction};
use std::io::{Read, Seek};
use zip::ZipArchive;

// Queries

pub async fn list_feed_source_slugs(pool: &PgPool) -> Result<Vec<String>> {
    let rows = sqlx::query_as::<_, (String,)>(
        r#"
        SELECT slug
        FROM gtfs_meta.feed_sources
        ORDER BY slug
        "#,
    )
    .fetch_all(pool)
    .await
    .context("failed to list GTFS feed source slugs")?;

    Ok(rows.into_iter().map(|(slug,)| slug).collect())
}

pub async fn fetch_feed_source_download_info(
    pool: &PgPool,
    slug: &str,
) -> Result<Option<FeedSourceDownloadInfo>> {
    let row = sqlx::query_as::<_, FeedSourceDownloadInfo>(
        r#"
        SELECT id, slug, direct_download_url
        FROM gtfs_meta.feed_sources
        WHERE slug = $1
        "#,
    )
    .bind(slug)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("failed to fetch GTFS source {}", slug))?;

    Ok(row)
}

pub async fn fetch_active_version_download_info(
    pool: &PgPool,
    source_id: i64,
) -> Result<Option<FeedVersionDownloadInfo>> {
    let row = sqlx::query_as::<_, FeedVersionDownloadInfo>(
        r#"
        SELECT
            version.id,
            version.content_sha256,
            version.http_etag,
            version.http_last_modified
        FROM gtfs_meta.feed_sources source
        JOIN gtfs_meta.feed_versions version ON version.id = source.active_version_id
        WHERE source.id = $1
        "#,
    )
    .bind(source_id)
    .fetch_optional(pool)
    .await
    .context("failed to inspect active GTFS version download info")?;

    Ok(row)
}

pub async fn fetch_version_import_info<'e, E>(
    executor: E,
    version_id: i64,
) -> Result<FeedVersionImportInfo>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, FeedVersionImportInfo>(
        r#"
        SELECT id, source_id, download_url, content_sha256, file_bytes, file_path, status
        FROM gtfs_meta.feed_versions
        WHERE id = $1
        "#,
    )
    .bind(version_id)
    .fetch_one(executor)
    .await
    .with_context(|| format!("failed to fetch GTFS version {}", version_id))
}

pub async fn fetch_aggregated_stop(
    pool: &PgPool,
    version_id: i64,
    stop_id: &str,
) -> Result<Vec<FeedAggregatedStop>> {
    sqlx::query_as::<_, FeedAggregatedStop>(
        r#"
        WITH RECURSIVE stop_tree AS (
            SELECT
                stop.version_id,
                stop.stop_id,
                stop.stop_code,
                stop.stop_name,
                stop.stop_desc,
                stop.stop_lat,
                stop.stop_lon,
                stop.zone_id,
                stop.stop_url,
                stop.location_type,
                stop.parent_station,
                stop.wheelchair_boarding,
                stop.platform_code,
                0::INTEGER AS depth,
                ARRAY[stop.stop_id] AS path
            FROM gtfs.stops stop
            WHERE stop.version_id = $1
              AND stop.stop_id = $2

            UNION ALL

            SELECT
                child.version_id,
                child.stop_id,
                child.stop_code,
                child.stop_name,
                child.stop_desc,
                child.stop_lat,
                child.stop_lon,
                child.zone_id,
                child.stop_url,
                child.location_type,
                child.parent_station,
                child.wheelchair_boarding,
                child.platform_code,
                parent.depth + 1 AS depth,
                parent.path || child.stop_id AS path
            FROM gtfs.stops child
            JOIN stop_tree parent
              ON parent.version_id = child.version_id
             AND parent.stop_id = child.parent_station
            WHERE NOT child.stop_id = ANY(parent.path)
        )
        SELECT
            stop_tree.version_id,
            stop_tree.stop_id,
            stop_tree.stop_code,
            stop_tree.stop_name,
            stop_tree.stop_desc,
            stop_tree.stop_lat,
            stop_tree.stop_lon,
            stop_tree.zone_id,
            stop_tree.stop_url,
            stop_tree.location_type,
            stop_tree.parent_station,
            stop_tree.wheelchair_boarding,
            stop_tree.platform_code,
            COALESCE(route_refs.route_ids, ARRAY[]::TEXT[]) AS route_ids,
            stop_tree.depth
        FROM stop_tree
        LEFT JOIN gtfs.stop_route_agg_refs route_refs
          ON route_refs.version_id = stop_tree.version_id
         AND route_refs.stop_id = stop_tree.stop_id
        ORDER BY stop_tree.depth, stop_tree.stop_id
        "#,
    )
    .bind(version_id)
    .bind(stop_id)
    .fetch_all(pool)
    .await
    .with_context(|| {
        format!(
            "failed to fetch aggregated GTFS stop {}/{}",
            version_id, stop_id
        )
    })
}

pub async fn fetch_route(
    pool: &PgPool,
    version_id: i64,
    route_id: &str,
) -> Result<Option<FeedRoute>> {
    sqlx::query_as::<_, FeedRoute>(
        r#"
        SELECT
            version_id,
            route_id,
            agency_id,
            route_short_name,
            route_long_name,
            route_desc,
            route_type,
            route_url,
            route_color,
            route_text_color
        FROM gtfs.routes
        WHERE version_id = $1
          AND route_id = $2
        "#,
    )
    .bind(version_id)
    .bind(route_id)
    .fetch_optional(pool)
    .await
    .with_context(|| format!("failed to fetch GTFS route {}/{}", version_id, route_id))
}

/// Like `fetch_version_import_info`, but locks via `FOR UPDATE`.
async fn fetch_version_import_info_for_update(
    tx: &mut Transaction<'_, Postgres>,
    version_id: i64,
) -> Result<FeedVersionImportInfo> {
    sqlx::query_as::<_, FeedVersionImportInfo>(
        r#"
        SELECT id, source_id, download_url, content_sha256, file_bytes, file_path, status
        FROM gtfs_meta.feed_versions
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(version_id)
    .fetch_one(&mut **tx)
    .await
    .with_context(|| format!("failed to lock GTFS version {}", version_id))
}

// Mutators

pub async fn upsert_feed_sources_seed(
    pool: &PgPool,
    seed: &SeedFile,
    delete_existing: bool,
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start GTFS feed source seed transaction")?;

    if delete_existing {
        let seed_slugs = seed
            .sources
            .iter()
            .map(|source| source.slug.clone())
            .collect::<Vec<_>>();

        sqlx::query(
            r#"
            DELETE FROM gtfs_meta.feed_sources
            WHERE NOT (slug = ANY($1::TEXT[]))
            "#,
        )
        .bind(seed_slugs)
        .execute(&mut *tx)
        .await
        .context("failed to delete GTFS feed sources missing from seed")?;
    }

    for source in seed.sources.iter() {
        sqlx::query(
            r#"
            INSERT INTO gtfs_meta.feed_sources (
                slug,
                name,
                source_url,
                direct_download_url,
                license_url,
                attribution,
                created_at,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, now(), now())
            ON CONFLICT (slug) DO UPDATE SET
                name = EXCLUDED.name,
                source_url = EXCLUDED.source_url,
                direct_download_url = EXCLUDED.direct_download_url,
                license_url = EXCLUDED.license_url,
                attribution = EXCLUDED.attribution,
                updated_at = now()
            "#,
        )
        .bind(&source.slug)
        .bind(&source.name)
        .bind(&source.source_url)
        .bind(&source.direct_download_url)
        .bind(&source.license_url)
        .bind(&source.attribution)
        .execute(&mut *tx)
        .await
        .with_context(|| format!("failed to upsert GTFS source {}", source.slug))?;
    }

    tx.commit()
        .await
        .context("failed to commit GTFS feed source seed transaction")?;

    Ok(())
}

pub struct CreateDownloadVersionInput<'a> {
    pub source: &'a FeedSourceDownloadInfo,
    pub download_url: &'a str,
    pub content_sha256: &'a str,
    pub file_bytes: i64,
    pub file_path: &'a str,
    pub http_etag: Option<&'a str>,
    pub http_last_modified: Option<&'a str>,
}

/// Creates a new feed version initialized to the downloaded state.
/// - Expects artifact to have already been stored
pub async fn create_downloaded_version(
    pool: &PgPool,
    input: CreateDownloadVersionInput<'_>,
) -> Result<FeedVersionImportInfo> {
    sqlx::query_as::<_, FeedVersionImportInfo>(
        r#"
        INSERT INTO gtfs_meta.feed_versions (
            source_id,
            download_url,
            content_sha256,
            file_bytes,
            file_path,
            http_etag,
            http_last_modified,
            status,
            error_message,
            fetched_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, 'downloaded', NULL, now())
        RETURNING id, source_id, download_url, content_sha256, file_bytes, file_path, status
        "#,
    )
    .bind(input.source.id)
    .bind(input.download_url)
    .bind(input.content_sha256)
    .bind(input.file_bytes)
    .bind(input.file_path)
    .bind(input.http_etag)
    .bind(input.http_last_modified)
    .fetch_one(pool)
    .await
    .context("failed to insert downloaded GTFS feed version")
}

pub async fn update_version_http_download_info(
    pool: &PgPool,
    version_id: i64,
    http_etag: Option<&str>,
    http_last_modified: Option<&str>,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE gtfs_meta.feed_versions
        SET http_etag = COALESCE($2, http_etag),
            http_last_modified = COALESCE($3, http_last_modified)
        WHERE id = $1
        "#,
    )
    .bind(version_id)
    .bind(http_etag)
    .bind(http_last_modified)
    .execute(pool)
    .await
    .context("failed to update GTFS feed version HTTP cache validators")?;

    Ok(())
}

/// Imports the feed version archive file into the database, removing any entries for the feed version.
/// - Performs a no-op if the version is not in `active` or `imported` state
/// - Errors if the version is not in `downloaded` or `import_failed` state
pub async fn import_feed_version_from_zip<R>(
    pool: &PgPool,
    version_id: i64,
    zip_archive: ZipArchive<R>,
) -> Result<()>
where
    R: Read + Seek,
{
    let mut tx = pool
        .begin()
        .await
        .context("failed to start GTFS import transaction")?;

    lock_feed_version(&mut tx, version_id).await?;
    sqlx::query("SET LOCAL synchronous_commit = OFF")
        .execute(&mut *tx)
        .await
        .context("failed to set import transaction throughput options")?;

    let version = fetch_version_import_info_for_update(&mut tx, version_id).await?;
    match version.status.as_str() {
        "imported" | "active" => {
            tx.commit()
                .await
                .context("failed to commit GTFS no-op import transaction")?;
            return Ok(());
        }
        "downloaded" | "import_failed" => {}
        other => bail!(
            "GTFS version {} cannot be imported from status {}",
            version_id,
            other
        ),
    }

    importer::import_feed_version(&mut tx, version_id, zip_archive).await?;

    let _ = sqlx::query(
        r#"
        UPDATE gtfs_meta.feed_versions
        SET status = 'imported',
            error_message = NULL,
            imported_at = now()
        WHERE id = $1
        "#,
    )
    .bind(version_id)
    .execute(&mut *tx)
    .await
    .context("failed to mark GTFS version as imported")?;

    tx.commit()
        .await
        .context("failed to commit GTFS import transaction")?;

    Ok(())
}

/// Marks a feed version state as `import_failed`
/// - Performs a no-op if the version is in `active` or in the `imported` state.
pub async fn mark_import_failed(pool: &PgPool, version_id: i64, error_message: &str) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE gtfs_meta.feed_versions
        SET status = 'import_failed',
            error_message = $2
        WHERE id = $1
          AND status NOT IN ('active', 'imported')
        "#,
    )
    .bind(version_id)
    .bind(error_message)
    .execute(pool)
    .await
    .context("failed to mark GTFS import as failed")?;

    Ok(())
}

#[derive(Debug, Clone)]
pub enum PromoteVersionOutcome {
    Promoted(FeedVersionImportInfo),
    CurrentActiveIsNewer(FeedVersionImportInfo),
    AlreadyActive(FeedVersionImportInfo),
}

/// Promotes a feed version to the active state, handling the de-promotion of a previous active version.
/// - Will not promote if the active version was fetched more recently than the promotion candidate
pub async fn promote_feed_version(pool: &PgPool, version_id: i64) -> Result<PromoteVersionOutcome> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start GTFS promotion transaction")?;

    let source_id = fetch_version_import_info(&mut *tx, version_id)
        .await?
        .source_id;

    lock_feed_source(&mut tx, source_id).await?;
    lock_feed_version(&mut tx, version_id).await?;

    let version = fetch_version_import_info_for_update(&mut tx, version_id).await?;
    match version.status.as_str() {
        "active" => {
            tx.commit()
                .await
                .context("failed to commit GTFS already-active promotion transaction")?;
            return Ok(PromoteVersionOutcome::AlreadyActive(version));
        }
        "imported" => {}
        other => bail!(
            "GTFS version {} cannot be promoted from status {}",
            version_id,
            other
        ),
    }

    let (should_promote,) = sqlx::query_as::<_, (bool,)>(
        r#"
        SELECT NOT EXISTS (
            SELECT 1
            FROM gtfs_meta.feed_sources source
            JOIN gtfs_meta.feed_versions active_version
              ON active_version.id = source.active_version_id
            JOIN gtfs_meta.feed_versions target_version
              ON target_version.id = $2
            WHERE source.id = $1
              AND active_version.fetched_at > target_version.fetched_at
        )
        "#,
    )
    .bind(version.source_id)
    .bind(version.id)
    .fetch_one(&mut *tx)
    .await
    .context("failed to compare GTFS active and candidate versions")?;

    if !should_promote {
        tx.commit()
            .await
            .context("failed to commit skipped GTFS promotion transaction")?;
        return Ok(PromoteVersionOutcome::CurrentActiveIsNewer(version));
    }

    sqlx::query(
        r#"
        UPDATE gtfs_meta.feed_versions
        SET status = 'imported'
        WHERE source_id = $1
          AND id <> $2
          AND status = 'active'
        "#,
    )
    .bind(version.source_id)
    .bind(version.id)
    .execute(&mut *tx)
    .await
    .context("failed to demote previous active GTFS version")?;

    let promoted = sqlx::query_as::<_, FeedVersionImportInfo>(
        r#"
        UPDATE gtfs_meta.feed_versions
        SET status = 'active',
            promoted_at = now(),
            error_message = NULL
        WHERE id = $1
        RETURNING id, source_id, download_url, content_sha256, file_bytes, file_path, status
        "#,
    )
    .bind(version.id)
    .fetch_one(&mut *tx)
    .await
    .context("failed to mark GTFS version as active")?;

    sqlx::query(
        r#"
        UPDATE gtfs_meta.feed_sources
        SET active_version_id = $2,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(version.source_id)
    .bind(version.id)
    .execute(&mut *tx)
    .await
    .context("failed to point GTFS source at active version")?;

    tx.commit()
        .await
        .context("failed to commit GTFS promotion transaction")?;

    Ok(PromoteVersionOutcome::Promoted(promoted))
}
