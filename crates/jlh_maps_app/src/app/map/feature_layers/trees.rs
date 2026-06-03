use crate::app::map::core::{MAP_VIEW_COLOR_RENDER_LAYER, MapViewSettings};
use crate::app::map::feature::bucket_layer::TileBucketLayerMeta;
use crate::app::map::feature::bucket_manager::TileBucket;
use crate::app::map::feature::scatter::{FeatureTileScatter, FeatureTileScatterConfig};
use crate::app::map::feature::tile::FeatureTile;
use crate::app::map::transform::tile_world_units_per_meter;
use bevy::asset::{Assets, Handle};
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Mesh, Mesh3d};
use bevy::prelude::*;

pub struct TreesPlugin;

impl Plugin for TreesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TreeSphereAssets>()
            .add_systems(PostUpdate, sync_tree_spheres);
    }
}

const TREE_SOURCE_LAYER: &str = "landcover";
const TREE_CLASS_PROPERTY_KEY: &str = "class";
const TREE_ALLOWED_CLASSES: &[&str] = &["forest", "wood"];
const TREE_SAMPLE_RADIUS_METERS: f32 = 25.0;
const TREE_MAX_POINTS_PER_TILE: usize = 800;
const TREE_SCATTER_CHUNK_EXTENT_METERS: f32 = 256.0;
const TREE_SCATTER_RASTER_CELL_SIZE_METERS: f32 = 4.0;
const TREE_SPHERE_RADIUS_METERS: f32 = 10.0;

#[derive(Component)]
pub(crate) struct TreeTileBucket;

pub(super) struct TreeTileBucketLayer;

impl TileBucketLayerMeta for TreeTileBucketLayer {
    type BucketMarker = TreeTileBucket;
    type EnabledParams = Res<'static, MapViewSettings>;
    type SpawnParams = ();

    fn is_enabled(settings: &SystemParamItem<'_, '_, Self::EnabledParams>) -> bool {
        settings.enable_trees
    }

    fn spawn(
        mut e_commands: EntityCommands,
        _params: &mut SystemParamItem<'_, '_, Self::SpawnParams>,
        _: Entity,
        bucket: &TileBucket,
    ) {
        e_commands.insert((
            Name::new(format!(
                "Tree tile {}/{:?}",
                bucket.source_id, bucket.tile_id
            )),
            TreeTile::default(),
            FeatureTile::new(
                bucket.maplibre_int_id,
                &bucket.source_id,
                bucket.tile_id,
                bucket.center,
            ),
            FeatureTileScatter::new(FeatureTileScatterConfig {
                layer_id: TREE_SOURCE_LAYER,
                class_property_key: Some(TREE_CLASS_PROPERTY_KEY),
                allowed_classes: Some(TREE_ALLOWED_CLASSES),
                sample_radius_meters: TREE_SAMPLE_RADIUS_METERS,
                max_points: TREE_MAX_POINTS_PER_TILE,
                chunk_extent_meters: TREE_SCATTER_CHUNK_EXTENT_METERS,
                raster_cell_size_meters: TREE_SCATTER_RASTER_CELL_SIZE_METERS,
            }),
        ));
    }
}

#[derive(Component, Default)]
struct TreeTile {
    scatter_revision: Option<u64>,
    spawned_spheres: Vec<Entity>,
}

#[derive(Component)]
struct TreeSphere;

#[derive(Resource)]
struct TreeSphereAssets {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

impl FromWorld for TreeSphereAssets {
    fn from_world(world: &mut World) -> Self {
        let mesh = {
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            meshes.add(Sphere::new(1.0).mesh().ico(2).expect("valid sphere mesh"))
        };
        let material = {
            let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
            materials.add(StandardMaterial {
                base_color: Color::srgb(0.18, 0.46, 0.16),
                perceptual_roughness: 0.9,
                reflectance: 0.05,
                ..default()
            })
        };

        Self { mesh, material }
    }
}

fn sync_tree_spheres(
    mut commands: Commands,
    sphere_assets: Res<TreeSphereAssets>,
    mut tree_tiles: Query<(Entity, &FeatureTile, &FeatureTileScatter, &mut TreeTile)>,
) {
    for (entity, feature_tile, scatter, mut tree_tile) in tree_tiles.iter_mut() {
        let scatter_revision = scatter.data_revision();
        if tree_tile.scatter_revision == Some(scatter_revision) {
            continue;
        }

        for sphere in tree_tile.spawned_spheres.drain(..) {
            commands.entity(sphere).despawn();
        }

        if let Some(positions) = scatter.positions() {
            let radius =
                tile_world_units_per_meter(feature_tile.tile_id) as f32 * TREE_SPHERE_RADIUS_METERS;
            for (index, position) in positions.iter().enumerate() {
                let sphere = commands
                    .spawn((
                        Name::new(format!("Tree sphere {index}")),
                        Transform::from_translation(*position).with_scale(Vec3::splat(radius)),
                        Visibility::default(),
                        RenderLayers::layer(MAP_VIEW_COLOR_RENDER_LAYER),
                        Mesh3d(sphere_assets.mesh.clone()),
                        MeshMaterial3d(sphere_assets.material.clone()),
                        NotShadowCaster,
                        TreeSphere,
                    ))
                    .id();
                commands.entity(entity).add_child(sphere);
                tree_tile.spawned_spheres.push(sphere);
            }
        }

        tree_tile.scatter_revision = Some(scatter_revision);
    }
}
