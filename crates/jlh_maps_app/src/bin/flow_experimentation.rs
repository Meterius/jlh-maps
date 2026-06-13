use anyhow::{Context, Result, anyhow, bail, ensure};
use bevy::math::{DVec2, USizeVec2, dvec2};
use geo::algorithm::unary_union;
use geo_types::{Coord, LineString, MultiPolygon, Polygon as GeoPolygon};
use geojson::{Feature, FeatureCollection, GeoJson, Geometry, Value};
use geozero::GeozeroDatasource;
use geozero::geojson::GeoJsonString;
use geozero::mvt::{Message as _, MvtWriter, Tile as MvtTile};
use jlh_maps_app::app::maplibre_gl_js::mvt::parse_tile;
use jlh_maps_app::app::maplibre_gl_js::types::{CanonicalTileId, MlTile, MlTileFeature};
use jlh_maps_app::app::maplibre_gl_js::utils::mercator_coordinate::tile_uv_from_lng_lat;
use jlh_maps_app::app::maplibre_gl_js::utils::tile::get_tile_lnglat_bounds;
use jlh_maps_app::utils::flow::{
    CellFlowGrid, FlowFromGeometryConfig, FlowPoissonCorrectionConfig, FluidGeometryConfig,
    apply_fluid_boundary, apply_fluid_exterior, create_flow_grid_from_geometry,
    poisson_correct_flow_grid,
};
use serde_json::{Map as JsonMap, json};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const DEFAULT_TILE: CanonicalTileId = CanonicalTileId {
    z: 14,
    x: 8798,
    y: 5373,
};
const DEFAULT_PMTILES_PATH: &str = "data/berlin.pmtiles";
const OUTPUT_DIR: &str = "crates/jlh_maps_app/experimentation/flow";
const OUTPUT_FILE_NAME: &str = "flow_geometry.pmtiles";

const WATER_LAYER: &str = "water";
const WATERWAY_LAYER: &str = "waterway";
const FLOW_LAYER: &str = "flow_arrows";
const FLOW_NORMAL_PROJECTED_LAYER: &str = "flow_arrows_normal_projected";
const FINAL_GRID_FLOW_LAYER: &str = "final_grid_flow";
const DEFAULT_TILE_EXTENT: f64 = 4096.0;

const FLOW_GRID_SIZE: usize = 256;
const FLOW_GRID_BUFFER_CELLS: usize = 4;
const FLOW_NEAREST_SEGMENT_COUNT: usize = 16;
const FLOW_SIGMA: f64 = 64.0 / DEFAULT_TILE_EXTENT;
const FLOW_POISSON_ITERATIONS: usize = 600;
const FLOW_POISSON_TOLERANCE: f64 = 1e-5;
const FLOW_VECTOR_STRIDE: u32 = 1;

fn main() -> Result<()> {
    let run_started_at = Instant::now();
    let repo_root = find_repo_root()?;
    let pmtiles_path = repo_root.join(DEFAULT_PMTILES_PATH);
    let output_dir = repo_root.join(OUTPUT_DIR);
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;

    let mut output_tiles = Vec::new();
    for tile_id in experiment_tile_ids(DEFAULT_TILE) {
        output_tiles.push(
            build_flow_output_tile(&pmtiles_path, tile_id)
                .with_context(|| format!("failed to build flow tile {}", tile_label(tile_id)))?,
        );
    }

    let output_path = output_dir.join(OUTPUT_FILE_NAME);
    let write_started_at = Instant::now();
    write_output_pmtiles(&output_path, &output_tiles)?;
    print_elapsed_timing(
        "archive",
        "write combined pmtiles",
        write_started_at.elapsed(),
        run_started_at.elapsed(),
    );

    println!("wrote {}", output_path.display());
    for tile in &output_tiles {
        println!(
            "tile {}: {} water polygons -> {} merged polygons, {} waterway lines, {} flow arrows, {} normal-projected flow arrows, {} final flow arrows, {}x{} flow grid",
            tile_label(tile.tile_id),
            tile.stats.water_polygons,
            tile.stats.merged_water_polygons,
            tile.stats.waterway_lines,
            tile.stats.flow_arrows,
            tile.stats.normal_projected_flow_arrows,
            tile.stats.final_flow_arrows,
            tile.stats.grid_dim.x,
            tile.stats.grid_dim.y
        );
    }

    Ok(())
}

