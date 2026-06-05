use bevy::prelude::*;
use std::cell::Cell;

thread_local! {
    pub(super) static NEXT_INTEGRATION_ID: Cell<u32> = const { Cell::new(1) };
}

#[derive(Component, Default)]
pub struct MaplibreMapIntegration {
    pub id: u32,
}

pub(super) fn with_map_entity(
    world: &mut World,
    integration_id: u32,
    callback: impl FnOnce(&mut World, Entity),
) {
    let Some(entity) = find_map_integration(world, integration_id) else {
        return;
    };

    callback(world, entity);
}

pub(super) fn find_map_integration(world: &mut World, integration_id: u32) -> Option<Entity> {
    let mut query = world.query::<(Entity, &MaplibreMapIntegration)>();
    query
        .iter(world)
        .find(|(_, integration)| integration.id == integration_id)
        .map(|(entity, _)| entity)
}
