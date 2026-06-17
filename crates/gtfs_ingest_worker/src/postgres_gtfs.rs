use crate::model::SeedFile;
use anyhow::Context;
use sqlx::PgPool;

pub async fn upsert_feed_sources_seed(pool: &PgPool, seed: &SeedFile) -> anyhow::Result<()> {
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
