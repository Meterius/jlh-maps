use crate::app::map::transform::tile_flat_bounds_world;
use crate::app::maplibre_gl_js::integration::MaplibreMapIntegration;
use crate::app::maplibre_gl_js::types::CanonicalTileId;
use crate::utils::debug::SoftExpect;
use bevy::app::{App, Plugin};
use bevy::ecs::system::{StaticSystemParam, SystemParam, SystemParamItem};
use bevy::math::DVec3;
use bevy::prelude::{
    ChildOf, Commands, Component, Entity, EntityCommands, Name, Query, Transform, Update, Vec2,
};
use bevy::prelude::{IntoScheduleConfigs, Visibility};
use big_space::grid::Grid;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::marker::PhantomData;

pub trait TileBucketManagerMeta: Send + Sync + Sized + 'static {
    type TileKind: Clone + Copy + PartialEq + Eq + Hash + Send + Sync + 'static;

    fn tile_kinds() -> &'static [Self::TileKind];

    type TileKindEnabledParams: SystemParam + 'static;

    fn is_tile_kind_enabled(
        params: &SystemParamItem<'_, '_, Self::TileKindEnabledParams>,
        kind: Self::TileKind,
    ) -> bool;

    type InitializeTileParams: SystemParam + 'static;

    fn initialize_tile(
        e_commands: EntityCommands,
        params: &mut SystemParamItem<'_, '_, Self::InitializeTileParams>,
        bucket: &TileBucket<Self>,
        kind: Self::TileKind,
    );
}

pub struct TileBucketManagerPlugin<C: TileBucketManagerMeta>(PhantomData<C>);

impl<C: TileBucketManagerMeta> Default for TileBucketManagerPlugin<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: TileBucketManagerMeta> TileBucketManagerPlugin<C> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<C: TileBucketManagerMeta> Plugin for TileBucketManagerPlugin<C> {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (sync_enabled_kinds::<C>, sync_spawned_buckets::<C>).chain(),
        );
    }
}

#[derive(Component)]
pub struct TileBucketManager<C: TileBucketManagerMeta> {
    pub maplibre_int_id: Entity,
    enabled_kinds: HashSet<C::TileKind>,
    spawned_buckets_by_source: HashMap<String, HashMap<CanonicalTileId, Entity>>,
    _marker: PhantomData<C>,
}

impl<C: TileBucketManagerMeta> TileBucketManager<C> {
    pub fn new(maplibre_int_id: Entity) -> Self {
        Self {
            maplibre_int_id,
            spawned_buckets_by_source: HashMap::default(),
            enabled_kinds: HashSet::default(),
            _marker: PhantomData,
        }
    }
}

#[derive(Component)]
pub struct TileBucket<C: TileBucketManagerMeta> {
    pub maplibre_int_id: Entity,
    pub source_id: String,
    pub tile_id: CanonicalTileId,

    pub center: DVec3,
    pub half_extents: Vec2,

    _marker: PhantomData<C>,
}

#[derive(Component)]
pub struct TileBucketTile<C: TileBucketManagerMeta> {
    kind: C::TileKind,
    manager_id: Entity,
}

#[allow(clippy::type_complexity)]
fn sync_enabled_kinds<C: TileBucketManagerMeta>(
    mut commands: Commands,
    enabled_params: StaticSystemParam<C::TileKindEnabledParams>,
    mut initialize_params: StaticSystemParam<C::InitializeTileParams>,
    mut managers: Query<(Entity, &mut TileBucketManager<C>)>,
    buckets: Query<(Entity, &TileBucket<C>, &ChildOf)>,
    tiles: Query<(Entity, &TileBucketTile<C>)>,
) {
    for (manager_id, mut manager) in managers.iter_mut() {
        for kind in C::tile_kinds() {
            let is_enabled = C::is_tile_kind_enabled(&*enabled_params, *kind);
            let was_enabled = manager.enabled_kinds.contains(kind);

            if is_enabled == was_enabled {
                continue;
            }

            if is_enabled {
                manager.enabled_kinds.insert(*kind);

                for (bucket_id, bucket, ChildOf(bucket_manager_id)) in buckets.iter() {
                    if *bucket_manager_id != manager_id {
                        continue;
                    }

                    spawn_kind(
                        &mut commands,
                        &mut *initialize_params,
                        bucket,
                        bucket_id,
                        *kind,
                        manager_id,
                    );
                }
            } else {
                manager.enabled_kinds.remove(kind);

                for (tile_id, tile) in tiles.iter() {
                    if tile.manager_id == manager_id && tile.kind == *kind {
                        commands.entity(tile_id).despawn();
                    }
                }
            }
        }
    }
}

