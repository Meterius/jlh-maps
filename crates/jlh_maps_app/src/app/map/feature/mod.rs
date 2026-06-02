pub mod bucket;
pub mod edge_distance_texture;
pub mod mesh;
pub mod utils;

use crate::app::map::feature::edge_distance_texture::FeatureEdgeDistanceTexturePlugin;
use crate::app::map::feature::mesh::FeatureMeshPlugin;
use bevy::prelude::Plugin;

pub struct MapFeaturePlugin;

impl Plugin for MapFeaturePlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_plugins((FeatureMeshPlugin, FeatureEdgeDistanceTexturePlugin));
    }
}