fn print_elapsed_timing(scope: &str, step: &str, step_elapsed: Duration, total_elapsed: Duration) {
    println!(
        "timing {scope} {step}: step={:.1}ms total={:.1}ms",
        step_elapsed.as_secs_f64() * 1000.0,
        total_elapsed.as_secs_f64() * 1000.0
    );
}

fn print_tile_step_timing(
    tile_id: CanonicalTileId,
    step: &str,
    tile_started_at: Instant,
    step_started_at: &mut Instant,
) {
    let now = Instant::now();
    print_elapsed_timing(
        &format!("tile {}", tile_label(tile_id)),
        step,
        now.duration_since(*step_started_at),
        now.duration_since(tile_started_at),
    );
    *step_started_at = now;
}

#[derive(Clone)]
struct FlowOutputTile {
    tile_id: CanonicalTileId,
    tile_bounds: (DVec2, DVec2),
    mvt_bytes: Vec<u8>,
    stats: FlowOutputTileStats,
}

#[derive(Clone)]
struct FlowOutputTileStats {
    water_polygons: usize,
    merged_water_polygons: usize,
    waterway_lines: usize,
    flow_arrows: usize,
    normal_projected_flow_arrows: usize,
    final_flow_arrows: usize,
    grid_dim: USizeVec2,
}

fn experiment_tile_ids(center: CanonicalTileId) -> Vec<CanonicalTileId> {
    let Some(tile_count) = 1_i64.checked_shl(center.z) else {
        return Vec::new();
    };
    let center_x = center.x as i64;
    let center_y = center.y as i64;
    let mut tile_ids = Vec::new();

    for y in center_y - 1..=center_y + 1 {
        for x in center_x - 1..=center_x + 1 {
            if (0..tile_count).contains(&x) && (0..tile_count).contains(&y) {
                tile_ids.push(CanonicalTileId {
                    z: center.z,
                    x: x as u32,
                    y: y as u32,
                });
            }
        }
    }

    tile_ids
}

