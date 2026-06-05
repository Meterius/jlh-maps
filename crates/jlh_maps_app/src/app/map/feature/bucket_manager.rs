use crate::app::map::transform::tile_flat_world_bounds;
use crate::app::maplibre_gl_js::types::{CanonicalTileId, MlData};
use crate::utils::debug::SoftExpect;
use bevy::app::{App, Plugin};
use bevy::math::DVec3;
use bevy::prelude::{
    Commands, Component, Entity, EntityCommands, IntoScheduleConfigs, Name, Query, SystemSet,
    Transform, Update, Vec2, Visibility,
};
use big_space::grid::Grid;
use std::collections::HashMap;

pub type TileBucketOnSpawn = fn(EntityCommands, &TileBucket);

pub struct TileBucketManagerPlugin;

impl Plugin for TileBucketManagerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            sync_spawned_buckets.in_set(TileBucketSystems::SyncBuckets),
        );
    }
}

#[derive(SystemSet, Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum TileBucketSystems {
    SyncBuckets,
    SyncLayers,
}

#[derive(Component)]
pub struct TileBucketManager {
    pub maplibre_int_id: Entity,
    on_spawn: TileBucketOnSpawn,
    spawned_buckets_by_source: HashMap<String, HashMap<CanonicalTileId, Entity>>,
}

impl TileBucketManager {
    pub fn new(maplibre_int_id: Entity, on_spawn: TileBucketOnSpawn) -> Self {
        Self {
            maplibre_int_id,
            on_spawn,
            spawned_buckets_by_source: HashMap::default(),
        }
    }
}

#[derive(Clone, Component)]
pub struct TileBucket {
    pub maplibre_int_id: Entity,
    pub source_id: String,
    pub tile_id: CanonicalTileId,

    pub center: DVec3,
    pub half_extents: Vec2,
}

fn sync_spawned_buckets(
    mut commands: Commands,
    ml_data_query: Query<&MlData>,
    mut managers: Query<(Entity, &Grid, &mut TileBucketManager)>,
) {
    for (manager_id, grid, mut manager) in managers.iter_mut() {
        let Some(ml_data) = ml_data_query
            .get(manager.maplibre_int_id)
            .ok()
            .soft_expect("")
        else {
            continue;
        };

        let maplibre_int_id = manager.maplibre_int_id;
        let on_spawn = manager.on_spawn;
        for (source_id, source) in &ml_data.sources {
            let spawned_buckets = manager
                .spawned_buckets_by_source
                .entry(source_id.clone())
                .or_default();

            for tile_id in source
                .renderable_tile_ids
                .iter()
                .filter(|tile_id| source.tiles.contains_key(*tile_id))
            {
                if spawned_buckets.contains_key(tile_id) {
                    continue;
                }

                let bucket_eid = spawn_bucket(
                    &mut commands,
                    maplibre_int_id,
                    manager_id,
                    grid,
                    source_id,
                    *tile_id,
                    on_spawn,
                );
                spawned_buckets.insert(*tile_id, bucket_eid);
            }
        }

        manager
            .spawned_buckets_by_source
            .retain(|source_id, spawned_buckets| {
                let source = ml_data.sources.get(source_id);

                spawned_buckets.retain(|tile_id, bucket_eid| {
                    let Some(source) = source else {
                        commands.entity(*bucket_eid).despawn();
                        return false;
                    };

                    if !source.tiles.contains_key(tile_id) {
                        commands.entity(*bucket_eid).despawn();
                        return false;
                    }

                    let visibility = if source.renderable_tile_ids.contains(tile_id) {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    };
                    commands.entity(*bucket_eid).insert(visibility);

                    true
                });

                source.is_some()
            });
    }
}

fn spawn_bucket(
    commands: &mut Commands,
    maplibre_int_id: Entity,
    manager_id: Entity,
    grid: &Grid,
    source_id: &str,
    tile_id: CanonicalTileId,
    on_spawn: TileBucketOnSpawn,
) -> Entity {
    let (center, half_extents) = tile_flat_world_bounds(tile_id);
    let (cell, translation) = grid.translation_to_grid(center);

    let bucket = TileBucket {
        maplibre_int_id,
        source_id: source_id.to_owned(),
        tile_id,
        center,
        half_extents,
    };

    let bucket_id = commands
        .spawn((
            Name::new(format!("Bucket {tile_id:?} ({source_id})")),
            cell,
            Transform::from_translation(translation),
            Visibility::default(),
            bucket.clone(),
        ))
        .id();

    on_spawn(commands.entity(bucket_id), &bucket);
    commands.entity(manager_id).add_child(bucket_id);

    bucket_id
}
