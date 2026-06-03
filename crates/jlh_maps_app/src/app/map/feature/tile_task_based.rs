use crate::app::map::feature::tile::FeatureTile;
use crate::app::maplibre_gl_js::integration::MaplibreMapIntegration;
use crate::app::maplibre_gl_js::types::{MlTerrainTile, MlTile};
use crate::app::task_pool::AppTaskPool;
use crate::wasm_task_pool::Task;
use bevy::app::{App, Plugin};
use bevy::ecs::system::{StaticSystemParam, SystemParam, SystemParamItem};
use bevy::prelude::{Component, Entity, Query, Res, Update, Visibility};
use std::sync::Arc;

pub struct TileTaskBasedPlugin<C: TileTaskBasedMeta>(std::marker::PhantomData<C>);

impl<C: TileTaskBasedMeta> Default for TileTaskBasedPlugin<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: TileTaskBasedMeta> TileTaskBasedPlugin<C> {
    pub fn new() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<C: TileTaskBasedMeta> Plugin for TileTaskBasedPlugin<C> {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync_items::<C>);
    }
}

pub trait TileTaskBasedMeta: Send + Sync + 'static {
    type Data: Send + Sync + 'static;
    type State: Send + Sync + 'static;
    type Config: Clone + Send + Sync + 'static;

    type ApplyParams: SystemParam + 'static;

    fn use_terrain() -> bool;

    fn build_data(
        tile: Arc<MlTile>,
        terrain_tile: Option<MlTerrainTile>,
        config: Self::Config,
    ) -> Self::Data;

    fn apply_data(
        entity: Entity,
        params: &mut SystemParamItem<'_, '_, Self::ApplyParams>,
        config: &Self::Config,
        state: &mut Self::State,
        data: Option<Self::Data>,
    ) -> Option<Self::Data>;
}

#[derive(Default, Clone)]
pub struct TileTaskBasedRevision {
    terrain_hash: Option<String>,
    tile_revision: Option<u64>,
}

impl TileTaskBasedRevision {
    fn reset(&mut self) {
        self.terrain_hash = None;
        self.tile_revision = None;
    }

    fn is_empty(&self) -> bool {
        self.terrain_hash.is_none() && self.tile_revision.is_none()
    }
}

#[derive(Component)]
pub struct TileTaskBased<C: TileTaskBasedMeta> {
    config: C::Config,
    data: Option<C::Data>,
    state: C::State,
    dirty: bool,

    data_revision: u64,
    revision: TileTaskBasedRevision,

    pending_task: Option<Task<C::Data>>,
}

impl<C: TileTaskBasedMeta> TileTaskBased<C> {
    pub fn from_parts(config: C::Config, state: C::State) -> Self {
        Self {
            config,
            data: None,
            state,
            dirty: true,
            revision: TileTaskBasedRevision::default(),
            data_revision: 0,
            pending_task: None,
        }
    }

    pub fn config(&self) -> &C::Config {
        &self.config
    }

    pub fn state(&self) -> &C::State {
        &self.state
    }

    pub fn data(&self) -> Option<&C::Data> {
        self.data.as_ref()
    }

    pub fn data_revision(&self) -> u64 {
        self.data_revision
    }

    fn clear_data(&mut self) {
        self.data = None;
        self.dirty = true;
        self.pending_task = None;
    }
}

impl<C: TileTaskBasedMeta> Default for TileTaskBased<C>
where
    C::Config: Default,
    C::State: Default,
{
    fn default() -> Self {
        Self::from_parts(C::Config::default(), C::State::default())
    }
}

fn sync_items<C: TileTaskBasedMeta>(
    map_ints: Query<&MaplibreMapIntegration>,
    task_pool: Res<AppTaskPool>,
    mut params: StaticSystemParam<C::ApplyParams>,
    mut buckets: Query<(
        Entity,
        &FeatureTile,
        &mut TileTaskBased<C>,
        Option<&Visibility>,
    )>,
) {
    for (id, tile, mut tile_tb, visibility) in buckets.iter_mut() {
        let Some(map_int) = map_ints.get(tile.maplibre_int_id).ok() else {
            continue;
        };

        if matches!(visibility, Some(Visibility::Hidden)) {
            continue;
        }

        sync_item(map_int, &mut *params, id, tile, &mut tile_tb, &task_pool);
    }
}

fn sync_item<C: TileTaskBasedMeta>(
    map_int: &MaplibreMapIntegration,
    params: &mut SystemParamItem<'_, '_, C::ApplyParams>,
    id: Entity,
    tile: &FeatureTile,
    tile_tb: &mut TileTaskBased<C>,
    task_pool: &AppTaskPool,
) {
    // apply task returns
    if tile_tb
        .pending_task
        .as_ref()
        .is_some_and(|pending_task| pending_task.is_finished())
        && let Some(data) = tile_tb
            .pending_task
            .take()
            .and_then(|mut pending_task| pending_task.take_result())
        {
            tile_tb.data =
                C::apply_data(id, params, &tile_tb.config, &mut tile_tb.state, Some(data));
            tile_tb.data_revision += 1;
        }

    // clear if tile no longer exists
    let Some(tile) = tile.tile(map_int) else {
        if !tile_tb.revision.is_empty() {
            tile_tb.clear_data();
            tile_tb.data = C::apply_data(id, params, &tile_tb.config, &mut tile_tb.state, None);
            tile_tb.data_revision += 1;
            tile_tb.dirty = false;
            tile_tb.revision.reset();
        }
        return;
    };

    // check if tile or terrain data has changed

    if tile_tb.revision.tile_revision != Some(tile.revision) {
        tile_tb.clear_data();
        tile_tb.revision.tile_revision = Some(tile.revision);
    }

    let terrain_data = if C::use_terrain() {
        map_int.terrain.tiles.get(&tile.id)
    } else {
        None
    };
    let terrain_hash = terrain_data.map(|terrain_data| terrain_data.hash.clone());

    if tile_tb.revision.terrain_hash != terrain_hash {
        tile_tb.clear_data();
        tile_tb.revision.terrain_hash = terrain_hash.clone();
    }

    if !tile_tb.dirty {
        return;
    }

    // data has changed, start rebuild task

    let tile = Arc::clone(tile);
    let terrain_data = terrain_data.cloned();

    tile_tb.pending_task = Some({
        let config = tile_tb.config.clone();
        task_pool.spawn(move || C::build_data(tile, terrain_data, config))
    });
    tile_tb.dirty = false;
}
