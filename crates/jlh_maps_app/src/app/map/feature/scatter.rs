use crate::app::map::feature::tile_task_based::{
    TileTaskBased, TileTaskBasedMeta, TileTaskBasedPlugin,
};
use crate::app::map::feature::utils::poly::ring_without_closing_position;
use crate::app::map::transform::{
    lng_lat_alt_to_world, lng_lat_bounds_contains, tile_flat_center_world,
    tile_world_units_per_meter, world_xy_to_lnglat,
};
use crate::app::maplibre_gl_js::types::{CanonicalTileId, MlTerrainTile, MlTile, MlTileFeature};
use crate::app::maplibre_gl_js::utils::terrain::get_dem_elevation;
use crate::app::maplibre_gl_js::utils::tile::get_tile_lnglat_bounds;
use bevy::app::App;
use bevy::ecs::system::SystemParamItem;
use bevy::math::{DVec2, DVec3, Vec2, Vec3, Vec3Swizzles, vec2};
use bevy::prelude::{Entity, Plugin};
use geojson::{JsonValue, Value};
use map_scatter::prelude::{
    FieldGraphSpec, FieldProgramCache, FieldSemantics, Kind, Layer, NodeSpec, PoissonDiskSampling,
    RunConfig, ScatterRunner, Texture, TextureChannel, TextureRegistry,
};
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::sync::Arc;

const GEOMETRY_MASK_TEXTURE_ID: &str = "geometry_mask";
const SCATTER_KIND_ID: &str = "tree";
const SCATTER_LAYER_ID: &str = "tree_scatter";

pub struct FeatureScatterPlugin;

impl Plugin for FeatureScatterPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TileTaskBasedPlugin::<FeatureTileScatterMeta>::new());
    }
}

pub type FeatureTileScatter = TileTaskBased<FeatureTileScatterMeta>;

pub struct FeatureTileScatterMeta;

#[derive(Clone, Copy)]
pub struct FeatureTileScatterConfig {
    pub layer_id: &'static str,

    pub class_property_key: Option<&'static str>,
    pub allowed_classes: Option<&'static [&'static str]>,

    pub sample_radius_meters: f32,
    pub max_points: usize,
    pub chunk_extent_meters: f32,
    pub raster_cell_size_meters: f32,
}

impl Default for FeatureTileScatterConfig {
    fn default() -> Self {
        Self {
            layer_id: "",
            class_property_key: None,
            allowed_classes: None,
            sample_radius_meters: 25.0,
            max_points: 800,
            chunk_extent_meters: 256.0,
            raster_cell_size_meters: 4.0,
        }
    }
}

impl FeatureTileScatter {
    pub fn new(config: FeatureTileScatterConfig) -> Self {
        TileTaskBased::from_parts(config, FeatureTileScatterState)
    }

    pub fn positions(&self) -> Option<&[Vec3]> {
        self.data()
            .map(|scatter_points| scatter_points.positions.as_slice())
    }
}

pub struct FeatureTileScatterState;

#[derive(Default)]
pub struct FeatureScatterPoints {
    positions: Vec<Vec3>,
}

impl TileTaskBasedMeta for FeatureTileScatterMeta {
    type Data = FeatureScatterPoints;
    type State = FeatureTileScatterState;
    type Config = FeatureTileScatterConfig;
    type ApplyParams = ();

    fn use_terrain() -> bool {
        true
    }

    fn build_data(
        tile: Arc<MlTile>,
        terrain_tile: Option<MlTerrainTile>,
        config: Self::Config,
    ) -> Self::Data {
        build_scatter_points(tile, terrain_tile.as_ref(), config)
    }

    fn apply_data(
        _entity: Entity,
        _params: &mut SystemParamItem<'_, '_, Self::ApplyParams>,
        _config: &Self::Config,
        _state: &mut Self::State,
        data: Option<Self::Data>,
    ) -> Option<Self::Data> {
        data
    }
}

