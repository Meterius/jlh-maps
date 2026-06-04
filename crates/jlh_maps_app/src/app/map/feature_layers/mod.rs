pub mod buildings;
pub mod trees;
pub mod waters;

use crate::app::map::camera::MapViewCamera;
use crate::app::map::core::MapViewSettings;
use crate::app::map::feature::bucket_layer::TileBucketLayerPlugin;
use crate::app::map::feature::bucket_manager::{TileBucketManager, TileBucketManagerPlugin};
use bevy::app::PostUpdate;
use bevy::math::{Vec2, Vec3, Vec3Swizzles};
use bevy::prelude::{
    App, ChildOf, Component, Entity, GlobalTransform, IntoScheduleConfigs, Plugin, Query, Res,
    TransformSystems, Visibility, Without,
};

pub struct FeatureLayersPlugin;

impl Plugin for FeatureLayersPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            TileBucketManagerPlugin,
            TileBucketLayerPlugin::<waters::WaterTileBucketLayer>::new(),
            TileBucketLayerPlugin::<buildings::BuildingTileBucketLayer>::new(),
            TileBucketLayerPlugin::<trees::TreeTileBucketLayer>::new(),
            buildings::BuildingsPlugin,
            trees::TreesPlugin,
            waters::WatersPlugin,
        ));

        app.add_systems(
            PostUpdate,
            sync_distance_visibility.after(TransformSystems::Propagate),
        );
    }
}

pub fn make_bucket_manager(maplibre_int_id: Entity) -> TileBucketManager {
    TileBucketManager::new(maplibre_int_id, |mut e_commands, _| {
        e_commands.insert((
            waters::WaterTileBucket,
            buildings::BuildingTileBucket,
            trees::TreeTileBucket,
        ));
    })
}

pub const DISABLE_DISTANCE_VISIBILITY: bool = false;

#[derive(Component)]
struct MapFeatureDistanceVisibility {
    flat_half_extents: Vec2,
}

fn sync_distance_visibility(
    cameras: Query<(&MapViewCamera, &GlobalTransform, &ChildOf)>,
    parents: Query<&ChildOf>,
    settings: Res<MapViewSettings>,
    mut distance_visible_entities: Query<
        (
            &MapFeatureDistanceVisibility,
            &GlobalTransform,
            &ChildOf,
            &mut Visibility,
        ),
        Without<MapViewCamera>,
    >,
) {
    for (distance_visibility, entity_transform, ChildOf(entity_parent), mut visibility) in
        distance_visible_entities.iter_mut()
    {
        // TODO: remove hierarchical lookup
        let Some(camera_transform) =
            cameras
                .iter()
                .find_map(|(_, camera_transform, ChildOf(camera_parent))| {
                    has_ancestor_or_self(*entity_parent, *camera_parent, &parents)
                        .then_some(camera_transform)
                })
        else {
            continue;
        };

        let max_distance_squared =
            settings.feature_visibility_distance * settings.feature_visibility_distance;
        let distance_squared = distance_to_flat_bounds_squared(
            camera_transform.translation(),
            entity_transform.translation(),
            distance_visibility.flat_half_extents,
        );

        *visibility = if DISABLE_DISTANCE_VISIBILITY || distance_squared <= max_distance_squared {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

fn has_ancestor_or_self(mut entity: Entity, target: Entity, parents: &Query<&ChildOf>) -> bool {
    loop {
        if entity == target {
            return true;
        }

        let Ok(ChildOf(parent)) = parents.get(entity) else {
            return false;
        };

        entity = *parent;
    }
}

fn distance_to_flat_bounds_squared(point: Vec3, center: Vec3, half_extents: Vec2) -> f32 {
    let local = point - center;
    let outside_xy = (local.xy().abs() - half_extents).max(Vec2::ZERO);

    outside_xy.length_squared() + local.z * local.z
}
