use crate::gtfs::postgres::locking::lock_feed_source;
use anyhow::{Context, Result};
use futures_util::Stream;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use tracing::info;

const MAX_WEB_MERCATOR_LATITUDE: f64 = 85.051_128_78;
pub const GTFS_TILING_ZOOM: i32 = 13;
pub const GTFS_TILING_EXPORT_CHUNK_ZOOM: i32 = 7;

// Syncing

#[derive(Debug, Clone)]
pub enum SyncTilingStatus {
    NoActiveVersion,
    AlreadyCurrent,
    Synced,
}

#[derive(Debug, Clone)]
pub struct SyncTilingSourceOutcome {
    pub previous_tiled_version_id: Option<i32>,
    pub tiled_version_id: Option<i32>,
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

    // Every tiling feature table populated below shares this sequence so feature_id
    // stays unique within the source tiling.
    create_tiling_feature_id_sequence(&mut tx).await?;
    import_source_tiling_data(&mut tx, source.source_id, active_version_id).await?;
    drop_tiling_feature_id_sequence(&mut tx).await?;

    tx.commit()
        .await
        .context("failed to commit GTFS tiling transaction")?;

    info!(
        source_slug = %source.source_slug,
        "synced GTFS tiling geometries"
    );

    Ok(SyncTilingSourceOutcome {
        previous_tiled_version_id,
        tiled_version_id: Some(active_version_id),
        status: SyncTilingStatus::Synced,
    })
}

#[derive(Debug, Clone, FromRow)]
pub struct TilingExportTileId {
    pub z: i32,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, FromRow)]
pub struct TilingExportTile {
    pub z: i32,
    pub x: i32,
    pub y: i32,
    pub tile: Vec<u8>,
}

pub fn stream_tile_ids_intersecting_geometry<'a>(
    pool: &'a PgPool,
    source_slug: Option<&'a str>,
    zoom: i32,
) -> impl Stream<Item = std::result::Result<TilingExportTileId, sqlx::Error>> + 'a {
    sqlx::query_as::<_, TilingExportTileId>(
        r#"
        WITH selected_tilings AS (
            SELECT
                tiling.source_id,
                tiling.version_id,
                source.slug AS source_slug
            FROM gtfs_tiling.source_tilings tiling
            JOIN gtfs_meta.feed_sources source
              ON source.id = tiling.source_id
            WHERE ($1::TEXT IS NULL OR source.slug = $1)
        ),
        feature_extent AS (
            SELECT ST_Extent(stop.geom) AS bounds
            FROM gtfs_tiling.stop_points stop
            JOIN selected_tilings tiling
              ON tiling.source_id = stop.source_id
             AND tiling.version_id = stop.version_id
        ),
        zoom_inputs AS (
            SELECT
                $2::INTEGER AS z,
                POWER(2.0, $2::INTEGER)::INTEGER AS n,
                GREATEST(-180.0, LEAST(180.0, ST_XMin(bounds))) AS min_lon,
                GREATEST(-180.0, LEAST(180.0, ST_XMax(bounds))) AS max_lon,
                GREATEST($3, LEAST($4, ST_YMin(bounds))) AS min_lat,
                GREATEST($3, LEAST($4, ST_YMax(bounds))) AS max_lat
            FROM feature_extent
            WHERE bounds IS NOT NULL
        ),
        tile_ranges AS (
            SELECT
                z,
                GREATEST(
                    0,
                    LEAST(n - 1, FLOOR(((min_lon + 180.0) / 360.0) * n)::INTEGER)
                ) AS x_min,
                GREATEST(
                    0,
                    LEAST(n - 1, FLOOR(((max_lon + 180.0) / 360.0) * n)::INTEGER)
                ) AS x_max,
                GREATEST(
                    0,
                    LEAST(
                        n - 1,
                        FLOOR(
                            (
                                (
                                    1.0
                                    - LN(TAN(RADIANS(max_lat)) + 1.0 / COS(RADIANS(max_lat))) / PI()
                                )
                                / 2.0
                            )
                            * n
                        )::INTEGER
                    )
                ) AS y_min,
                GREATEST(
                    0,
                    LEAST(
                        n - 1,
                        FLOOR(
                            (
                                (
                                    1.0
                                    - LN(TAN(RADIANS(min_lat)) + 1.0 / COS(RADIANS(min_lat))) / PI()
                                )
                                / 2.0
                            )
                            * n
                        )::INTEGER
                    )
                ) AS y_max
            FROM zoom_inputs
        ),
        candidate_tiles AS (
            SELECT z, x, y
            FROM tile_ranges
            CROSS JOIN LATERAL generate_series(x_min, x_max) x
            CROSS JOIN LATERAL generate_series(y_min, y_max) y
        )
        SELECT coord.z, coord.x, coord.y
        FROM candidate_tiles coord
        CROSS JOIN LATERAL (
            SELECT ST_Transform(ST_TileEnvelope(coord.z, coord.x, coord.y), 4326) AS bounds_4326
        ) tile_bounds
        WHERE EXISTS (
            SELECT 1
            FROM gtfs_tiling.stop_points stop
            JOIN selected_tilings tiling
              ON tiling.source_id = stop.source_id
             AND tiling.version_id = stop.version_id
            WHERE stop.geom && tile_bounds.bounds_4326
              AND ST_Intersects(stop.geom, tile_bounds.bounds_4326)
        )
        ORDER BY coord.z, coord.x, coord.y
        "#,
    )
    .bind(source_slug)
    .bind(zoom)
    .bind(-MAX_WEB_MERCATOR_LATITUDE)
    .bind(MAX_WEB_MERCATOR_LATITUDE)
    .fetch(pool)
}

