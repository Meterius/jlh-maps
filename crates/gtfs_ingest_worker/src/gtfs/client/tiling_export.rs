use super::client::GtfsIngestClient;
use crate::gtfs::postgres;
use crate::gtfs::postgres::{GTFS_TILING_EXPORT_CHUNK_ZOOM, GTFS_TILING_ZOOM};
use anyhow::{Context, Result, bail};
use futures_util::stream::BoxStream;
use futures_util::{StreamExt, TryStreamExt};
use pmtiles::{PmTilesWriter, TileCoord, TileType};
use serde_json::json;
use sqlx::PgPool;
use std::io::{Seek, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::info;

#[derive(Debug, Clone)]
pub struct ExportTilingOutcome {
    pub source_slug: Option<String>,
    pub tile_count: i64,
}

impl GtfsIngestClient {
    pub async fn export_tiling<W>(
        &self,
        source_slug: Option<&str>,
        writer: W,
        parallelism: usize,
    ) -> anyhow::Result<ExportTilingOutcome>
    where
        W: Write + Seek,
    {
        let parallelism = parallelism.max(1);

        let metadata = tiling_metadata(source_slug)?;

        let mut writer = PmTilesWriter::new(TileType::Mvt)
            .min_zoom(to_zoom_u8(GTFS_TILING_ZOOM)?)
            .max_zoom(to_zoom_u8(GTFS_TILING_ZOOM)?)
            .center_zoom(to_zoom_u8(GTFS_TILING_ZOOM)?)
            .center(0.0, 0.0)
            .bounds(-180.0, -85.051_128_78, 180.0, 85.051_128_78)
            .metadata(&metadata)
            .create(writer)
            .context("failed to initialize GTFS PMTiles writer")?;

        info!(
            source_slug = %source_slug.unwrap_or("<all>"),
            parallelism,
            chunk_zoom = GTFS_TILING_EXPORT_CHUNK_ZOOM,
            export_zoom = GTFS_TILING_ZOOM,
            "streaming GTFS PMTiles export tiles"
        );

        let mut tile_stream = stream_tiling_export_tiles(&self.pool, source_slug, parallelism);
        let mut tile_count = 0_i64;

        while let Some(tile) = tile_stream
            .try_next()
            .await
            .context("failed to stream GTFS MVT tiles for export")?
        {
            let coord = TileCoord::new(
                to_zoom_u8(tile.z)?,
                to_tile_u32(tile.x)?,
                to_tile_u32(tile.y)?,
            )
            .context("failed to create GTFS PMTiles tile coordinate")?;

            writer.add_tile(coord, &tile.tile).with_context(|| {
                format!(
                    "failed to write GTFS PMTiles tile {}/{}/{}",
                    tile.z, tile.x, tile.y
                )
            })?;

            tile_count += 1;
        }

        writer
            .finalize()
            .context("failed to finalize GTFS PMTiles archive")?;

        if tile_count == 0 {
            match source_slug {
                Some(source_slug) => bail!(
                    "GTFS tiling for source {} has no tiles to export",
                    source_slug
                ),
                None => bail!("GTFS tiling has no tiles to export"),
            }
        }

        info!(
            source_slug = %source_slug.unwrap_or("<all>"),
            tile_count,
            "finished GTFS PMTiles export"
        );

        Ok(ExportTilingOutcome {
            source_slug: source_slug.map(str::to_owned),
            tile_count,
        })
    }
}

fn stream_tiling_export_tiles<'a>(
    pool: &'a PgPool,
    source_slug: Option<&'a str>,
    parallelism: usize,
) -> BoxStream<'a, Result<postgres::TilingExportTile, sqlx::Error>> {
    let max_processed_chunk_index = Arc::new(AtomicUsize::new(0));
    let chunk_stream = postgres::stream_tile_ids_intersecting_geometry(
        pool,
        source_slug,
        GTFS_TILING_EXPORT_CHUNK_ZOOM,
    )
    .map_ok({
        let max_processed_chunk_index = Arc::clone(&max_processed_chunk_index);
        move |tile_id| {
            let (chunk_index, total_chunks) = chunk_progress_index(&tile_id);
            let previous_max = max_processed_chunk_index.fetch_max(chunk_index, Ordering::AcqRel);
            let processed_chunks = previous_max.max(chunk_index);
            let remaining_chunks = total_chunks.saturating_sub(processed_chunks);

            info!(
                source_slug = %source_slug.unwrap_or("<all>"),
                chunk_z = tile_id.z,
                chunk_x = tile_id.x,
                chunk_y = tile_id.y,
                processed_chunks,
                remaining_chunks,
                total_chunks,
                progress = %format_args!("{processed_chunks}/{total_chunks}"),
                "processing GTFS PMTiles export chunk"
            );

            postgres::stream_export_tiles(pool, source_slug, tile_id).boxed()
        }
    });

    if parallelism <= 1 {
        chunk_stream.try_flatten().boxed()
    } else {
        chunk_stream
            .try_flatten_unordered(Some(parallelism))
            .boxed()
    }
}

fn chunk_progress_index(tile_id: &postgres::TilingExportTileId) -> (usize, usize) {
    let z = tile_id.z.max(0) as u32;
    let tile_axis_count = 1_usize << z;
    let chunk_index = tile_id.y.max(0) as usize * tile_axis_count + tile_id.x.max(0) as usize + 1;
    let total_chunks = tile_axis_count * tile_axis_count;

    (chunk_index, total_chunks)
}

fn tiling_metadata(source_slug: Option<&str>) -> anyhow::Result<String> {
    Ok(json!({
        "name": "gtfs",
        "description": "GTFS schedule stop vector tiles",
        "version": source_slug.unwrap_or("gtfs"),
        "vector_layers": [
            {
                "id": "stops",
                "description": "GTFS stop and station points",
                "fields": {
                    "source_slug": "String",
                    "stop_id": "String",
                    "stop_code": "String",
                    "stop_name": "String",
                    "location_type": "Number",
                    "wheelchair_boarding": "Number",
                    "platform_code": "String"
                }
            }
        ]
    })
    .to_string())
}

fn to_zoom_u8(zoom: i32) -> anyhow::Result<u8> {
    zoom.try_into()
        .with_context(|| format!("invalid PMTiles zoom {}", zoom))
}

fn to_tile_u32(value: i32) -> anyhow::Result<u32> {
    value
        .try_into()
        .with_context(|| format!("invalid PMTiles tile coordinate {}", value))
}