fn build_scatter_points(
    tile: Arc<MlTile>,
    terrain_data: Option<&MlTerrainTile>,
    config: FeatureTileScatterConfig,
) -> FeatureScatterPoints {
    let world_units_per_meter = tile_world_units_per_meter(tile.id) as f32;
    if !world_units_per_meter.is_finite() || world_units_per_meter <= 0.0 {
        return FeatureScatterPoints::default();
    }

    let polygon_mask = PolygonMaskTexture::from_tile_layer(&tile, &config);
    if polygon_mask.is_empty() {
        return FeatureScatterPoints::default();
    }

    let (_, half_extents) = crate::app::map::transform::tile_flat_bounds_world(tile.id);
    let domain_extent = half_extents * 2.0;
    if domain_extent.x <= 0.0 || domain_extent.y <= 0.0 {
        return FeatureScatterPoints::default();
    }

    let mut textures = TextureRegistry::new();
    textures.register(GEOMETRY_MASK_TEXTURE_ID, polygon_mask);

    let cache = FieldProgramCache::new();
    let mut rng = StdRng::seed_from_u64(scatter_seed(config.layer_id, tile.id));
    let plan = PlanBuilder::tree_plan(config.sample_radius_meters * world_units_per_meter);
    let run_config = RunConfig::new(domain_extent)
        .with_chunk_extent((config.chunk_extent_meters * world_units_per_meter).max(1.0))
        .with_raster_cell_size((config.raster_cell_size_meters * world_units_per_meter).max(0.01))
        .with_grid_halo(2);
    let mut runner = ScatterRunner::new(run_config, &textures, &cache);
    let result = runner.run(&plan, &mut rng);
    let terrain_tile =
        terrain_data.map(|terrain_data| TerrainElevationTile::new(tile.id, terrain_data));
    let center = tile_flat_center_world(tile.id);

    let positions = result
        .placements
        .into_iter()
        .take(config.max_points)
        .map(|placement| {
            let local_xy = placement.position;
            let world_xy = center.xy() + local_xy.as_dvec2();
            let lnglat = world_xy_to_lnglat(world_xy);
            let terrain_altitude = terrain_tile
                .as_ref()
                .and_then(|terrain_tile| terrain_tile.elevation_meters(lnglat))
                .unwrap_or(0.0);
            let z = lng_lat_alt_to_world(lnglat.x, lnglat.y, terrain_altitude).z - center.z;
            Vec3::new(local_xy.x, local_xy.y, z as f32)
        })
        .collect();

    FeatureScatterPoints { positions }
}

struct PlanBuilder;

impl PlanBuilder {
    fn tree_plan(sample_radius: f32) -> map_scatter::prelude::Plan {
        let mut spec = FieldGraphSpec::default();
        spec.add_with_semantics(
            "inside_geometry",
            NodeSpec::texture(GEOMETRY_MASK_TEXTURE_ID, TextureChannel::R),
            FieldSemantics::Gate,
        );
        spec.add_with_semantics(
            "probability",
            NodeSpec::constant(1.0),
            FieldSemantics::Probability,
        );

        let layer = Layer::new_with(
            SCATTER_LAYER_ID,
            vec![Kind::new(SCATTER_KIND_ID, spec)],
            PoissonDiskSampling::new(sample_radius.max(0.01)),
        );

        map_scatter::prelude::Plan::new().with_layer(layer)
    }
}

#[derive(Default)]
struct PolygonMaskTexture {
    polygons: Vec<MaskPolygon>,
}

impl PolygonMaskTexture {
    fn from_tile_layer(tile: &MlTile, config: &FeatureTileScatterConfig) -> Self {
        let center = tile_flat_center_world(tile.id);
        let polygons = tile
            .layers
            .get(config.layer_id)
            .iter()
            .flat_map(|layer| layer.features.values())
            .filter(|feature| feature_matches_config(feature, config))
            .flat_map(|feature| feature_mask_polygons(feature, center))
            .collect();

        Self { polygons }
    }

    fn is_empty(&self) -> bool {
        self.polygons.is_empty()
    }
}

impl Texture for PolygonMaskTexture {
    fn sample(&self, _channel: TextureChannel, p: Vec2) -> f32 {
        if self.polygons
            .iter()
            .any(|polygon| polygon.contains(p)) { 1.0 } else { 0.0 }
    }
}

