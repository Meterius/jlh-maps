use crate::app::map::feature::tile_task_based::{
    TileTaskBased, TileTaskBasedMeta, TileTaskBasedPlugin,
};
use crate::app::map::feature::utils::poly::ring_without_closing_position;
use crate::app::map::transform::{
    lng_lat_from_world_xy, tile_flat_world_center, tile_world_units_per_meter,
    world_from_lng_lat_alt,
};
use crate::app::maplibre_gl_js::types::{CanonicalTileId, MlTerrainTile, MlTile, MlTileFeature};
use crate::app::maplibre_gl_js::utils::mercator_coordinate::{
    lng_lat_is_in_bounds, tile_uv_from_lng_lat,
};
use crate::app::maplibre_gl_js::utils::terrain::get_terrain_elevation;
use crate::app::maplibre_gl_js::utils::tile::get_tile_lnglat_bounds;
use bevy::app::App;
use bevy::ecs::system::SystemParamItem;
use bevy::math::{DVec3, Vec2, Vec3, Vec3Swizzles};
use bevy::prelude::{Entity, Plugin};
use geojson::{JsonValue, Value};
use map_scatter::prelude::{
    FieldGraphSpec, FieldProgramCache, FieldSemantics, Kind, Layer, NodeSpec, PoissonDiskSampling,
    RunConfig, ScatterRunner, Texture, TextureChannel, TextureRegistry,
};
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

const GEOMETRY_MASK_TEXTURE_ID: &str = "geometry_mask";
const DENSITY_PATCH_TEXTURE_ID: &str = "density_patch";
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
pub struct FeatureTileScatterDensityConfig {
    pub patch_size_meters: f32,
    pub min_probability: f32,
    pub max_probability: f32,
    pub contrast: f32,
}

#[derive(Clone, Copy)]
pub struct FeatureTileScatterConfig {
    pub layer_id: &'static str,

    pub class_property_key: Option<&'static str>,
    pub allowed_classes: Option<&'static [&'static str]>,

    pub sample_radius_meters: f32,
    pub max_points: usize,
    pub chunk_extent_meters: f32,
    pub raster_cell_size_meters: f32,

    pub density: Option<FeatureTileScatterDensityConfig>,
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
            density: None,
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
        terrain_tile: Option<Arc<MlTerrainTile>>,
        config: Self::Config,
    ) -> Self::Data {
        build_scatter_points(tile, terrain_tile.as_deref(), config)
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

    let (_, half_extents) = crate::app::map::transform::tile_flat_world_bounds(tile.id);
    let domain_extent = half_extents * 2.0;
    if domain_extent.x <= 0.0 || domain_extent.y <= 0.0 {
        return FeatureScatterPoints::default();
    }

    let mut textures = TextureRegistry::new();
    textures.register(GEOMETRY_MASK_TEXTURE_ID, polygon_mask);
    let has_density_patch = if let Some(density_config) = config.density {
        let patch_size = density_config.patch_size_meters * world_units_per_meter;
        if patch_size.is_finite() && patch_size > 0.0 {
            textures.register(
                DENSITY_PATCH_TEXTURE_ID,
                DensityPatchTexture::new(scatter_seed(tile.id), patch_size, density_config),
            );
            true
        } else {
            false
        }
    } else {
        false
    };

    let cache = FieldProgramCache::new();

    let mut rng = StdRng::seed_from_u64(scatter_seed(tile.id));

    let plan = PlanBuilder::tree_plan(
        config.sample_radius_meters * world_units_per_meter,
        has_density_patch,
    );

    let run_config = RunConfig::new(domain_extent)
        .with_chunk_extent((config.chunk_extent_meters * world_units_per_meter).max(1.0))
        .with_raster_cell_size((config.raster_cell_size_meters * world_units_per_meter).max(0.01))
        .with_grid_halo(2);

    let mut runner = ScatterRunner::new(run_config, &textures, &cache);

    let result = runner.run(&plan, &mut rng);

    let center = tile_flat_world_center(tile.id);
    let bounds = get_tile_lnglat_bounds(tile.id);

    let positions = result
        .placements
        .into_iter()
        .take(config.max_points)
        .map(|placement| {
            let local_xy = placement.position;
            let world_xy = center.xy() + local_xy.as_dvec2();
            let lnglat = lng_lat_from_world_xy(world_xy);

            let terrain_altitude = terrain_data
                .filter(|_| lng_lat_is_in_bounds(bounds, lnglat))
                .and_then(|terrain_data| {
                    get_terrain_elevation(
                        &terrain_data.terrain_data,
                        tile_uv_from_lng_lat(bounds, lnglat),
                    )
                })
                .map(f64::from)
                .unwrap_or(0.0);

            let z = world_from_lng_lat_alt(lnglat.x, lnglat.y, terrain_altitude).z - center.z;
            Vec3::new(local_xy.x, local_xy.y, z as f32)
        })
        .collect();

    FeatureScatterPoints { positions }
}