pub fn stream_export_tiles<'a>(
    pool: &'a PgPool,
    source_slug: Option<&'a str>,
    chunk_tile_id: TilingExportTileId,
) -> impl Stream<Item = std::result::Result<TilingExportTile, sqlx::Error>> + 'a {
    sqlx::query_as::<_, TilingExportTile>(
        r#"
        WITH selected_tilings AS (
            SELECT
                tiling.source_id,
                tiling.version_id,
                source.slug AS source_slug
            FROM gtfs_tiling.source_tilings tiling
            JOIN gtfs_meta.feed_sources source
              ON source.id = tiling.source_id
            WHERE ($1::TEXT IS NULL OR source.slug = $1)
        ),
        tile_ranges AS (
            SELECT
                $2::INTEGER AS z,
                ($4::INTEGER * POWER(2.0, $2::INTEGER - $3::INTEGER)::INTEGER) AS x_min,
                (($4::INTEGER + 1) * POWER(2.0, $2::INTEGER - $3::INTEGER)::INTEGER - 1) AS x_max,
                ($5::INTEGER * POWER(2.0, $2::INTEGER - $3::INTEGER)::INTEGER) AS y_min,
                (($5::INTEGER + 1) * POWER(2.0, $2::INTEGER - $3::INTEGER)::INTEGER - 1) AS y_max
            WHERE $3::INTEGER <= $2::INTEGER
        ),
        tile_coords AS (
            SELECT z, x, y
            FROM tile_ranges
            CROSS JOIN LATERAL generate_series(x_min, x_max) x
            CROSS JOIN LATERAL generate_series(y_min, y_max) y
        ),
        mvt_tiles AS (
            SELECT
                coord.z,
                coord.x,
                coord.y,
                COALESCE(tile_data.tile, ''::BYTEA) AS tile
            FROM tile_coords coord
            CROSS JOIN LATERAL (
                SELECT
                    bounds_3857,
                    ST_Transform(bounds_3857, 4326) AS bounds_4326
                FROM (
                    SELECT ST_TileEnvelope(coord.z, coord.x, coord.y) AS bounds_3857
                ) bounds
            ) tile_bounds
            CROSS JOIN LATERAL (
                WITH tile_stop_features AS (
                    SELECT
                        ST_AsMVTGeom(
                            ST_Transform(stop_point.geom, 3857),
                            tile_bounds.bounds_3857,
                            4096,
                            64,
                            TRUE
                        ) AS geom,
                        stop_point.feature_id,
                        stop_point.version_id,
                        tiling.source_slug,
                        stop_point.stop_item_id,
                        stop.item_gtfs_id AS stop_id,
                        stop.stop_code,
                        stop.stop_name,
                        stop.stop_desc,
                        parent_stop.item_gtfs_id AS parent_station_id,
                        stop.location_type,
                        stop.wheelchair_boarding,
                        stop.platform_code
                    FROM gtfs_tiling.stop_points stop_point
                    JOIN gtfs.stops stop
                      ON stop.item_id = stop_point.stop_item_id
                     AND stop.version_id = stop_point.version_id
                    LEFT JOIN gtfs.stops parent_stop
                      ON parent_stop.version_id = stop.version_id
                     AND parent_stop.item_id = stop.parent_station_item_id
                    JOIN selected_tilings tiling
                      ON tiling.source_id = stop_point.source_id
                     AND tiling.version_id = stop_point.version_id
                    WHERE stop_point.geom && tile_bounds.bounds_4326
                      AND ST_Intersects(stop_point.geom, tile_bounds.bounds_4326)
                ),
                route_summaries AS (
                    SELECT
                        stop_route_agg_ref.version_id,
                        stop_route_agg_ref.stop_item_id,
                        string_agg(DISTINCT route.route_type::TEXT, ',' ORDER BY route.route_type::TEXT)
                            FILTER (WHERE route.route_type IS NOT NULL) AS route_types,
                        cardinality(stop_route_agg_ref.route_item_ids)::INTEGER AS route_count
                    FROM tile_stop_features feature
                    JOIN gtfs.stop_route_agg_refs stop_route_agg_ref
                      ON stop_route_agg_ref.version_id = feature.version_id
                     AND stop_route_agg_ref.stop_item_id = feature.stop_item_id
                    LEFT JOIN LATERAL unnest(stop_route_agg_ref.route_item_ids) route_ref(route_item_id)
                      ON TRUE
                    LEFT JOIN gtfs.routes route
                      ON route.version_id = stop_route_agg_ref.version_id
                     AND route.item_id = route_ref.route_item_id
                    GROUP BY stop_route_agg_ref.version_id, stop_route_agg_ref.stop_item_id, stop_route_agg_ref.route_item_ids
                )
                SELECT ST_AsMVT(stop_feature, 'stops', 4096, 'geom', 'feature_id') AS tile
                FROM (
                    SELECT
                        feature.geom,
                        feature.feature_id,
                        feature.source_slug,
                        feature.version_id,
                        feature.stop_id,
                        feature.stop_code,
                        feature.stop_name,
                        feature.stop_desc,
                        feature.parent_station_id,
                        feature.location_type,
                        feature.wheelchair_boarding,
                        feature.platform_code,
                        COALESCE(route_summary.route_types, '') AS route_types,
                        COALESCE(route_summary.route_count, 0) AS route_count
                    FROM tile_stop_features feature
                    LEFT JOIN route_summaries route_summary
                      ON route_summary.version_id = feature.version_id
                     AND route_summary.stop_item_id = feature.stop_item_id
                    WHERE feature.geom IS NOT NULL
                ) stop_feature
            ) tile_data
        )
        SELECT z, x, y, tile
        FROM mvt_tiles
        WHERE OCTET_LENGTH(tile) > 0
        ORDER BY z, x, y
        "#,
    )
    .bind(source_slug)
    .bind(GTFS_TILING_ZOOM)
    .bind(chunk_tile_id.z)
    .bind(chunk_tile_id.x)
    .bind(chunk_tile_id.y)
    .fetch(pool)
}