fn build_flow_output_tile(pmtiles_path: &Path, tile_id: CanonicalTileId) -> Result<FlowOutputTile> {
    let tile_started_at = Instant::now();
    let mut step_started_at = tile_started_at;

    let tile_bytes = read_pmtiles_tile(pmtiles_path, tile_id)?;
    print_tile_step_timing(
        tile_id,
        "read source tile",
        tile_started_at,
        &mut step_started_at,
    );

    let tile_extent = layer_extent(&tile_bytes, WATER_LAYER, WATERWAY_LAYER)?;
    let tile = parse_tile(tile_id, tile_bytes, 0).map_err(|err| anyhow!(err))?;
    let tile_bounds = get_tile_lnglat_bounds(tile_id);
    print_tile_step_timing(
        tile_id,
        "parse source tile",
        tile_started_at,
        &mut step_started_at,
    );

    let water_polygons = extract_layer_polygons(&tile, WATER_LAYER, tile_bounds, tile_extent);
    let waterway_lines = extract_layer_lines(&tile, WATERWAY_LAYER, tile_bounds, tile_extent);
    print_tile_step_timing(
        tile_id,
        "extract geometry",
        tile_started_at,
        &mut step_started_at,
    );

    let merged_water = if water_polygons.is_empty() {
        MultiPolygon(vec![])
    } else {
        unary_union(&water_polygons)
    };
    print_tile_step_timing(
        tile_id,
        "merge water geometry",
        tile_started_at,
        &mut step_started_at,
    );

    let interior_grid_dim = USizeVec2::splat(FLOW_GRID_SIZE);
    let flow_grid_dim = buffered_grid_dim(interior_grid_dim, FLOW_GRID_BUFFER_CELLS);
    let flow_grid_origin = USizeVec2::splat(FLOW_GRID_BUFFER_CELLS);
    let flow_bounds = buffered_flow_bounds(
        (DVec2::ZERO, DVec2::splat(tile_extent)),
        interior_grid_dim,
        FLOW_GRID_BUFFER_CELLS,
    );
    let flow_grid = create_flow_grid_from_geometry(
        flow_grid_dim,
        FlowFromGeometryConfig {
            bounds: flow_bounds,
            fluid_region: merged_water.clone(),
            flow_alignments: waterway_lines.clone(),
            sigma: FLOW_SIGMA,
            max_nearest_neighbor: FLOW_NEAREST_SEGMENT_COUNT,
        },
    );
    print_tile_step_timing(
        tile_id,
        "generate base mac flow",
        tile_started_at,
        &mut step_started_at,
    );

    let flow_cell_grid =
        flow_grid.to_cell_flow_grid_window(flow_grid_origin, interior_grid_dim, true);
    let flow_arrows = build_flow_arrow_features(tile_extent, &flow_cell_grid);
    print_tile_step_timing(
        tile_id,
        "build base arrows",
        tile_started_at,
        &mut step_started_at,
    );

    let mut constrained_flow_grid = flow_grid.clone();
    apply_fluid_exterior(
        &mut constrained_flow_grid,
        FluidGeometryConfig {
            bounds: flow_bounds,
            fluid_region: &merged_water,
        },
    );
    apply_fluid_boundary(
        &mut constrained_flow_grid,
        FluidGeometryConfig {
            bounds: flow_bounds,
            fluid_region: &merged_water,
        },
    );
    print_tile_step_timing(
        tile_id,
        "apply fluid constraints",
        tile_started_at,
        &mut step_started_at,
    );

    let normal_projected_flow_cell_grid =
        constrained_flow_grid.to_cell_flow_grid_window(flow_grid_origin, interior_grid_dim, false);
    let normal_projected_flow_arrows =
        build_flow_arrow_features(tile_extent, &normal_projected_flow_cell_grid);
    print_tile_step_timing(
        tile_id,
        "build constrained arrows",
        tile_started_at,
        &mut step_started_at,
    );

    let final_flow_grid = poisson_correct_flow_grid(
        constrained_flow_grid,
        FlowPoissonCorrectionConfig {
            iterations: FLOW_POISSON_ITERATIONS,
            tolerance: FLOW_POISSON_TOLERANCE,
        },
    );
    print_tile_step_timing(
        tile_id,
        "poisson correction",
        tile_started_at,
        &mut step_started_at,
    );

    let final_flow_cell_grid =
        final_flow_grid.to_cell_flow_grid_window(flow_grid_origin, interior_grid_dim, true);
    let final_flow_arrows = build_flow_arrow_features(tile_extent, &final_flow_cell_grid);
    print_tile_step_timing(
        tile_id,
        "build final arrows",
        tile_started_at,
        &mut step_started_at,
    );

    let mvt_tile = build_output_mvt_tile(
        tile_extent,
        &merged_water,
        &waterway_lines,
        &flow_arrows,
        &normal_projected_flow_arrows,
        &final_flow_arrows,
    )?;
    print_tile_step_timing(
        tile_id,
        "encode output mvt",
        tile_started_at,
        &mut step_started_at,
    );

    let stats = FlowOutputTileStats {
        water_polygons: water_polygons.len(),
        merged_water_polygons: merged_water.0.len(),
        waterway_lines: waterway_lines.len(),
        flow_arrows: flow_arrows.len(),
        normal_projected_flow_arrows: normal_projected_flow_arrows.len(),
        final_flow_arrows: final_flow_arrows.len(),
        grid_dim: flow_grid.dim(),
    };

    Ok(FlowOutputTile {
        tile_id,
        tile_bounds,
        mvt_bytes: mvt_tile.encode_to_vec(),
        stats,
    })
}

