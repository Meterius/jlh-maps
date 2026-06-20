use crate::gtfs::postgres;
use crate::gtfs::postgres::GTFS_TILING_EXPORT_CHUNK_ZOOM;
use anyhow::Result;
use futures_util::stream::BoxStream;
use futures_util::{StreamExt, TryStreamExt};
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::info;

pub fn stream_tiling_export_tiles<'a>(
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

pub fn tiling_metadata(source_slug: Option<&str>) -> anyhow::Result<String> {
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
