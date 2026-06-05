use crate::app::maplibre_gl_js::types::{CanonicalTileId, MlData, MlTile};
use bevy::math::DVec3;
use bevy::prelude::{Component, Entity};
use std::sync::Arc;

#[derive(Component)]
pub struct FeatureTile {
    pub maplibre_int_id: Entity,
    pub source_id: String,
    pub tile_id: CanonicalTileId,
    pub center: DVec3,
}

impl FeatureTile {
    pub fn new(
        maplibre_int_id: Entity,
        source_id: &str,
        tile_id: CanonicalTileId,
        center: DVec3,
    ) -> Self {
        Self {
            maplibre_int_id,
            source_id: source_id.to_owned(),
            tile_id,
            center,
        }
    }

    pub fn tile<'a>(&self, ml_data: &'a MlData) -> Option<&'a Arc<MlTile>> {
        ml_data
            .sources
            .get(&self.source_id)
            .and_then(|source| source.tiles.get(&self.tile_id))
    }
}