fn buffered_grid_dim(interior_dim: USizeVec2, buffer_cells: usize) -> USizeVec2 {
    USizeVec2::new(
        interior_dim.x + buffer_cells * 2,
        interior_dim.y + buffer_cells * 2,
    )
}

fn buffered_flow_bounds(
    bounds: (DVec2, DVec2),
    interior_dim: USizeVec2,
    buffer_cells: usize,
) -> (DVec2, DVec2) {
    let cell_size = (bounds.1 - bounds.0) / interior_dim.as_dvec2();
    let buffer = cell_size * buffer_cells as f64;
    (bounds.0 - buffer, bounds.1 + buffer)
}

fn find_repo_root() -> Result<PathBuf> {
    let current = std::env::current_dir().context("failed to read current directory")?;
    for candidate in current.ancestors() {
        if candidate.join(DEFAULT_PMTILES_PATH).is_file()
            && candidate.join("crates/jlh_maps_app/Cargo.toml").is_file()
        {
            return Ok(candidate.to_path_buf());
        }
    }

    bail!(
        "could not find repo root containing `{DEFAULT_PMTILES_PATH}` and `crates/jlh_maps_app/Cargo.toml`"
    )
}

fn read_pmtiles_tile(pmtiles_path: &Path, tile_id: CanonicalTileId) -> Result<Vec<u8>> {
    let coordinate = pmtiles::TileCoord::new(tile_id.z as u8, tile_id.x, tile_id.y)
        .with_context(|| format!("invalid tile id {}/{}/{}", tile_id.z, tile_id.x, tile_id.y))?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .context("failed to create PMTiles runtime")?;

    let tile = runtime.block_on(async {
        let reader = pmtiles::AsyncPmTilesReader::new_with_path(pmtiles_path).await?;
        ensure!(
            reader.get_header().tile_type == pmtiles::TileType::Mvt,
            "expected an MVT PMTiles archive, got {:?}",
            reader.get_header().tile_type
        );
        reader
            .get_tile_decompressed(coordinate)
            .await?
            .with_context(|| {
                format!(
                    "tile {}/{}/{} was not found in {}",
                    tile_id.z,
                    tile_id.x,
                    tile_id.y,
                    pmtiles_path.display()
                )
            })
    })?;

    Ok(tile.to_vec())
}

fn layer_extent(tile_bytes: &[u8], water_layer: &str, waterway_layer: &str) -> Result<f64> {
    let reader = mvt_reader::Reader::new(tile_bytes.to_vec())
        .map_err(|err| anyhow!("failed to decode MVT tile for layer metadata: {err:?}"))?;
    let layer_metadata = reader
        .get_layer_metadata()
        .map_err(|err| anyhow!("failed to read MVT layer metadata: {err:?}"))?;

    Ok(layer_metadata
        .iter()
        .find(|layer| layer.name == water_layer)
        .or_else(|| {
            layer_metadata
                .iter()
                .find(|layer| layer.name == waterway_layer)
        })
        .map(|layer| layer.extent as f64)
        .unwrap_or(DEFAULT_TILE_EXTENT))
}

fn extract_layer_polygons(
    tile: &MlTile,
    layer_id: &str,
    tile_bounds: (DVec2, DVec2),
    tile_extent: f64,
) -> Vec<GeoPolygon<f64>> {
    let mut polygons = Vec::new();
    if let Some(layer) = tile.layers.get(layer_id) {
        for feature in layer.features.values() {
            collect_feature_polygons(feature, tile_bounds, tile_extent, &mut polygons);
        }
    }
    polygons
}

