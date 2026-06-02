use crate::app::maplibre_gl_js::mvt::parse_tile_layers;
use bevy::prelude::Reflect;
use geojson::Geometry;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::app::maplibre_gl_js::utils::terrain::TerrainData;

#[derive(Default)]
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

#[derive(Default)]
pub struct MlTerrain {
    pub active_tile_ids: HashSet<CanonicalTileId>,
    pub tiles: HashMap<CanonicalTileId, MlTerrainTile>,
}

#[derive(Clone, Debug)]
pub struct MlTerrainTile {
    pub id: CanonicalTileId,
    pub hash: String,
    pub terrain_data: TerrainData,
}

#[derive(Default)]
pub struct MlData {
    pub sources: HashMap<String, MlSource>,
    next_revision: u64,
}

#[derive(Default)]
pub struct MlSource {
    pub id: String,
    pub layers: HashMap<String, MlLayer>,
}

#[derive(Default)]
pub struct MlLayer {
    pub id: String,
    pub tiles: HashMap<CanonicalTileId, Arc<MlTile>>,
}

#[derive(Default)]
pub struct MlTile {
    pub id: CanonicalTileId,
    pub revision: u64,
    pub features: HashMap<u64, MlTileFeature>,
}

#[derive(Clone, Debug)]
pub struct MlTileFeature {
    pub id: u64,
    pub geometry: Geometry,
    pub properties: HashMap<String, serde_json::Value>,
}

impl MlData {
    pub fn update_tile(
        &mut self,
        source_id: String,
        tile_id: CanonicalTileId,
        data: Vec<u8>,
    ) -> Result<u64, String> {
        self.next_revision = self.next_revision.saturating_add(1).max(1);
        let revision = self.next_revision;
        let layers = match parse_tile_layers(tile_id, data, revision) {
            Ok(layers) => layers,
            Err(err) => {
                self.remove_tile(&source_id, &tile_id);
                return Err(err);
            }
        };

        self.remove_tile(&source_id, &tile_id);

        if layers.is_empty() {
            return Ok(revision);
        }

        let source = self
            .sources
            .entry(source_id.clone())
            .or_insert_with(|| MlSource::new(source_id));

        for (layer_id, tile) in layers {
            source
                .layers
                .entry(layer_id.clone())
                .or_insert_with(|| MlLayer::new(layer_id))
                .tiles
                .insert(tile_id, Arc::new(tile));
        }

        Ok(revision)
    }

    pub fn remove_tile(&mut self, source_id: &str, tile_id: &CanonicalTileId) {
        let Some(source) = self.sources.get_mut(source_id) else {
            return;
        };

        source.layers.retain(|_, layer| {
            layer.tiles.remove(tile_id);
            !layer.tiles.is_empty()
        });
        if source.layers.is_empty() {
            self.sources.remove(source_id);
        }
    }
}

impl MlSource {
    fn new(id: String) -> Self {
        Self {
            id,
            layers: HashMap::default(),
        }
    }
}

impl MlLayer {
    fn new(id: String) -> Self {
        Self {
            id,
            tiles: HashMap::default(),
        }
    }
}

#[derive(Default, Reflect, Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq)]
pub struct CanonicalTileId {
    pub z: u32,
    pub x: u32,
    pub y: u32,
}
