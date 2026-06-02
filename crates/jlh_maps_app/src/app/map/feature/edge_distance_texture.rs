use crate::app::map::feature::bucket::FeatureTileBucket;
use crate::app::map::feature::utils::poly::ring_without_closing_position;
use crate::app::map::transform::tile_uv;
use crate::app::maplibre_gl_js::integration::MaplibreMapIntegration;
use crate::app::maplibre_gl_js::types::{MlTile, MlTileFeature};
use crate::app::maplibre_gl_js::utils::tile::get_tile_lnglat_bounds;
use crate::app::task_pool::AppTaskPool;
use crate::utils::edge_distance::update_edge_distance_texture;
use crate::wasm_task_pool::Task;
use bevy::asset::{Assets, Handle, RenderAssetUsages};
use bevy::image::{Image, ImageSampler};
use bevy::math::{UVec2, dvec2};
use bevy::prelude::{Component, Plugin, Query, Res, ResMut, Update};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use geojson::Value;
use std::sync::Arc;

const EDGE_DISTANCE_MAX_UV: f32 = 0.01;

pub struct FeatureEdgeDistanceTexturePlugin;

impl Plugin for FeatureEdgeDistanceTexturePlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_systems(Update, sync_textures);
    }
}

// ECS

#[derive(Component)]
pub struct FeatureTileBucketEdgeDistanceTexture {
    pub texture: Handle<Image>,
    resolution: UVec2,
    data: Vec<f32>,
    dirty: bool,
    tile_revision: Option<u64>,
    pending_edge_distance_task: Option<Task<Vec<f32>>>,
}

impl FeatureTileBucketEdgeDistanceTexture {
    pub fn new(resolution: UVec2, images: &mut Assets<Image>) -> Self {
        let resolution = resolution.max(UVec2::ONE);
        let data = vec![0.0; (resolution.x * resolution.y) as usize];
        let texture = images.add(make_image(resolution, &data));

        Self {
            texture,
            resolution,
            data,
            dirty: true,
            tile_revision: None,
            pending_edge_distance_task: None,
        }
    }

    fn clear_data(&mut self) {
        self.data.fill(0.0);
        self.dirty = true;
        self.pending_edge_distance_task = None;
    }

    fn clear_component(&mut self) {
        self.clear_data();
        self.dirty = false;
        self.tile_revision = None;
        self.pending_edge_distance_task = None;
    }
}

fn sync_textures(
    map_ints: Query<&MaplibreMapIntegration>,
    task_pool: Res<AppTaskPool>,
    mut buckets: Query<(
        &FeatureTileBucket,
        &mut FeatureTileBucketEdgeDistanceTexture,
    )>,
    mut images: ResMut<Assets<Image>>,
) {
    for (bucket, mut edge_texture) in buckets.iter_mut() {
        let Some(map_int) = map_ints.get(bucket.maplibre_int_id).ok() else {
            continue;
        };

        sync_texture(map_int, bucket, &mut edge_texture, &mut images, &task_pool);
    }
}

fn sync_texture(
    map_int: &MaplibreMapIntegration,
    bucket: &FeatureTileBucket,
    edge_texture: &mut FeatureTileBucketEdgeDistanceTexture,
    images: &mut Assets<Image>,
    task_pool: &AppTaskPool,
) {
    // apply texture from task returns
    if let Some(data) = edge_texture
        .pending_edge_distance_task
        .as_mut()
        .and_then(|pending_task| pending_task.poll_once())
    {
        edge_texture.pending_edge_distance_task = None;
        edge_texture.data = data;
        apply_texture(edge_texture, images);
    }

    // delete texture if tile no longer exists
    let Some(tile) = bucket.tile(map_int) else {
        if edge_texture.tile_revision.is_some() {
            edge_texture.clear_component();
        }
        return;
    };

    // check if tile or terrain data has changed

    if edge_texture.tile_revision != Some(tile.revision) {
        edge_texture.clear_data();
        edge_texture.tile_revision = Some(tile.revision);
    }

    if !edge_texture.dirty {
        return;
    }

    // data has changed, start texture rebuild task

    let bounds = get_tile_lnglat_bounds(bucket.tile_id);
    let resolution = edge_texture.resolution;
    let tile = Arc::clone(tile);
    let task = task_pool.spawn(move || build_texture(tile, bounds, resolution));
    edge_texture.pending_edge_distance_task = Some(task);
    edge_texture.dirty = false;
}

// Texture Construction / Application

fn apply_texture(edge_texture: &FeatureTileBucketEdgeDistanceTexture, images: &mut Assets<Image>) {
    if let Some(image) = images.get_mut(&edge_texture.texture) {
        *image = make_image(edge_texture.resolution, &edge_texture.data);
    }
}

fn build_texture(
    tile: Arc<MlTile>,
    bounds: (bevy::math::DVec2, bevy::math::DVec2),
    resolution: UVec2,
) -> Vec<f32> {
    let mut data = vec![0.0; (resolution.x * resolution.y) as usize];
    let edges = build_features_edge_segments(bounds, tile.features.values());
    update_edge_distance_texture(
        &edges,
        &mut data,
        resolution.x as usize,
        resolution.y as usize,
        EDGE_DISTANCE_MAX_UV,
    );
    data
}

fn build_features_edge_segments<'a>(
    bounds: (bevy::math::DVec2, bevy::math::DVec2),
    features: impl IntoIterator<Item = &'a MlTileFeature>,
) -> Vec<f32> {
    let mut edges = Vec::new();
    for feature in features {
        match &feature.geometry.value {
            Value::Polygon(polygon) => push_polygon_edge_segments(bounds, polygon, &mut edges),
            Value::MultiPolygon(polygons) => {
                for polygon in polygons {
                    push_polygon_edge_segments(bounds, polygon, &mut edges);
                }
            }
            _ => {}
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
            .map(|position| tile_uv(bounds, dvec2(position[0], position[1])))
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

fn make_image(resolution: UVec2, data: &[f32]) -> Image {
    let format = TextureFormat::R32Float;
    let mut bytes = Vec::with_capacity(size_of_val(data));
    for value in data {
        bytes.extend_from_slice(&value.to_ne_bytes());
    }

    let mut image = Image::new(
        Extent3d {
            width: resolution.x,
            height: resolution.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        bytes,
        format,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::linear();
    image
}
