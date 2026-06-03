use crate::app::map::feature::bucket_manager::{TileBucket, TileBucketSystems};
use bevy::app::{App, Plugin};
use bevy::ecs::system::{StaticSystemParam, SystemParam, SystemParamItem};
use bevy::prelude::{
    Commands, Component, Entity, EntityCommands, IntoScheduleConfigs, Query, Transform, Update,
    With, Without,
};
use std::marker::PhantomData;

pub trait TileBucketLayerMeta: Send + Sync + Sized + 'static {
    type BucketMarker: Component;
    type EnabledParams: SystemParam + 'static;
    type SpawnParams: SystemParam + 'static;

    fn is_enabled(params: &SystemParamItem<'_, '_, Self::EnabledParams>) -> bool;

    fn spawn(
        e_commands: EntityCommands,
        params: &mut SystemParamItem<'_, '_, Self::SpawnParams>,
        bucket_entity: Entity,
        bucket: &TileBucket,
    );
}

pub struct TileBucketLayerPlugin<L: TileBucketLayerMeta>(PhantomData<L>);

impl<L: TileBucketLayerMeta> Default for TileBucketLayerPlugin<L> {
    fn default() -> Self {
        Self::new()
    }
}

impl<L: TileBucketLayerMeta> TileBucketLayerPlugin<L> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<L: TileBucketLayerMeta> Plugin for TileBucketLayerPlugin<L> {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            sync_tile_bucket_layer::<L>
                .in_set(TileBucketSystems::SyncLayers)
                .after(TileBucketSystems::SyncBuckets),
        );
    }
}

#[derive(Component)]
pub struct TileBucketLayerSpawned<L: TileBucketLayerMeta> {
    child_id: Entity,
    _marker: PhantomData<L>,
}

impl<L: TileBucketLayerMeta> TileBucketLayerSpawned<L> {
    fn new(child_id: Entity) -> Self {
        Self {
            child_id,
            _marker: PhantomData,
        }
    }
}

#[derive(Component)]
pub struct TileBucketLayerTile<L: TileBucketLayerMeta> {
    _marker: PhantomData<L>,
}

impl<L: TileBucketLayerMeta> TileBucketLayerTile<L> {
    fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

#[allow(clippy::type_complexity)]
fn sync_tile_bucket_layer<L: TileBucketLayerMeta>(
    mut commands: Commands,
    enabled_params: StaticSystemParam<L::EnabledParams>,
    mut spawn_params: StaticSystemParam<L::SpawnParams>,
    buckets_to_spawn: Query<
        (Entity, &TileBucket),
        (With<L::BucketMarker>, Without<TileBucketLayerSpawned<L>>),
    >,
    spawned_buckets: Query<(Entity, &TileBucketLayerSpawned<L>), With<L::BucketMarker>>,
) {
    if !L::is_enabled(&*enabled_params) {
        for (bucket_id, spawned) in spawned_buckets.iter() {
            commands.entity(spawned.child_id).despawn();
            commands
                .entity(bucket_id)
                .remove::<TileBucketLayerSpawned<L>>();
        }

        return;
    }

    for (bucket_id, bucket) in buckets_to_spawn.iter() {
        spawn_tile::<L>(&mut commands, &mut *spawn_params, bucket, bucket_id);
    }
}

fn spawn_tile<L: TileBucketLayerMeta>(
    commands: &mut Commands,
    spawn_params: &mut SystemParamItem<'_, '_, L::SpawnParams>,
    bucket: &TileBucket,
    bucket_id: Entity,
) {
    let child_id = commands
        .spawn((Transform::default(), TileBucketLayerTile::<L>::new()))
        .id();

    L::spawn(commands.entity(child_id), spawn_params, bucket_id, bucket);

    commands.entity(bucket_id).add_child(child_id);
    commands
        .entity(bucket_id)
        .insert(TileBucketLayerSpawned::<L>::new(child_id));
}
