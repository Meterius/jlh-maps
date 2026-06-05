use crate::app::maplibre_gl_js::mvt::parse_tile;
use crate::app::task_pool::AppTaskPool;
use crate::wasm_task_pool::Task;
use bevy::prelude::{Component, Reflect, default};
use geojson::Geometry;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::app::maplibre_gl_js::utils::terrain::TerrainData;

#[derive(Component, Default)]
#[allow(dead_code)]
pub struct MlView {
    pub width: f64,
    pub height: f64,
    pub zoom: f64,
    pub pitch: f64,
    pub bearing: f64,
    pub center_lng: f64,
    pub center_lat: f64,
    pub main_matrix: Vec<f64>,
}

#[derive(Component, Default)]
pub struct MlTerrain {
    pub tiles: HashMap<CanonicalTileId, Arc<MlTerrainTile>>,
    pub active_tile_ids: HashSet<CanonicalTileId>,
}

#[derive(Debug)]
pub struct MlTerrainTile {
    pub id: CanonicalTileId,
    pub hash: u64,
    pub terrain_data: TerrainData,
}

#[derive(Component, Default)]
pub struct MlData {
    pub sources: HashMap<String, MlSource>,
    next_revision: u64,
    pending_tile_parse_tasks: HashMap<MlTileKey, MlTileParseTask>,
}

#[derive(Default)]
pub struct MlSource {
    pub id: String,
    pub tiles: HashMap<CanonicalTileId, Arc<MlTile>>,
    pub renderable_tile_ids: HashSet<CanonicalTileId>,
}

#[derive(Default)]
pub struct MlTile {
    pub id: CanonicalTileId,
    pub revision: u64,
    pub layers: HashMap<String, MlTileLayer>,
}

#[derive(Default)]
pub struct MlTileLayer {
    pub id: String,
    pub features: HashMap<u64, MlTileFeature>,
}

#[derive(Clone, Debug)]
pub struct MlTileFeature {
    pub id: u64,
    pub geometry: Geometry,
    pub properties: HashMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct MlTileKey {
    source_id: String,
    tile_id: CanonicalTileId,
}

impl MlTileKey {
    fn new(source_id: impl Into<String>, tile_id: CanonicalTileId) -> Self {
        Self {
            source_id: source_id.into(),
            tile_id,
        }
    }
}

struct MlTileParseTask {
    revision: u64,
    task: Task<Result<MlTile, String>>,
}

impl MlData {
    pub fn update_tile(
        &mut self,
        source_id: String,
        tile_id: CanonicalTileId,
        data: Vec<u8>,
        task_pool: &AppTaskPool,
    ) -> u64 {
        self.next_revision = self.next_revision.saturating_add(1).max(1);
        let revision = self.next_revision;

        let task = task_pool.spawn(move || parse_tile(tile_id, data, revision));
        self.pending_tile_parse_tasks.insert(
            MlTileKey::new(source_id, tile_id),
            MlTileParseTask { revision, task },
        );

        revision
    }

    pub fn apply_pending_tile_parse_results(&mut self) {
        let completed_tasks = self
            .pending_tile_parse_tasks
            .iter_mut()
            .filter_map(|(tile_key, pending_task)| {
                if !pending_task.task.is_finished() {
                    return None;
                }

                pending_task
                    .task
                    .take_result()
                    .map(|tile| (tile_key.clone(), pending_task.revision, tile))
            })
            .collect::<Vec<_>>();

        for (tile_key, revision, tile) in completed_tasks {
            let is_current = self
                .pending_tile_parse_tasks
                .get(&tile_key)
                .is_some_and(|pending_task| pending_task.revision == revision);
            if !is_current {
                continue;
            }

            self.pending_tile_parse_tasks.remove(&tile_key);

            match tile {
                Ok(tile) => {
                    self.apply_tile(tile_key.source_id, tile);
                }
                Err(err) => {
                    let source_id = &tile_key.source_id;
                    let tile_id = &tile_key.tile_id;

                    if let Some(source) = self.sources.get_mut(source_id) {
                        source.tiles.remove(tile_id);
                    }

                    tracing::warn!(
                        "Failed to parse MapLibre source tile {}/{:?}: {}",
                        tile_key.source_id,
                        tile_key.tile_id,
                        err
                    );
                    continue;
                }
            };
        }
    }

    fn apply_tile(&mut self, source_id: String, tile: MlTile) {
        if tile.layers.is_empty() {
            return;
        }

        let source = self
            .sources
            .entry(source_id.clone())
            .or_insert_with(|| MlSource {
                id: source_id,
                ..default()
            });

        source.tiles.insert(tile.id, Arc::new(tile));
    }

    pub fn remove_tile(&mut self, source_id: &str, tile_id: &CanonicalTileId) {
        self.pending_tile_parse_tasks
            .remove(&MlTileKey::new(source_id, *tile_id));

        if let Some(source) = self.sources.get_mut(source_id) {
            source.tiles.remove(tile_id);
        }
    }

    pub fn set_renderable_tiles(&mut self, source_id: String, tile_ids: HashSet<CanonicalTileId>) {
        let source = self
            .sources
            .entry(source_id.clone())
            .or_insert_with(|| MlSource {
                id: source_id,
                ..default()
            });

        source.renderable_tile_ids = tile_ids;
    }
}

#[derive(Default, Reflect, Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq)]
pub struct CanonicalTileId {
    pub z: u32,
    pub x: u32,
    pub y: u32,
}
