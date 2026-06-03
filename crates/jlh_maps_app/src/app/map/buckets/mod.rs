pub mod buildings;
pub mod waters;

use crate::app::map::buckets::buildings::GlobalBuildingMaterial;
use crate::app::map::buckets::waters::WaterMaterial;
use crate::app::map::core::MapViewSettings;
use crate::app::map::feature::tile_bucket_manager::{
    TileBucket, TileBucketManager, TileBucketManagerMeta, TileBucketManagerPlugin,
};
use bevy::asset::Assets;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::{App, EntityCommands, Image, Plugin, Res, ResMut};
use std::hash::Hash;

pub struct BucketsPlugin;

impl Plugin for BucketsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            buildings::BuildingsPlugin,
            waters::WatersPlugin,
            TileBucketManagerPlugin::<MapTileBucketMeta>::new(),
        ));
    }
}

// TODO: replace per bucket kind structure
pub(crate) type MapTileBucketManager = TileBucketManager<MapTileBucketMeta>;

pub(crate) type BucketInitializeTileParams = (
    ResMut<'static, Assets<Image>>,
    ResMut<'static, Assets<WaterMaterial>>,
    Res<'static, GlobalBuildingMaterial>,
);

pub(crate) struct MapTileBucketMeta;

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(crate) enum MapTileKind {
    Water,
    Building,
}

const MAP_TILE_KINDS: &[MapTileKind] = &[MapTileKind::Water, MapTileKind::Building];

impl TileBucketManagerMeta for MapTileBucketMeta {
    type TileKind = MapTileKind;

    fn tile_kinds() -> &'static [Self::TileKind] {
        MAP_TILE_KINDS
    }

    type TileKindEnabledParams = Res<'static, MapViewSettings>;

    fn is_tile_kind_enabled(
        settings: &SystemParamItem<'_, '_, Self::TileKindEnabledParams>,
        kind: Self::TileKind,
    ) -> bool {
        match kind {
            MapTileKind::Water => settings.enable_waters,
            MapTileKind::Building => settings.enable_buildings,
        }
    }

    type InitializeTileParams = BucketInitializeTileParams;

    fn initialize_tile(
        mut e_commands: EntityCommands,
        params: &mut SystemParamItem<'_, '_, Self::InitializeTileParams>,
        bucket: &TileBucket<Self>,
        kind: Self::TileKind,
    ) {
        match kind {
            MapTileKind::Water => waters::initialize_water_tile(&mut e_commands, params, bucket),
            MapTileKind::Building => {
                buildings::initialize_building_tile(&mut e_commands, params, bucket)
            }
        }
    }
}
