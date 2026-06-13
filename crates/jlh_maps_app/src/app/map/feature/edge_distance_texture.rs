use crate::app::map::feature::tile_task_based::{
    TileTaskBased, TileTaskBasedMeta, TileTaskBasedPlugin,
};
use crate::app::map::feature::utils::poly::ring_without_closing_position;
use crate::app::maplibre_gl_js::types::{MlTerrainTile, MlTile, MlTileFeature};
use crate::app::maplibre_gl_js::utils::mercator_coordinate::tile_uv_from_lng_lat;
use crate::app::maplibre_gl_js::utils::tile::get_tile_lnglat_bounds;
use crate::utils::edge_distance::update_edge_distance_texture;
use bevy::asset::{Assets, Handle, RenderAssetUsages};
use bevy::ecs::system::SystemParamItem;
use bevy::image::{Image, ImageSampler};
use bevy::math::{UVec2, dvec2};
use bevy::prelude::{Entity, Plugin, ResMut, uvec2};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use geo::algorithm::unary_union;
use geo_types::Polygon as GeoPolygon;
use geojson::Value;
use std::sync::Arc;

const EDGE_DISTANCE_MAX_UV: f32 = 0.01;

pub struct FeatureEdgeDistanceTexturePlugin;

impl Plugin for FeatureEdgeDistanceTexturePlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_plugins(TileTaskBasedPlugin::<FeatureTileEdgeDistanceTextureMeta>::new());
    }
}

// ECS

pub type FeatureTileEdgeDistanceTexture = TileTaskBased<FeatureTileEdgeDistanceTextureMeta>;

pub struct FeatureTileEdgeDistanceTextureMeta;

#[derive(Clone, Copy)]
pub struct FeatureTileEdgeDistanceTextureConfig {
    pub layer_id: &'static str,
    pub resolution: UVec2,
}

impl Default for FeatureTileEdgeDistanceTextureConfig {
    fn default() -> Self {
        Self {
            layer_id: "",
            resolution: UVec2::ONE,
        }
    }
}

pub struct FeatureTileEdgeDistanceTextureState {
    texture: Handle<Image>,
}

impl FeatureTileEdgeDistanceTexture {
    pub fn new(layer_id: &'static str, resolution: UVec2, images: &mut Assets<Image>) -> Self {
        let resolution = resolution.max(UVec2::ONE);
        let texture = images.add(make_default_image());

        TileTaskBased::from_parts(
            FeatureTileEdgeDistanceTextureConfig {
                layer_id,
                resolution,
            },
            FeatureTileEdgeDistanceTextureState { texture },
        )
    }

    pub fn texture(&self) -> &Handle<Image> {
        &self.state().texture
    }
}

impl TileTaskBasedMeta for FeatureTileEdgeDistanceTextureMeta {
    type Data = Image;
    type State = FeatureTileEdgeDistanceTextureState;
    type Config = FeatureTileEdgeDistanceTextureConfig;
    type ApplyParams = ResMut<'static, Assets<Image>>;

    fn use_terrain() -> bool {
        false
    }

    fn build_data(
        tile: Arc<MlTile>,
        _terrain_tile: Option<Arc<MlTerrainTile>>,
        config: Self::Config,
    ) -> Self::Data {
        build_texture_image(tile, config.layer_id, config.resolution)
    }

    fn apply_data(
        _entity_eid: Entity,
        images: &mut SystemParamItem<'_, '_, Self::ApplyParams>,
        _config: &Self::Config,
        state: &mut Self::State,
        image: Option<Self::Data>,
    ) -> Option<Self::Data> {
        apply_texture(state, image, images);
        None
    }
}

// Texture Construction / Application

fn apply_texture(
    state: &FeatureTileEdgeDistanceTextureState,
    image: Option<Image>,
    images: &mut Assets<Image>,
) {
    if let Some(target_image) = images.get_mut(&state.texture) {
        *target_image = image.unwrap_or_else(make_default_image);
    }
}

fn build_texture_image(tile: Arc<MlTile>, layer_id: &'static str, resolution: UVec2) -> Image {
    let bounds = get_tile_lnglat_bounds(tile.id);
    let mut data = vec![0.0; (resolution.x * resolution.y) as usize];
    let edges = build_features_edge_segments(
        bounds,
        tile.layers
            .get(layer_id)
            .iter()
            .flat_map(|layer| layer.features.values()),
    );
    update_edge_distance_texture(
        &edges,
        &mut data,
        resolution.x as usize,
        resolution.y as usize,
        EDGE_DISTANCE_MAX_UV,
    );
    make_image(resolution, data)
}

fn build_features_edge_segments<'a>(
    bounds: (bevy::math::DVec2, bevy::math::DVec2),
    features: impl IntoIterator<Item = &'a MlTileFeature>,
) -> Vec<f32> {
    let mut geo_polygons: Vec<GeoPolygon<f64>> = Vec::new();
    for feature in features {
        match &feature.geometry.value {
            Value::Polygon(polygon) => {
                if let Ok(polygon) = GeoPolygon::try_from(Value::Polygon(polygon.clone())) {
                    geo_polygons.push(polygon);
                }
            }
            Value::MultiPolygon(polygons) => {
                for polygon in polygons {
                    if let Ok(polygon) = GeoPolygon::try_from(Value::Polygon(polygon.clone())) {
                        geo_polygons.push(polygon);
                    }
                }
            }
            _ => {}
        }
    }

    let mut edges = Vec::new();
    if geo_polygons.is_empty() {
        return edges;
    }

    let merged_geometry = unary_union(&geo_polygons);
    for polygon in &merged_geometry.0 {
        if let Value::Polygon(polygon) = Value::from(polygon) {
            push_polygon_edge_segments(bounds, &polygon, &mut edges);
        }
    }

    edges
}

fn push_polygon_edge_segments(
    bounds: (bevy::math::DVec2, bevy::math::DVec2),
    polygon: &[Vec<Vec<f64>>],
    edges: &mut Vec<f32>,
) {
    for ring in polygon {
        let ring_positions = ring_without_closing_position(ring);
        let uvs = ring_positions
            .iter()
            .filter(|&position| position.len() >= 2)
            .map(|position| tile_uv_from_lng_lat(bounds, dvec2(position[0], position[1])))
            .collect::<Vec<_>>();

        if uvs.len() < 2 {
            continue;
        }

        for index in 0..uvs.len() {
            let a = uvs[index];
            let b = uvs[(index + 1) % uvs.len()];
            edges.extend([a.x, a.y, b.x, b.y]);
        }
    }
}

// Utils

fn make_image(resolution: UVec2, data: Vec<f32>) -> Image {
    let mut bytes = Vec::with_capacity(data.len() * size_of::<f32>());
    for value in data {
        bytes.extend_from_slice(&value.to_ne_bytes());
    }

    make_image_from_bytes(resolution, bytes)
}

fn make_default_image() -> Image {
    make_image_from_bytes(uvec2(1, 1), 1.0f32.to_ne_bytes().to_vec())
}

fn make_image_from_bytes(resolution: UVec2, bytes: Vec<u8>) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: resolution.x,
            height: resolution.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        bytes,
        TextureFormat::R32Float,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::linear();
    image
}
