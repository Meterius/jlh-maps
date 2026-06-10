pub mod camera;
pub mod core;
pub mod feature;
pub mod feature_layers;
pub mod interop;
pub mod lighting;
pub mod terrain;
pub mod transform;
pub mod skybox;
pub mod utils;

use bevy::prelude::*;

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            camera::CameraPlugin,
            core::CorePlugin,
            terrain::TerrainPlugin,
            feature::MapFeaturePlugin,
            feature_layers::FeatureLayersPlugin,
            lighting::LightingPlugin,
            skybox::SkyboxPlugin,
        ));
    }
}
