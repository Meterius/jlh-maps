pub mod integration;
pub mod interop;
pub mod mvt;
pub mod types;
pub mod utils;

use crate::app::maplibre_gl_js::integration::MaplibreMapIntegration;
use bevy::prelude::*;

pub struct MaplibreGlJsPlugin;

impl Plugin for MaplibreGlJsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, apply_pending_tile_parse_results);
    }
}

fn apply_pending_tile_parse_results(
    mut integrations: Query<&mut MaplibreMapIntegration>,
) {
    for mut integration in integrations.iter_mut() {
        integration.data.apply_pending_tile_parse_results();
    }
}
