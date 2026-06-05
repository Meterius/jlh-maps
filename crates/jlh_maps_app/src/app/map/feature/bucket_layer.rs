use crate::app::map::feature::bucket_manager::{TileBucket, TileBucketSystems};
use bevy::app::{App, Plugin};
use bevy::ecs::system::{StaticSystemParam, SystemParam, SystemParamItem};
use bevy::prelude::{
    Commands, Component, Entity, EntityCommands, IntoScheduleConfigs, Query, Transform, Update,
    Visibility, With, Without,
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
        bucket_eid: Entity,
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
    child_eid: Entity,
    _marker: PhantomData<L>,
}

impl<L: TileBucketLayerMeta> TileBucketLayerSpawned<L> {
    fn new(child_eid: Entity) -> Self {
        Self {
            child_eid,
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
    spawnable_buckets: Query<
        (Entity, &TileBucket),
        (With<L::BucketMarker>, Without<TileBucketLayerSpawned<L>>),
    >,
    spawned_buckets: Query<(Entity, &TileBucketLayerSpawned<L>), With<L::BucketMarker>>,
) {
    if !L::is_enabled(&*enabled_params) {
        for (bucket_eid, spawned) in spawned_buckets.iter() {
            commands.entity(spawned.child_eid).despawn();
            commands
                .entity(bucket_eid)
                .remove::<TileBucketLayerSpawned<L>>();
        }

        return;
    }

    for (bucket_eid, bucket) in spawnable_buckets.iter() {
        spawn_tile::<L>(&mut commands, &mut *spawn_params, bucket, bucket_eid);
    }
}

fn spawn_tile<L: TileBucketLayerMeta>(
    commands: &mut Commands,
    spawn_params: &mut SystemParamItem<'_, '_, L::SpawnParams>,
    bucket: &TileBucket,
    bucket_eid: Entity,
) {
    let child_eid = commands
        .spawn((
            Transform::default(),
            Visibility::Inherited,
            TileBucketLayerTile::<L>::new(),
        ))
        .id();

    L::spawn(commands.entity(child_eid), spawn_params, bucket_eid, bucket);

    commands.entity(bucket_eid).add_child(child_eid);
    commands
        .entity(bucket_eid)
        .insert(TileBucketLayerSpawned::<L>::new(child_eid));
}
