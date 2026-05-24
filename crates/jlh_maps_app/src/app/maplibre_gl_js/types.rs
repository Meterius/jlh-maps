use bevy::prelude::Reflect;
use geojson::Geometry;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

use crate::app::maplibre_gl_js::utils::terrain::TerrainData;

#[derive(Default)]
#[allow(dead_code)]
pub struct MaplibreMapViewData {
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
pub struct MaplibreTerrainData {
    pub active_tile_ids: HashSet<CanonicalTileId>,
    pub tiles: HashMap<CanonicalTileId, MaplibreTerrainTileData>,
}

#[derive(Clone, Debug)]
pub struct MaplibreTerrainTileData {
    pub hash: String,
    pub terrain_data: TerrainData,
}

#[derive(Default)]
pub struct MaplibreSourceData {
    pub sources: HashMap<String, MaplibreTileSourceData>,
    next_revision: u64,
}

impl MaplibreSourceData {
    pub fn update_tile(
        &mut self,
        source_id: String,
        tile_id: CanonicalTileId,
        data: Vec<u8>,
    ) -> u64 {
        self.next_revision = self.next_revision.saturating_add(1).max(1);
        let revision = self.next_revision;

        self.sources
            .entry(source_id)
            .or_default()
            .tiles
            .insert(tile_id, MaplibreTileData { revision, data });

        revision
    }

    pub fn remove_tile(&mut self, source_id: &str, tile_id: &CanonicalTileId) {
        let Some(source) = self.sources.get_mut(source_id) else {
            return;
        };

        source.tiles.remove(tile_id);
        if source.tiles.is_empty() {
            self.sources.remove(source_id);
        }
    }
}

#[derive(Default)]
pub struct MaplibreTileSourceData {
    pub tiles: HashMap<CanonicalTileId, MaplibreTileData>,
}

#[derive(Clone, Debug)]
pub struct MaplibreTileData {
    pub revision: u64,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct SourceLayerFeature {
    pub tile_id: CanonicalTileId,
    pub id: u64,
    pub geometry: Geometry,
    pub properties: HashMap<String, serde_json::Value>,
}

#[derive(Reflect, Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq)]
pub struct CanonicalTileId {
    pub z: u32,
    pub x: u32,
    pub y: u32,
}
