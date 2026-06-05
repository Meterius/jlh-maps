use bevy::ecs::bundle::{Bundle, NoBundleEffect};
use bevy::ecs::change_detection::DetectChanges;
use bevy::prelude::*;
use std::marker::PhantomData;

pub trait EntitySpawnerMeta: Send + Sync + 'static {
    type Params: Send + Sync + 'static;
    type Item: Send + Sync + 'static;
    type SpawnBundle: Bundle<Effect: NoBundleEffect>;
    type UpdateBundle: Bundle<Effect: NoBundleEffect>;

    // components inserted on new entities
    fn spawn_bundle(params: &Self::Params, index: usize, item: &Self::Item) -> Self::SpawnBundle;

    // components inserted on existing entities and new entities
    fn update_bundle(params: &Self::Params, index: usize, item: &Self::Item) -> Self::UpdateBundle;
}

// Implements a spawner which spawns entities based on a list of items, re-using existing entities based on index.
pub struct EntitySpawnerPlugin<M: EntitySpawnerMeta>(PhantomData<M>);

impl<M: EntitySpawnerMeta> Default for EntitySpawnerPlugin<M> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<M: EntitySpawnerMeta> Plugin for EntitySpawnerPlugin<M> {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync_entity_spawners::<M>);
    }
}

// ECS

#[derive(Component)]
pub struct EntitySpawner<M: EntitySpawnerMeta> {
    // entity which entities are spawned into
    pub container_id: Entity,
    // global parameter
    pub params: M::Params,
    // item-level parameters for which entities are spawned/despawned
    pub items: Vec<M::Item>,
    _marker: PhantomData<M>,
}

impl<M: EntitySpawnerMeta> EntitySpawner<M> {
    pub fn new(container_id: Entity, params: M::Params, items: Vec<M::Item>) -> Self {
        Self {
            container_id,
            params,
            items,
            _marker: PhantomData,
        }
    }
}

impl<M: EntitySpawnerMeta> EntitySpawner<M>
where
    M::Params: Default,
{
    pub fn default(container_id: Entity, items: Vec<M::Item>) -> Self {
        Self::new(container_id, Default::default(), items)
    }
}

#[derive(Component)]
#[relationship(relationship_target = EntitySpawnerChildren<C>)]
pub struct EntitySpawnerChildOf<C: EntitySpawnerMeta>(#[relationship] Entity, PhantomData<C>);

impl<C: EntitySpawnerMeta> EntitySpawnerChildOf<C> {
    pub fn new(spawner: Entity) -> Self {
        Self(spawner, PhantomData)
    }

    pub fn spawner(&self) -> Entity {
        self.0
    }
}

#[derive(Component)]
#[relationship_target(relationship = EntitySpawnerChildOf<C>)]
pub struct EntitySpawnerChildren<C: EntitySpawnerMeta>(#[relationship] Vec<Entity>, PhantomData<C>);

impl<C: EntitySpawnerMeta> EntitySpawnerChildren<C> {
    pub fn as_slice(&self) -> &[Entity] {
        &self.0
    }
}

fn sync_entity_spawners<M: EntitySpawnerMeta>(
    mut commands: Commands,
    spawners: Query<(
        Entity,
        Ref<EntitySpawner<M>>,
        Option<Ref<EntitySpawnerChildren<M>>>,
    )>,
) {
    for (spawner_entity, spawner, children) in spawners.iter() {
        if !spawner.is_changed() {
            continue;
        }

        let existing_children = children
            .as_ref()
            .map(|children| children.as_slice())
            .unwrap_or(&[]);
        let target_len = spawner.items.len();

        // update existing children

        let update_bundles = (0..target_len.min(existing_children.len()))
            .map(|index| {
                (
                    existing_children[index],
                    (
                        M::update_bundle(&spawner.params, index, &spawner.items[index]),
                        ChildOf(spawner.container_id),
                    ),
                )
            })
            .collect::<Vec<_>>();
        commands.insert_batch(update_bundles);

        // insert children for added items

        let spawn_bundles = (existing_children.len()..target_len)
            .map(|index| {
                (
                    M::spawn_bundle(&spawner.params, index, &spawner.items[index]),
                    M::update_bundle(&spawner.params, index, &spawner.items[index]),
                    ChildOf(spawner.container_id),
                    EntitySpawnerChildOf::<M>::new(spawner_entity),
                )
            })
            .collect::<Vec<_>>();
        commands.spawn_batch(spawn_bundles);

        // despawn children for removed items

        for child in existing_children.iter().skip(target_len).copied() {
            commands.entity(child).despawn();
        }
    }
}
