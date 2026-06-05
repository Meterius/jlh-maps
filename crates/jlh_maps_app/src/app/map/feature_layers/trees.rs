use crate::app::common::entity_spawner::{EntitySpawner, EntitySpawnerMeta, EntitySpawnerPlugin};
use crate::app::map::core::{MAP_VIEW_COLOR_RENDER_LAYER, MapViewSettings};
use crate::app::map::feature::bucket_layer::TileBucketLayerMeta;
use crate::app::map::feature::bucket_manager::TileBucket;
use crate::app::map::feature::scatter::{
    FeatureTileScatter, FeatureTileScatterConfig, FeatureTileScatterDensityConfig,
};
use crate::app::map::feature::tile::FeatureTile;
use crate::app::map::feature_layers::MapFeatureDistanceVisibility;
use crate::app::map::transform::tile_world_units_per_meter;
use crate::app::maplibre_gl_js::types::CanonicalTileId;
use bevy::asset::{AssetServer, Handle};
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::system::SystemParamItem;
use bevy::gltf::GltfAssetLabel;
use bevy::mesh::{Mesh, Mesh3d};
use bevy::prelude::*;
use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::collections::hash_map::DefaultHasher;
use std::f32::consts::TAU;
use std::hash::{Hash, Hasher};

pub struct TreesPlugin;

impl Plugin for TreesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TreeModelAssets>()
            .add_plugins(EntitySpawnerPlugin::<TreeEntitySpawner>::default())
            .add_systems(Update, sync_trees);
    }
}

const TREE_MODEL_ASSET_PATH: &str = "models/trees.glb";
const TREE_MODEL_COUNT: usize = 7;
const TREE_MODEL_MATERIAL_INDEX: usize = 0;
const TREE_SOURCE_LAYER: &str = "landcover";
const TREE_CLASS_PROPERTY_KEY: &str = "class";
const TREE_ALLOWED_CLASSES: &[&str] = &["forest", "wood"];
const TREE_SAMPLE_RADIUS_METERS: f32 = 16.0;
const TREE_MAX_POINTS_PER_TILE: usize = 4200;
const TREE_SCATTER_CHUNK_EXTENT_METERS: f32 = 256.0;
const TREE_SCATTER_RASTER_CELL_SIZE_METERS: f32 = 4.0;
const TREE_SCATTER_DENSITY_PATCH_SIZE_METERS: f32 = 90.0;
const TREE_SCATTER_DENSITY_MIN_PROBABILITY: f32 = 0.4;
const TREE_SCATTER_DENSITY_MAX_PROBABILITY: f32 = 1.0;
const TREE_SCATTER_DENSITY_CONTRAST: f32 = 1.7;
const TREE_MODEL_SCALE_METERS: f32 = 3.0;
const TREE_MODEL_UNIFORM_SCALE_MIN: f32 = 0.9;
const TREE_MODEL_UNIFORM_SCALE_MAX: f32 = 1.12;
const TREE_MODEL_Z_SCALE_MIN: f32 = 0.9;
const TREE_MODEL_Z_SCALE_MAX: f32 = 1.15;
const TREE_MIN_ZOOM: u32 = 14;

#[derive(Component)]
pub(crate) struct TreeTileBucket;

pub(super) struct TreeTileBucketLayer;

impl TileBucketLayerMeta for TreeTileBucketLayer {
    type BucketMarker = TreeTileBucket;
    type EnabledParams = Res<'static, MapViewSettings>;
    type SpawnParams = Res<'static, TreeModelAssets>;

    fn is_enabled(settings: &SystemParamItem<'_, '_, Self::EnabledParams>) -> bool {
        settings.enable_trees
    }

    fn spawn(
        mut e_commands: EntityCommands,
        tree_assets: &mut SystemParamItem<'_, '_, Self::SpawnParams>,
        _: Entity,
        bucket: &TileBucket,
    ) {
        let entity = e_commands.id();

        e_commands.insert(Name::new("Tree"));

        if bucket.tile_id.z >= TREE_MIN_ZOOM {
            e_commands.insert((
                TreeTile::default(),
                FeatureTile::new(
                    bucket.maplibre_int_id,
                    &bucket.source_id,
                    bucket.tile_id,
                    bucket.center,
                ),
                MapFeatureDistanceVisibility {
                    flat_half_extents: bucket.half_extents,
                },
                EntitySpawner::<TreeEntitySpawner>::new(
                    entity,
                    TreeSpawnerParams {
                        tile_id: bucket.tile_id,
                        scale: tile_world_units_per_meter(bucket.tile_id) as f32
                            * TREE_MODEL_SCALE_METERS,
                        models: tree_assets.models.clone(),
                    },
                    Vec::new(),
                ),
                FeatureTileScatter::new(FeatureTileScatterConfig {
                    layer_id: TREE_SOURCE_LAYER,
                    class_property_key: Some(TREE_CLASS_PROPERTY_KEY),
                    allowed_classes: Some(TREE_ALLOWED_CLASSES),
                    sample_radius_meters: TREE_SAMPLE_RADIUS_METERS,
                    max_points: TREE_MAX_POINTS_PER_TILE,
                    chunk_extent_meters: TREE_SCATTER_CHUNK_EXTENT_METERS,
                    raster_cell_size_meters: TREE_SCATTER_RASTER_CELL_SIZE_METERS,
                    density: Some(FeatureTileScatterDensityConfig {
                        patch_size_meters: TREE_SCATTER_DENSITY_PATCH_SIZE_METERS,
                        min_probability: TREE_SCATTER_DENSITY_MIN_PROBABILITY,
                        max_probability: TREE_SCATTER_DENSITY_MAX_PROBABILITY,
                        contrast: TREE_SCATTER_DENSITY_CONTRAST,
                    }),
                }),
            ));
        }
    }
}