#[derive(Debug, FromRow)]
struct TilingSourceState {
    source_id: i64,
    source_slug: String,
    active_version_id: Option<i32>,
    tiled_version_id: Option<i32>,
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
    version_id: i32,
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

async fn create_tiling_feature_id_sequence(tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    sqlx::query("CREATE TEMP SEQUENCE gtfs_tiling_feature_id_seq AS BIGINT")
        .execute(&mut **tx)
        .await
        .context("failed to create GTFS tiling feature id sequence")?;

    Ok(())
}

async fn drop_tiling_feature_id_sequence(tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    sqlx::query("DROP SEQUENCE gtfs_tiling_feature_id_seq")
        .execute(&mut **tx)
        .await
        .context("failed to drop GTFS tiling feature id sequence")?;

    Ok(())
}

// Importing

async fn import_source_tiling_data(
    tx: &mut Transaction<'_, Postgres>,
    source_id: i64,
    version_id: i32,
) -> Result<()> {
    import_source_tiling_stop_points(tx, source_id, version_id).await?;
    import_source_trip_shape_lines(tx, source_id, version_id).await?;
    import_source_trip_stop_sequence_lines(tx, source_id, version_id).await?;
    Ok(())
}

async fn import_source_tiling_stop_points(
    tx: &mut Transaction<'_, Postgres>,
    source_id: i64,
    version_id: i32,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO gtfs_tiling.stop_points (
            source_id,
            version_id,
            feature_id,
            stop_item_id,
            geom
        )
        SELECT
            $1,
            $2,
            nextval('gtfs_tiling_feature_id_seq')::BIGINT,
            item_id,
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

async fn import_source_trip_shape_lines(
    tx: &mut Transaction<'_, Postgres>,
    source_id: i64,
    version_id: i32,
) -> Result<()> {
    sqlx::query(
        r#"
        WITH valid_shape_lines AS (
            SELECT
                shape.shape_item_id,
                shape.geom
            FROM gtfs.shapes_seq shape
            WHERE shape.version_id = $2
              AND shape.geom IS NOT NULL
              AND ST_IsValid(shape.geom)
              AND ST_NPoints(ST_RemoveRepeatedPoints(shape.geom)) >= 2
        ),
        line_features AS MATERIALIZED (
            SELECT
                candidate.route_item_id,
                candidate.shape_item_id,
                nextval('gtfs_tiling_feature_id_seq')::BIGINT AS feature_id
            FROM (
                SELECT DISTINCT
                    trip.route_item_id,
                    trip.shape_item_id
                FROM gtfs.trips trip
                JOIN valid_shape_lines shape_line
                  ON shape_line.shape_item_id = trip.shape_item_id
                WHERE trip.version_id = $2
                  AND trip.route_item_id IS NOT NULL
                  AND trip.shape_item_id IS NOT NULL
            ) candidate
        ),
        inserted_trip_lines AS (
            INSERT INTO gtfs_tiling.trip_lines (
                source_id,
                version_id,
                feature_id,
                route_item_id,
                geom
            )
            SELECT
                $1,
                $2,
                line_feature.feature_id,
                line_feature.route_item_id,
                shape_line.geom
            FROM line_features line_feature
            JOIN valid_shape_lines shape_line
              ON shape_line.shape_item_id = line_feature.shape_item_id
            RETURNING feature_id
        )
        INSERT INTO gtfs_tiling.trip_line_refs (
            source_id,
            version_id,
            route_item_id,
            trip_item_id,
            trip_line_feature_id
        )
        SELECT
            $1,
            $2,
            trip.route_item_id,
            trip.item_id,
            line_feature.feature_id
        FROM gtfs.trips trip
        JOIN line_features line_feature
          ON line_feature.route_item_id = trip.route_item_id
         AND line_feature.shape_item_id = trip.shape_item_id
        JOIN inserted_trip_lines inserted_trip_line
          ON inserted_trip_line.feature_id = line_feature.feature_id
        WHERE trip.version_id = $2
          AND trip.route_item_id IS NOT NULL
          AND trip.shape_item_id IS NOT NULL
        ON CONFLICT (source_id, version_id, trip_item_id) DO NOTHING
        "#,
    )
    .bind(source_id)
    .bind(version_id)
    .execute(&mut **tx)
    .await
    .context("failed to materialize shaped GTFS trip lines")?;

    Ok(())
}

async fn import_source_trip_stop_sequence_lines(
    tx: &mut Transaction<'_, Postgres>,
    source_id: i64,
    version_id: i32,
) -> Result<()> {
    drop_stop_sequence_trip_line_temp_tables(tx).await?;
    create_stop_sequence_trip_line_temp_tables(tx).await?;
    collect_stop_sequence_trip_candidates(tx, version_id).await?;
    collect_stop_sequence_line_features(tx).await?;
    persist_stop_sequence_trip_lines(tx, source_id).await?;
    persist_stop_sequence_trip_line_refs(tx, source_id).await?;
    drop_stop_sequence_trip_line_temp_tables(tx).await?;

    Ok(())
}

async fn create_stop_sequence_trip_line_temp_tables(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TEMP TABLE gtfs_tiling_stop_sequence_trip_candidates (
            version_id INTEGER NOT NULL,
            route_item_id INTEGER NOT NULL,
            trip_item_id INTEGER NOT NULL,
            stop_item_ids INTEGER[] NOT NULL
        ) ON COMMIT DROP
        "#,
    )
    .execute(&mut **tx)
    .await
    .context("failed to create GTFS stop-sequence trip candidate temp table")?;

    sqlx::query(
        r#"
        CREATE TEMP TABLE gtfs_tiling_stop_sequence_line_features (
            version_id INTEGER NOT NULL,
            route_item_id INTEGER NOT NULL,
            stop_item_ids INTEGER[] NOT NULL,
            representative_trip_item_id INTEGER NOT NULL,
            feature_id BIGINT NOT NULL
        ) ON COMMIT DROP
        "#,
    )
    .execute(&mut **tx)
    .await
    .context("failed to create GTFS stop-sequence line feature temp table")?;

    Ok(())
}

async fn drop_stop_sequence_trip_line_temp_tables(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<()> {
    sqlx::query("DROP TABLE IF EXISTS gtfs_tiling_stop_sequence_line_features")
        .execute(&mut **tx)
        .await
        .context("failed to drop GTFS stop-sequence line feature temp table")?;

    sqlx::query("DROP TABLE IF EXISTS gtfs_tiling_stop_sequence_trip_candidates")
        .execute(&mut **tx)
        .await
        .context("failed to drop GTFS stop-sequence trip candidate temp table")?;

    Ok(())
}

async fn collect_stop_sequence_trip_candidates(
    tx: &mut Transaction<'_, Postgres>,
    version_id: i32,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO gtfs_tiling_stop_sequence_trip_candidates (
            version_id,
            route_item_id,
            trip_item_id,
            stop_item_ids
        )
        SELECT
            trip.version_id,
            trip.route_item_id,
            trip.item_id,
            stop_time.stop_item_ids
        FROM gtfs.trips trip
        JOIN gtfs.stop_times_seq stop_time
          ON stop_time.version_id = trip.version_id
         AND stop_time.trip_item_id = trip.item_id
        WHERE trip.version_id = $1
          AND trip.route_item_id IS NOT NULL
          AND trip.shape_item_id IS NULL
          AND array_length(stop_time.stop_item_ids, 1) >= 2
        "#,
    )
    .bind(version_id)
    .execute(&mut **tx)
    .await
    .context("failed to collect stop-sequence GTFS trip line candidates")?;

    Ok(())
}

async fn collect_stop_sequence_line_features(tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO gtfs_tiling_stop_sequence_line_features (
            version_id,
            route_item_id,
            stop_item_ids,
            representative_trip_item_id,
            feature_id
        )
        SELECT
            trip_candidate.version_id,
            trip_candidate.route_item_id,
            trip_candidate.stop_item_ids,
            MIN(trip_candidate.trip_item_id),
            nextval('gtfs_tiling_feature_id_seq')::BIGINT
        FROM gtfs_tiling_stop_sequence_trip_candidates trip_candidate
        GROUP BY
            trip_candidate.version_id,
            trip_candidate.route_item_id,
            trip_candidate.stop_item_ids
        "#,
    )
    .execute(&mut **tx)
    .await
    .context("failed to collect stop-sequence GTFS trip line features")?;

