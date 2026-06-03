pub mod buildings;
pub mod trees;
pub mod waters;

use crate::app::map::feature::bucket_layer::TileBucketLayerPlugin;
use crate::app::map::feature::bucket_manager::{TileBucketManager, TileBucketManagerPlugin};
use bevy::prelude::{App, Entity, Plugin};

pub struct FeatureLayersPlugin;

impl Plugin for FeatureLayersPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            TileBucketManagerPlugin,
            TileBucketLayerPlugin::<waters::WaterTileBucketLayer>::new(),
            TileBucketLayerPlugin::<buildings::BuildingTileBucketLayer>::new(),
            TileBucketLayerPlugin::<trees::TreeTileBucketLayer>::new(),
            buildings::BuildingsPlugin,
            trees::TreesPlugin,
            waters::WatersPlugin,
        ));
    }
}

pub fn make_bucket_manager(maplibre_int_id: Entity) -> TileBucketManager {
    TileBucketManager::new(maplibre_int_id, |mut e_commands, _| {
        e_commands.insert((
            waters::WaterTileBucket,
            buildings::BuildingTileBucket,
            trees::TreeTileBucket,
        ));
    })
}
