pub mod integration;
pub mod interop;
pub mod mvt;
pub mod types;
pub mod utils;

use crate::app::maplibre_gl_js::integration::MaplibreMapIntegration;
use crate::app::maplibre_gl_js::types::MlData;
use bevy::prelude::*;

pub struct MaplibreGlJsPlugin;

impl Plugin for MaplibreGlJsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, apply_pending_tile_parse_results);
    }
}

fn apply_pending_tile_parse_results(
    mut integrations: Query<&mut MlData, With<MaplibreMapIntegration>>,
) {
    for mut data in integrations.iter_mut() {
        data.apply_pending_tile_parse_results();
    }
}