fn extract_layer_lines(
    tile: &MlTile,
    layer_id: &str,
    tile_bounds: (DVec2, DVec2),
    tile_extent: f64,
) -> Vec<LineString<f64>> {
    let mut lines = Vec::new();
    if let Some(layer) = tile.layers.get(layer_id) {
        for feature in layer.features.values() {
            collect_feature_lines(feature, tile_bounds, tile_extent, &mut lines);
        }
    }
    lines
}

fn collect_feature_polygons(
    feature: &MlTileFeature,
    tile_bounds: (DVec2, DVec2),
    tile_extent: f64,
    polygons: &mut Vec<GeoPolygon<f64>>,
) {
    collect_value_polygons(&feature.geometry.value, tile_bounds, tile_extent, polygons);
}

fn collect_feature_lines(
    feature: &MlTileFeature,
    tile_bounds: (DVec2, DVec2),
    tile_extent: f64,
    lines: &mut Vec<LineString<f64>>,
) {
    collect_value_lines(&feature.geometry.value, tile_bounds, tile_extent, lines);
}

fn collect_value_polygons(
    value: &Value,
    tile_bounds: (DVec2, DVec2),
    tile_extent: f64,
    polygons: &mut Vec<GeoPolygon<f64>>,
) {
    match value {
        Value::Polygon(rings) => {
            if let Some(polygon) = polygon_from_positions(rings, tile_bounds, tile_extent) {
                polygons.push(polygon);
            }
        }
        Value::MultiPolygon(multi_polygon) => {
            for rings in multi_polygon {
                if let Some(polygon) = polygon_from_positions(rings, tile_bounds, tile_extent) {
                    polygons.push(polygon);
                }
            }
        }
        Value::GeometryCollection(geometries) => {
            for geometry in geometries {
                collect_value_polygons(&geometry.value, tile_bounds, tile_extent, polygons);
            }
        }
        _ => {}
    }
}

fn collect_value_lines(
    value: &Value,
    tile_bounds: (DVec2, DVec2),
    tile_extent: f64,
    lines: &mut Vec<LineString<f64>>,
) {
    match value {
        Value::LineString(positions) => {
            if let Some(line) = line_from_positions(positions, tile_bounds, tile_extent, 2) {
                lines.push(line);
            }
        }
        Value::MultiLineString(line_strings) => {
            for positions in line_strings {
                if let Some(line) = line_from_positions(positions, tile_bounds, tile_extent, 2) {
                    lines.push(line);
                }
            }
        }
        Value::GeometryCollection(geometries) => {
            for geometry in geometries {
                collect_value_lines(&geometry.value, tile_bounds, tile_extent, lines);
            }
        }
        _ => {}
    }
}

fn polygon_from_positions(
    rings: &[Vec<Vec<f64>>],
    tile_bounds: (DVec2, DVec2),
    tile_extent: f64,
) -> Option<GeoPolygon<f64>> {
    let exterior = line_from_positions(rings.first()?, tile_bounds, tile_extent, 4)?;
    let interiors = rings
        .iter()
        .skip(1)
        .filter_map(|ring| line_from_positions(ring, tile_bounds, tile_extent, 4))
        .collect();

    Some(GeoPolygon::new(exterior, interiors))
}

fn line_from_positions(
    positions: &[Vec<f64>],
    tile_bounds: (DVec2, DVec2),
    tile_extent: f64,
    min_len: usize,
) -> Option<LineString<f64>> {
    let mut coords = positions
        .iter()
        .filter_map(|position| tile_coord_from_lng_lat(position, tile_bounds, tile_extent))
        .collect::<Vec<_>>();

    if coords.len() < min_len {
        return None;
    }

    if min_len >= 4 && coords.first() != coords.last() {
        let first = coords[0];
        coords.push(first);
    }

    Some(LineString(coords))
}