#[derive(Component, Default)]
struct TreeTile {
    scatter_revision: Option<u64>,
}

#[derive(Clone)]
struct TreeModelAsset {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

#[derive(Resource)]
pub(super) struct TreeModelAssets {
    models: Vec<TreeModelAsset>,
}

struct TreeEntitySpawner;

struct TreeSpawnerParams {
    tile_id: CanonicalTileId,
    scale: f32,
    models: Vec<TreeModelAsset>,
}

impl EntitySpawnerMeta for TreeEntitySpawner {
    type Item = Vec3;
    type Params = TreeSpawnerParams;
    type SpawnBundle = (Name, Visibility, RenderLayers);
    type UpdateBundle = (Transform, Mesh3d, MeshMaterial3d<StandardMaterial>);

    fn spawn_bundle(_params: &Self::Params, index: usize, _item: &Self::Item) -> Self::SpawnBundle {
        (
            Name::new(format!("Tree model {index}")),
            Visibility::default(),
            RenderLayers::layer(MAP_VIEW_COLOR_RENDER_LAYER),
        )
    }

    fn update_bundle(params: &Self::Params, index: usize, item: &Self::Item) -> Self::UpdateBundle {
        let mut rng = StdRng::seed_from_u64(tree_instance_seed(params.tile_id, index));
        let model = &params.models[rng.random_range(0..params.models.len())];
        let uniform_scale =
            rng.random_range(TREE_MODEL_UNIFORM_SCALE_MIN..TREE_MODEL_UNIFORM_SCALE_MAX);
        let z_scale = rng.random_range(TREE_MODEL_Z_SCALE_MIN..TREE_MODEL_Z_SCALE_MAX);
        let rotation_z = rng.random_range(0.0..TAU);
        let model_scale = params.scale * uniform_scale;

        (
            Transform::from_translation(*item)
                .with_rotation(Quat::from_rotation_z(rotation_z))
                .with_scale(Vec3::new(model_scale, model_scale, model_scale * z_scale)),
            Mesh3d(model.mesh.clone()),
            MeshMaterial3d(model.material.clone()),
        )
    }
}

impl FromWorld for TreeModelAssets {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.resource::<AssetServer>();
        let models = (0..TREE_MODEL_COUNT)
            .map(|mesh_index| TreeModelAsset {
                mesh: asset_server.load(
                    GltfAssetLabel::Primitive {
                        mesh: mesh_index,
                        primitive: 0,
                    }
                    .from_asset(TREE_MODEL_ASSET_PATH),
                ),
                material: asset_server.load(
                    GltfAssetLabel::Material {
                        index: TREE_MODEL_MATERIAL_INDEX,
                        is_scale_inverted: false,
                    }
                    .from_asset(TREE_MODEL_ASSET_PATH),
                ),
            })
            .collect();

        Self { models }
    }
}

fn sync_trees(
    mut tree_tiles: Query<(
        &FeatureTileScatter,
        &mut TreeTile,
        &mut EntitySpawner<TreeEntitySpawner>,
    )>,
) {
    for (scatter, mut tree_tile, mut spawner) in tree_tiles.iter_mut() {
        let scatter_revision = scatter.data_revision();
        if tree_tile.scatter_revision == Some(scatter_revision) {
            continue;
        }

        spawner.items = scatter
            .positions()
            .map(|positions| positions.to_vec())
            .unwrap_or_default();

        tree_tile.scatter_revision = Some(scatter_revision);
    }
}

fn tree_instance_seed(tile_id: CanonicalTileId, index: usize) -> u64 {
    let mut hasher = DefaultHasher::new();
    tile_id.hash(&mut hasher);
    index.hash(&mut hasher);
    hasher.finish()
}