struct MaskPolygon {
    outer: Vec<Vec2>,
    holes: Vec<Vec<Vec2>>,
}

impl MaskPolygon {
    fn contains(&self, point: Vec2) -> bool {
        point_in_ring(point, &self.outer)
            && !self.holes.iter().any(|hole| point_in_ring(point, hole))
    }
}

fn feature_matches_config(feature: &MlTileFeature, config: &FeatureTileScatterConfig) -> bool {
    let Some(allowed_classes) = config.allowed_classes else {
        return true;
    };
    let Some(class_property_key) = config.class_property_key else {
        return true;
    };

    feature
        .properties
        .get(class_property_key)
        .and_then(json_value_as_str)
        .is_some_and(|class| allowed_classes.contains(&class))
}

fn json_value_as_str(value: &JsonValue) -> Option<&str> {
    match value {
        JsonValue::String(value) => Some(value.as_str()),
        _ => None,
    }
}

fn feature_mask_polygons(feature: &MlTileFeature, center: DVec3) -> Vec<MaskPolygon> {
    match &feature.geometry.value {
        Value::Polygon(polygon) => mask_polygon(polygon, center).into_iter().collect(),
        Value::MultiPolygon(polygons) => polygons
            .iter()
            .filter_map(|polygon| mask_polygon(polygon, center))
            .collect(),
        _ => Vec::new(),
    }
}

fn mask_polygon(polygon: &[Vec<Vec<f64>>], center: DVec3) -> Option<MaskPolygon> {
    let mut rings = polygon
        .iter()
        .filter_map(|ring| mask_ring(ring, center))
        .collect::<Vec<_>>();
    if rings.is_empty() {
        return None;
    }

    let outer = rings.remove(0);
    Some(MaskPolygon {
        outer,
        holes: rings,
    })
}

fn mask_ring(ring: &[Vec<f64>], center: DVec3) -> Option<Vec<Vec2>> {
    let positions = ring_without_closing_position(ring)
        .iter()
        .filter_map(|position| {
            if position.len() < 2 {
                return None;
            }

            let world = lng_lat_alt_to_world(position[0], position[1], 0.0) - center;
            Some(world.xy().as_vec2())
        })
        .collect::<Vec<_>>();

    (positions.len() >= 3).then_some(positions)
}

fn point_in_ring(point: Vec2, ring: &[Vec2]) -> bool {
    if ring.len() < 3 {
        return false;
    }

    let mut inside = false;
    let mut previous = ring[ring.len() - 1];

    for current in ring.iter().copied() {
        let crosses_y = (current.y > point.y) != (previous.y > point.y);
        if crosses_y {
            let denominator = previous.y - current.y;
            if denominator.abs() > f32::EPSILON {
                let x_intersection =
                    (previous.x - current.x) * (point.y - current.y) / denominator + current.x;
                if point.x < x_intersection {
                    inside = !inside;
                }
            }
        }

        previous = current;
    }

    inside
}

struct TerrainElevationTile<'a> {
    bounds: (DVec2, DVec2),
    terrain_data: &'a MlTerrainTile,
}

impl<'a> TerrainElevationTile<'a> {
    fn new(tile_id: CanonicalTileId, terrain_data: &'a MlTerrainTile) -> Self {
        Self {
            bounds: get_tile_lnglat_bounds(tile_id),
            terrain_data,
        }
    }

    fn elevation_meters(&self, lnglat: DVec2) -> Option<f64> {
        if !lng_lat_bounds_contains(self.bounds, lnglat) {
            return None;
        }

        let bounds_size = self.bounds.1 - self.bounds.0;
        if bounds_size.x == 0.0 || bounds_size.y == 0.0 {
            return None;
        }

        let uv = ((lnglat - self.bounds.0) / bounds_size).as_vec2();
        let uv = vec2(uv.x, 1.0 - uv.y);
        get_dem_elevation(&self.terrain_data.terrain_data, uv).map(f64::from)
    }
}

fn scatter_seed(layer_id: &str, tile_id: CanonicalTileId) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in layer_id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    for value in [tile_id.z, tile_id.x, tile_id.y] {
        hash ^= u64::from(value);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