fn tile_coord_from_lng_lat(
    position: &[f64],
    tile_bounds: (DVec2, DVec2),
    tile_extent: f64,
) -> Option<Coord<f64>> {
    if position.len() < 2 {
        return None;
    }

    let uv = tile_uv_from_lng_lat(tile_bounds, dvec2(position[0], position[1]));
    Some(Coord {
        x: f64::from(uv.x) * f64::from(tile_extent),
        y: f64::from(uv.y) * f64::from(tile_extent),
    })
}

#[derive(Clone)]
struct FlowArrowFeature {
    grid_x: usize,
    grid_y: usize,
    position: DVec2,
    direction: DVec2,
    parts: Vec<LineString<f64>>,
}

fn build_flow_arrow_features(tile_extent: f64, flow_grid: &CellFlowGrid) -> Vec<FlowArrowFeature> {
    let dim = flow_grid.dim();
    let cell_size = tile_extent / dim.x.max(1) as f64;
    let arrow_length = cell_size * FLOW_VECTOR_STRIDE as f64 * 0.65;

    let mut arrows = Vec::new();
    for y in (0..dim.y).step_by(FLOW_VECTOR_STRIDE as usize) {
        for x in (0..dim.x).step_by(FLOW_VECTOR_STRIDE as usize) {
            let coord = USizeVec2::new(x, y);
            let Some(direction) = flow_grid.get_cell(coord) else {
                continue;
            };
            if direction.length_squared() <= 1e-12 {
                continue;
            }

            let position = dvec2(
                (x as f64 + 0.5) / dim.x as f64 * tile_extent,
                (y as f64 + 0.5) / dim.y as f64 * tile_extent,
            );
            let parts = arrow_parts(position, direction.normalize(), arrow_length);
            arrows.push(FlowArrowFeature {
                grid_x: x,
                grid_y: y,
                position,
                direction,
                parts,
            });
        }
    }
    arrows
}

fn arrow_parts(position: DVec2, direction: DVec2, length: f64) -> Vec<LineString<f64>> {
    let start = position;
    let end = position + direction * length;
    let normal = dvec2(-direction.y, direction.x);
    let head_length = length * 0.28;
    let head_width = length * 0.14;
    let head_left = end - direction * head_length + normal * head_width;
    let head_right = end - direction * head_length - normal * head_width;

    vec![
        line_from_dvec2_pair(start, end),
        line_from_dvec2_pair(head_left, end),
        line_from_dvec2_pair(head_right, end),
    ]
}

fn line_from_dvec2_pair(a: DVec2, b: DVec2) -> LineString<f64> {
    LineString(vec![
        Coord {
            x: f64::from(a.x),
            y: f64::from(a.y),
        },
        Coord {
            x: f64::from(b.x),
            y: f64::from(b.y),
        },
    ])
}

fn build_output_mvt_tile(
    tile_extent: f64,
    water: &MultiPolygon<f64>,
    waterways: &[LineString<f64>],
    flow_arrows: &[FlowArrowFeature],
    normal_projected_flow_arrows: &[FlowArrowFeature],
    final_flow_arrows: &[FlowArrowFeature],
) -> Result<MvtTile> {
    let extent = tile_extent.round() as u16;
    let mut tile = MvtTile::default();
    tile.layers.push(feature_collection_to_mvt_layer(
        WATER_LAYER,
        water_feature_collection(water),
        extent,
    )?);
    tile.layers.push(feature_collection_to_mvt_layer(
        WATERWAY_LAYER,
        waterway_feature_collection(waterways),
        extent,
    )?);
    tile.layers.push(feature_collection_to_mvt_layer(
        FLOW_LAYER,
        flow_arrow_feature_collection(flow_arrows),
        extent,
    )?);
    tile.layers.push(feature_collection_to_mvt_layer(
        FLOW_NORMAL_PROJECTED_LAYER,
        flow_arrow_feature_collection(normal_projected_flow_arrows),
        extent,
    )?);
    tile.layers.push(feature_collection_to_mvt_layer(
        FINAL_GRID_FLOW_LAYER,
        flow_arrow_feature_collection(final_flow_arrows),
        extent,
    )?);
    Ok(tile)
}

