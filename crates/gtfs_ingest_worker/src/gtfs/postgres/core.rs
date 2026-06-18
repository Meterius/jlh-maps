use super::{
    importer,
    model::{FeedSourceDownloadInfo, FeedVersionInfo},
};
use crate::model::SeedFile;
use anyhow::{Context, Result, bail};
use sqlx::{PgPool, Postgres, Transaction};
use crate::gtfs::postgres::locking::{lock_feed_source, lock_feed_version};

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

pub async fn fetch_active_version_content_hash(
    pool: &PgPool,
    source_id: i64,
) -> Result<Option<String>> {
    let row = sqlx::query_as::<_, (String,)>(
        r#"
        SELECT version.content_sha256
        FROM gtfs_meta.feed_sources source
        JOIN gtfs_meta.feed_versions version ON version.id = source.active_version_id
        WHERE source.id = $1
        "#,
    )
    .bind(source_id)
    .fetch_optional(pool)
    .await
    .context("failed to inspect active GTFS version content hash")?;

    Ok(row.map(|row| row.0))
}

pub async fn fetch_version_info<'e, E>(executor: E, version_id: i64) -> Result<FeedVersionInfo>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, FeedVersionInfo>(
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

/// Like `fetch_version_info`, but locks via `FOR UPDATE`.
async fn fetch_version_info_for_update(
    tx: &mut Transaction<'_, Postgres>,
    version_id: i64,
) -> Result<FeedVersionInfo> {
    sqlx::query_as::<_, FeedVersionInfo>(
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

pub async fn upsert_feed_sources_seed(pool: &PgPool, seed: &SeedFile) -> Result<()> {
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
        .execute(pool)
        .await
        .with_context(|| format!("failed to upsert GTFS source {}", source.slug))?;
    }

    Ok(())
}

/// Creates a new feed version initialized to the downloaded state.
/// - Expects artifact to have already been stored
pub async fn create_downloaded_version(
    pool: &PgPool,
    source: &FeedSourceDownloadInfo,
    download_url: &str,
    content_sha256: &str,
    file_bytes: i64,
    file_path: &str,
) -> Result<FeedVersionInfo> {
    sqlx::query_as::<_, FeedVersionInfo>(
        r#"
        INSERT INTO gtfs_meta.feed_versions (
            source_id,
            download_url,
            content_sha256,
            file_bytes,
            file_path,
            status,
            error_message,
            fetched_at
        )
        VALUES ($1, $2, $3, $4, $5, 'downloaded', NULL, now())
        RETURNING id, source_id, download_url, content_sha256, file_bytes, file_path, status
        "#,
    )
    .bind(source.id)
    .bind(download_url)
    .bind(content_sha256)
    .bind(file_bytes)
    .bind(file_path)
    .fetch_one(pool)
    .await
    .context("failed to insert downloaded GTFS feed version")
}

/// Imports the feed version archive file into the database, removing any entries for the feed version.
/// - Performs a no-op if the version is not in `active` or `imported` state
/// - Errors if the version is not in `downloaded` or `import_failed` state
pub async fn import_feed_version_from_zip(
    pool: &PgPool,
    version_id: i64,
    zip_body: Vec<u8>,
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start GTFS import transaction")?;

    lock_feed_version(&mut tx, version_id).await?;
    sqlx::query("SET LOCAL synchronous_commit = OFF")
        .execute(&mut *tx)
        .await
        .context("failed to set import transaction throughput options")?;

    let version = fetch_version_info_for_update(&mut tx, version_id).await?;
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

    importer::import_feed_version(&mut tx, version_id, zip_body).await?;

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
    Promoted(FeedVersionInfo),
    CurrentActiveIsNewer(FeedVersionInfo),
    AlreadyActive(FeedVersionInfo),
}

/// Promotes a feed version to the active state, handling the de-promotion of a previous active version.
/// - Will not promote if the active version was fetched more recently than the promotion candidate
pub async fn promote_feed_version(pool: &PgPool, version_id: i64) -> Result<PromoteVersionOutcome> {
    let mut tx = pool
        .begin()
        .await
        .context("failed to start GTFS promotion transaction")?;

    let source_id = fetch_version_info(&mut *tx, version_id).await?.source_id;

    lock_feed_source(&mut tx, source_id).await?;
    lock_feed_version(&mut tx, version_id).await?;

    let version = fetch_version_info_for_update(&mut tx, version_id).await?;
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

    let promoted = sqlx::query_as::<_, FeedVersionInfo>(
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