    Ok(())
}

async fn persist_stop_sequence_trip_lines(
    tx: &mut Transaction<'_, Postgres>,
    source_id: i64,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO gtfs_tiling.trip_lines (
            source_id,
            version_id,
            feature_id,
            route_item_id,
            geom
        )
        SELECT
            $1,
            line_feature.version_id,
            line_feature.feature_id,
            line_feature.route_item_id,
            ST_MakeLine(
                ST_SetSRID(ST_MakePoint(stop.stop_lon, stop.stop_lat), 4326)
                ORDER BY stop_ref.stop_order
            )
        FROM gtfs_tiling_stop_sequence_line_features line_feature
        JOIN LATERAL unnest(line_feature.stop_item_ids) WITH ORDINALITY
            AS stop_ref(stop_item_id, stop_order)
          ON stop_ref.stop_item_id IS NOT NULL
        JOIN gtfs.stops stop
          ON stop.version_id = line_feature.version_id
         AND stop.item_id = stop_ref.stop_item_id
        WHERE stop.stop_lon BETWEEN -180.0 AND 180.0
          AND stop.stop_lat BETWEEN $2 AND $3
        GROUP BY
            line_feature.version_id,
            line_feature.route_item_id,
            line_feature.feature_id
        HAVING COUNT(*) >= 2
           AND (
               MIN(stop.stop_lon) <> MAX(stop.stop_lon)
               OR MIN(stop.stop_lat) <> MAX(stop.stop_lat)
           )
        "#,
    )
    .bind(source_id)
    .bind(-MAX_WEB_MERCATOR_LATITUDE)
    .bind(MAX_WEB_MERCATOR_LATITUDE)
    .execute(&mut **tx)
    .await
    .context("failed to persist stop-sequence GTFS trip lines")?;

    Ok(())
}