fn water_feature_collection(water: &MultiPolygon<f64>) -> FeatureCollection {
    FeatureCollection {
        bbox: None,
        features: vec![Feature {
            bbox: None,
            geometry: Some(Geometry::new(multi_polygon_value(water))),
            id: None,
            properties: Some(properties([("kind", json!("merged_water"))])),
            foreign_members: None,
        }],
        foreign_members: None,
    }
}

fn waterway_feature_collection(waterways: &[LineString<f64>]) -> FeatureCollection {
    FeatureCollection {
        bbox: None,
        features: waterways
            .iter()
            .enumerate()
            .map(|(index, line)| Feature {
                bbox: None,
                geometry: Some(Geometry::new(line_string_value(line))),
                id: None,
                properties: Some(properties([
                    ("kind", json!("waterway")),
                    ("source_index", json!(index)),
                ])),
                foreign_members: None,
            })
            .collect(),
        foreign_members: None,
    }
}

fn flow_arrow_feature_collection(flow_arrows: &[FlowArrowFeature]) -> FeatureCollection {
    FeatureCollection {
        bbox: None,
        features: flow_arrows
            .iter()
            .enumerate()
            .map(|(index, arrow)| Feature {
                bbox: None,
                geometry: Some(Geometry::new(multi_line_string_value(&arrow.parts))),
                id: None,
                properties: Some(properties([
                    ("kind", json!("flow_arrow")),
                    ("source_index", json!(index)),
                    ("grid_x", json!(arrow.grid_x)),
                    ("grid_y", json!(arrow.grid_y)),
                    ("x", json!(arrow.position.x)),
                    ("y", json!(arrow.position.y)),
                    ("u", json!(arrow.direction.x)),
                    ("v", json!(arrow.direction.y)),
                ])),
                foreign_members: None,
            })
            .collect(),
        foreign_members: None,
    }
}

fn multi_polygon_value(multi_polygon: &MultiPolygon<f64>) -> Value {
    Value::MultiPolygon(
        multi_polygon
            .0
            .iter()
            .map(|polygon| {
                std::iter::once(polygon.exterior())
                    .chain(polygon.interiors())
                    .map(line_positions)
                    .collect()
            })
            .collect(),
    )
}

fn line_string_value(line: &LineString<f64>) -> Value {
    Value::LineString(line_positions(line))
}

fn multi_line_string_value(lines: &[LineString<f64>]) -> Value {
    Value::MultiLineString(lines.iter().map(line_positions).collect())
}

fn line_positions(line: &LineString<f64>) -> Vec<Vec<f64>> {
    line.0.iter().map(|coord| vec![coord.x, coord.y]).collect()
}