struct PlanBuilder;

impl PlanBuilder {
    fn tree_plan(sample_radius: f32, has_density_patch: bool) -> map_scatter::prelude::Plan {
        let mut spec = FieldGraphSpec::default();
        spec.add_with_semantics(
            "inside_geometry",
            NodeSpec::texture(GEOMETRY_MASK_TEXTURE_ID, TextureChannel::R),
            FieldSemantics::Gate,
        );

        let probability = if has_density_patch {
            NodeSpec::texture(DENSITY_PATCH_TEXTURE_ID, TextureChannel::R)
        } else {
            NodeSpec::constant(1.0)
        };
        spec.add_with_semantics("probability", probability, FieldSemantics::Probability);

        let layer = Layer::new_with(
            SCATTER_LAYER_ID,
            vec![Kind::new(SCATTER_KIND_ID, spec)],
            PoissonDiskSampling::new(sample_radius.max(0.01)),
        );

        map_scatter::prelude::Plan::new().with_layer(layer)
    }
}

struct DensityPatchTexture {
    seed: u64,
    patch_size: f32,
    min_probability: f32,
    max_probability: f32,
    contrast: f32,
}

impl DensityPatchTexture {
    fn new(seed: u64, patch_size: f32, config: FeatureTileScatterDensityConfig) -> Self {
        let min_probability = config.min_probability.clamp(0.0, 1.0);
        let max_probability = config.max_probability.clamp(min_probability, 1.0);

        Self {
            seed,
            patch_size,
            min_probability,
            max_probability,
            contrast: config.contrast.max(0.01),
        }
    }
}

impl Texture for DensityPatchTexture {
    fn sample(&self, _channel: TextureChannel, p: Vec2) -> f32 {
        let coarse = value_noise(p / self.patch_size, self.seed, 0);
        let detail = value_noise(p / (self.patch_size * 0.45), self.seed, 1);
        let density = (coarse * 0.75 + detail * 0.25)
            .clamp(0.0, 1.0)
            .powf(self.contrast);

        self.min_probability + (self.max_probability - self.min_probability) * density
    }
}

fn value_noise(p: Vec2, seed: u64, octave: u64) -> f32 {
    let cell_x = p.x.floor();
    let cell_y = p.y.floor();
    let local_x = p.x - cell_x;
    let local_y = p.y - cell_y;

    let x0 = cell_x as i64;
    let y0 = cell_y as i64;
    let x1 = x0 + 1;
    let y1 = y0 + 1;

    let tx = smootherstep(local_x);
    let ty = smootherstep(local_y);

    let bottom = lerp(
        cell_random(seed, octave, x0, y0),
        cell_random(seed, octave, x1, y0),
        tx,
    );
    let top = lerp(
        cell_random(seed, octave, x0, y1),
        cell_random(seed, octave, x1, y1),
        tx,
    );

    lerp(bottom, top, ty)
}

fn cell_random(seed: u64, octave: u64, x: i64, y: i64) -> f32 {
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    octave.hash(&mut hasher);
    x.hash(&mut hasher);
    y.hash(&mut hasher);
    (hasher.finish() as f64 / u64::MAX as f64) as f32
}

fn smootherstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[derive(Default)]
struct PolygonMaskTexture {
    polygons: Vec<MaskPolygon>,
}

impl PolygonMaskTexture {
    fn from_tile_layer(tile: &MlTile, config: &FeatureTileScatterConfig) -> Self {
        let center = tile_flat_world_center(tile.id);
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
        if self.polygons.iter().any(|polygon| polygon.contains(p)) {
            1.0
        } else {
            0.0
        }
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

            let world = world_from_lng_lat_alt(position[0], position[1], 0.0) - center;
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

fn scatter_seed(tile_id: CanonicalTileId) -> u64 {
    let mut hasher = DefaultHasher::new();
    tile_id.hash(&mut hasher);
    hasher.finish()
}
