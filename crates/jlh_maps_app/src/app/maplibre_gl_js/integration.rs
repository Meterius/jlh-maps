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
    let Some(integration_eid) = find_map_integration(world, integration_id) else {
        return;
    };

    callback(world, integration_eid);
}

pub(super) fn find_map_integration(world: &mut World, integration_id: u32) -> Option<Entity> {
    let mut integrations = world.query::<(Entity, &MaplibreMapIntegration)>();
    integrations
        .iter(world)
        .find(|(_, integration)| integration.id == integration_id)
        .map(|(integration_eid, _)| integration_eid)
}