fn properties<const N: usize>(
    pairs: [(&str, serde_json::Value); N],
) -> JsonMap<String, serde_json::Value> {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

fn feature_collection_to_mvt_layer(
    layer_name: &str,
    feature_collection: FeatureCollection,
    extent: u16,
) -> Result<geozero::mvt::tile::Layer> {
    let geojson = serde_json::to_string(&GeoJson::FeatureCollection(feature_collection))
        .context("failed to serialize tile GeoJSON before MVT encoding")?;
    let mut geojson = GeoJsonString(geojson);
    let mut writer =
        MvtWriter::new_unscaled(u32::from(extent)).context("failed to create MVT writer")?;
    geojson
        .process(&mut writer)
        .context("failed to process tile GeoJSON into MVT layer")?;
    Ok(writer.layer(layer_name))
}

fn write_output_pmtiles(output_path: &Path, tiles: &[FlowOutputTile]) -> Result<()> {
    ensure!(!tiles.is_empty(), "no output tiles were generated");

    let archive_bounds = combined_tile_bounds(tiles);
    let min_zoom = tiles.iter().map(|tile| tile.tile_id.z).min().unwrap_or(0);
    let max_zoom = tiles.iter().map(|tile| tile.tile_id.z).max().unwrap_or(0);
    let center_lng = (archive_bounds.0.x + archive_bounds.1.x) * 0.5;
    let center_lat = (archive_bounds.0.y + archive_bounds.1.y) * 0.5;
    let metadata = serde_json::to_string(&json!({
        "name": "flow_experimentation",
        "description": "Water geometry, waterway geometry, and generated flow arrows.",
        "version": "1",
        "minzoom": min_zoom,
        "maxzoom": max_zoom,
        "bounds": [archive_bounds.0.x, archive_bounds.0.y, archive_bounds.1.x, archive_bounds.1.y],
        "center": [center_lng, center_lat, min_zoom],
        "vector_layers": [
            {
                "id": WATER_LAYER,
                "description": "Merged water polygons.",
                "fields": { "kind": "String" }
            },
            {
                "id": WATERWAY_LAYER,
                "description": "Waterway alignment lines.",
                "fields": { "kind": "String", "source_index": "Number" }
            },
            {
                "id": FLOW_LAYER,
                "description": "Generated flow arrows as MultiLineString shaft/head geometry.",
                "fields": {
                    "kind": "String",
                    "source_index": "Number",
                    "grid_x": "Number",
                    "grid_y": "Number",
                    "x": "Number",
                    "y": "Number",
                    "u": "Number",
                    "v": "Number"
                }
            },
            {
                "id": FLOW_NORMAL_PROJECTED_LAYER,
                "description": "Generated flow arrows from fixed fluid boundary cells.",
                "fields": {
                    "kind": "String",
                    "source_index": "Number",
                    "grid_x": "Number",
                    "grid_y": "Number",
                    "x": "Number",
                    "y": "Number",
                    "u": "Number",
                    "v": "Number"
                }
            },
            {
                "id": FINAL_GRID_FLOW_LAYER,
                "description": "Final generated flow arrows after constrained full-grid Poisson correction.",
                "fields": {
                    "kind": "String",
                    "source_index": "Number",
                    "grid_x": "Number",
                    "grid_y": "Number",
                    "x": "Number",
                    "y": "Number",
                    "u": "Number",
                    "v": "Number"
                }
            }
        ]
    }))?;

    let file = File::create(output_path)
        .with_context(|| format!("failed to create {}", output_path.display()))?;
    let mut writer = pmtiles::PmTilesWriter::new(pmtiles::TileType::Mvt)
        .min_zoom(min_zoom as u8)
        .max_zoom(max_zoom as u8)
        .bounds(
            archive_bounds.0.x,
            archive_bounds.0.y,
            archive_bounds.1.x,
            archive_bounds.1.y,
        )
        .center(center_lng, center_lat)
        .center_zoom(min_zoom as u8)
        .metadata(&metadata)
        .create(file)
        .context("failed to initialize output PMTiles writer")?;

    for tile in tiles {
        let coordinate =
            pmtiles::TileCoord::new(tile.tile_id.z as u8, tile.tile_id.x, tile.tile_id.y)
                .with_context(|| format!("invalid output tile {}", tile_label(tile.tile_id)))?;
        writer
            .add_tile(coordinate, &tile.mvt_bytes)
            .with_context(|| {
                format!("failed to add output MVT tile {}", tile_label(tile.tile_id))
            })?;
    }

    writer
        .finalize()
        .context("failed to finalize output PMTiles")?;

    Ok(())
}

fn combined_tile_bounds(tiles: &[FlowOutputTile]) -> (DVec2, DVec2) {
    let mut min = DVec2::splat(f64::INFINITY);
    let mut max = DVec2::splat(f64::NEG_INFINITY);

    for tile in tiles {
        min = min.min(tile.tile_bounds.0);
        max = max.max(tile.tile_bounds.1);
    }

    (min, max)
}

fn tile_label(tile_id: CanonicalTileId) -> String {
    format!("{}/{}/{}", tile_id.z, tile_id.x, tile_id.y)
}
