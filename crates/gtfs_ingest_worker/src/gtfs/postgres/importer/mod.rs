mod encoding;
mod specs;

use crate::utils::postgres_binary_copy::BinaryCopyInWriter;
use anyhow::{Context, Result, bail};
use encoding::GtfsZip;
use specs::{IMPORT_SPECS, ImportSpec, write_records};
use sqlx::{Postgres, Transaction};
use std::io::{Read, Seek};
use tracing::info;
use zip::ZipArchive;

/// Parses a GTFS ZIP and replaces rows for the given version inside the caller's transaction.
pub async fn import_feed_version<R>(
    tx: &mut Transaction<'_, Postgres>,
    version_id: i64,
    zip_archive: ZipArchive<R>,
) -> Result<()>
where
    R: Read + Seek,
{
    let mut gtfs_zip = GtfsZip::new(zip_archive).context("failed to inspect GTFS ZIP")?;

    delete_existing_derived_gtfs_rows(tx, version_id).await?;
    delete_existing_gtfs_rows(tx, version_id).await?;
    copy_gtfs_to_tables(tx, &mut gtfs_zip, version_id).await?;
    update_derived_gtfs_tables(tx, version_id).await?;

    Ok(())
}

async fn copy_gtfs_to_tables<R>(
    tx: &mut Transaction<'_, Postgres>,
    gtfs_zip: &mut GtfsZip<R>,
    version_id: i64,
) -> Result<()>
where
    R: Read + Seek,
{
    for spec in IMPORT_SPECS {
        copy_records_to_table(tx, spec, gtfs_zip, version_id).await?;
    }

    Ok(())
}

async fn copy_records_to_table<R>(
    tx: &mut Transaction<'_, Postgres>,
    spec: &ImportSpec,
    gtfs_zip: &mut GtfsZip<R>,
    version_id: i64,
) -> Result<()>
where
    R: Read + Seek,
{
    if !gtfs_zip.contains(spec) {
        if spec.required {
            bail!("GTFS artifact is missing required file {}", spec.file_name);
        }

        info!(
            version_id,
            target_table = spec.target_table,
            file_name = spec.file_name,
            rows = 0_u64,
            "skipped missing optional GTFS file"
        );
        return Ok(());
    }

    let copy_sql = format!(
        "COPY {} ({}) FROM STDIN WITH (FORMAT binary)",
        spec.target_table,
        spec.columns.join(", ")
    );

    let copy = tx
        .copy_in_raw(&copy_sql)
        .await
        .with_context(|| format!("failed to start COPY for {}", spec.target_table))?;

    let mut writer = BinaryCopyInWriter::new(copy, spec.columns.len())
        .with_context(|| format!("failed to create binary COPY writer for {}", spec.name))?;

    if let Err(error) = write_records(&mut writer, spec, gtfs_zip, version_id).await {
        let abort_message = error.to_string();
        let _ = writer.abort(abort_message).await;
        return Err(error).with_context(|| {
            format!(
                "failed to stream binary COPY rows for {}",
                spec.target_table
            )
        });
    }

    let row_count = writer.row_count();
    let copied_rows = writer
        .finish()
        .await
        .with_context(|| format!("failed to finish COPY for {}", spec.target_table))?;

    if copied_rows != row_count {
        bail!(
            "COPY row count mismatch for {}: parser produced {}, Postgres accepted {}",
            spec.target_table,
            row_count,
            copied_rows
        );
    }

    info!(
        version_id,
        target_table = spec.target_table,
        rows = copied_rows,
        "copied GTFS rows to durable table"
    );

    Ok(())
}

async fn delete_existing_gtfs_rows(
    tx: &mut Transaction<'_, Postgres>,
    version_id: i64,
) -> Result<()> {
    for table_name in IMPORT_SPECS.iter().map(|spec| spec.target_table) {
        sqlx::query(&format!("DELETE FROM {table_name} WHERE version_id = $1"))
            .bind(version_id)
            .execute(&mut **tx)
            .await
            .with_context(|| format!("failed to clear previous rows from {}", table_name))?;
    }

    Ok(())
}

async fn delete_existing_derived_gtfs_rows(
    tx: &mut Transaction<'_, Postgres>,
    version_id: i64,
) -> Result<()> {
    sqlx::query("DELETE FROM gtfs.stop_route_refs WHERE version_id = $1")
        .bind(version_id)
        .execute(&mut **tx)
        .await
        .context("failed to clear previous rows from gtfs.stop_route_refs")?;

    Ok(())
}

async fn update_derived_gtfs_tables(
    tx: &mut Transaction<'_, Postgres>,
    version_id: i64,
) -> Result<()> {
    let rows = sqlx::query(
        r#"
        INSERT INTO gtfs.stop_route_refs (
            version_id,
            stop_id,
            route_id
        )
        SELECT DISTINCT
            stop_time.version_id,
            stop_time.stop_id,
            trip.route_id
        FROM gtfs.stop_times stop_time
        JOIN gtfs.trips trip
          ON trip.version_id = stop_time.version_id
         AND trip.trip_id = stop_time.trip_id
        WHERE stop_time.version_id = $1
          AND stop_time.stop_id IS NOT NULL
          AND trip.route_id IS NOT NULL
        "#,
    )
    .bind(version_id)
    .execute(&mut **tx)
    .await
    .context("failed to update derived GTFS stop-trip references")?
    .rows_affected();

    info!(
        version_id,
        target_table = "gtfs.stop_route_refs",
        rows,
        "updated derived GTFS table"
    );

    Ok(())
}