fn sync_spawned_buckets<C: TileBucketManagerMeta>(
    mut commands: Commands,
    map_ints: Query<&MaplibreMapIntegration>,
    mut initialize_params: StaticSystemParam<C::InitializeTileParams>,
    mut managers: Query<(Entity, &Grid, &mut TileBucketManager<C>)>,
) {
    for (manager_id, grid, mut manager) in managers.iter_mut() {
        let Some(map_int) = map_ints.get(manager.maplibre_int_id).ok().soft_expect("") else {
            continue;
        };

        // handle bucket spawning
        let maplibre_int_id = manager.maplibre_int_id;
        let enabled_kinds = manager.enabled_kinds.clone();
        for (source_id, source) in &map_int.data.sources {
            let spawned_buckets = manager
                .spawned_buckets_by_source
                .entry(source_id.clone())
                .or_default();

            for tile_id in source.tiles.keys() {
                if spawned_buckets.contains_key(tile_id) {
                    continue;
                }

                let bucket_eid = spawn_bucket::<C>(
                    &mut commands,
                    &mut *initialize_params,
                    maplibre_int_id,
                    manager_id,
                    grid,
                    source_id,
                    *tile_id,
                    &enabled_kinds,
                );
                spawned_buckets.insert(*tile_id, bucket_eid);
            }
        }

        // handle bucket removal
        manager
            .spawned_buckets_by_source
            .retain(|source_id, spawned_buckets| {
                let source = map_int.data.sources.get(source_id);

                spawned_buckets.retain(|tile_id, bucket_eid| {
                    if source.is_none_or(|source| !source.tiles.contains_key(tile_id)) {
                        commands.entity(*bucket_eid).despawn();
                        return false;
                    }

                    true
                });

                source.is_some()
            });
    }
}

fn spawn_kind<C: TileBucketManagerMeta>(
    commands: &mut Commands,
    initialize_params: &mut SystemParamItem<'_, '_, C::InitializeTileParams>,
    bucket: &TileBucket<C>,
    bucket_id: Entity,
    kind: C::TileKind,
    manager_id: Entity,
) {
    let tile_eid = commands
        .spawn((
            Transform::default(),
            TileBucketTile::<C> { kind, manager_id },
        ))
        .id();

    C::initialize_tile(commands.entity(tile_eid), initialize_params, bucket, kind);

    commands.entity(bucket_id).add_child(tile_eid);
}

#[allow(clippy::too_many_arguments)]
fn spawn_bucket<C: TileBucketManagerMeta>(
    commands: &mut Commands,
    initialize_params: &mut SystemParamItem<'_, '_, C::InitializeTileParams>,
    maplibre_int_id: Entity,
    manager_id: Entity,
    grid: &Grid,
    source_id: &str,
    tile_id: CanonicalTileId,
    enabled_kinds: &HashSet<C::TileKind>,
) -> Entity {
    let (center, half_extends) = tile_flat_bounds_world(tile_id);
    let (cell, translation) = grid.translation_to_grid(center);

    let bucket = TileBucket {
        maplibre_int_id,
        source_id: source_id.to_owned(),
        tile_id,
        center,
        half_extents: half_extends,
        _marker: PhantomData::<C>,
    };

    let bucket_id = commands
        .spawn((
            Name::new(format!("Bucket {tile_id:?} ({source_id})")),
            cell,
            Transform::from_translation(translation),
            Visibility::default(),
        ))
        .id();

    commands.entity(manager_id).add_child(bucket_id);

    for kind in enabled_kinds {
        spawn_kind(
            commands,
            initialize_params,
            &bucket,
            bucket_id,
            *kind,
            manager_id,
        );
    }

    commands.entity(bucket_id).insert(bucket);

    bucket_id
}
