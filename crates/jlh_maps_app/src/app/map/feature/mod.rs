pub mod bucket_layer;
pub mod bucket_manager;
pub mod edge_distance_texture;
pub mod mesh;
pub mod scatter;
pub mod tile;
pub mod tile_task_based;
pub mod utils;

use crate::app::map::feature::edge_distance_texture::FeatureEdgeDistanceTexturePlugin;
use crate::app::map::feature::mesh::FeatureMeshPlugin;
use crate::app::map::feature::scatter::FeatureScatterPlugin;
use bevy::prelude::Plugin;

pub struct MapFeaturePlugin;

impl Plugin for MapFeaturePlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_plugins((
            FeatureMeshPlugin,
            FeatureEdgeDistanceTexturePlugin,
            FeatureScatterPlugin,
        ));
    }
}