async fn persist_stop_sequence_trip_line_refs(
    tx: &mut Transaction<'_, Postgres>,
    source_id: i64,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO gtfs_tiling.trip_line_refs (
            source_id,
            version_id,
            route_item_id,
            trip_item_id,
            trip_line_feature_id
        )
        SELECT
            $1,
            trip_candidate.version_id,
            trip_candidate.route_item_id,
            trip_candidate.trip_item_id,
            line_feature.feature_id
        FROM gtfs_tiling_stop_sequence_trip_candidates trip_candidate
        JOIN gtfs_tiling_stop_sequence_line_features line_feature
          ON line_feature.version_id = trip_candidate.version_id
         AND line_feature.route_item_id = trip_candidate.route_item_id
         AND line_feature.stop_item_ids = trip_candidate.stop_item_ids
        JOIN gtfs_tiling.trip_lines trip_line
          ON trip_line.source_id = $1
         AND trip_line.version_id = line_feature.version_id
         AND trip_line.feature_id = line_feature.feature_id
        ON CONFLICT (source_id, version_id, trip_item_id) DO NOTHING
        "#,
    )
    .bind(source_id)
    .execute(&mut **tx)
    .await
    .context("failed to persist stop-sequence GTFS trip line refs")?;

    Ok(())
}
