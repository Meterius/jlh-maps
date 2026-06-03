use crate::app::maplibre_gl_js::types::CanonicalTileId;
use crate::app::maplibre_gl_js::utils::mercator_coordinate::{LngLat, MercatorCoordinate};
use crate::app::maplibre_gl_js::utils::tile::get_tile_lnglat_bounds;
use bevy::math::{DVec2, DVec3, Vec2, Vec3Swizzles, dvec2, dvec3};

pub const MERCATOR_WORLD_SIZE: f64 = 100_000.0;

pub fn world_from_lng_lat_alt(lng: f64, lat: f64, alt: f64) -> DVec3 {
    world_from_mercator_coordinate(MercatorCoordinate::from_lng_lat(LngLat::new(lng, lat), alt))
}

fn world_from_mercator_coordinate(coords: MercatorCoordinate) -> DVec3 {
    let MercatorCoordinate { x, y, z } = coords;
    dvec3(x, -y, z) * MERCATOR_WORLD_SIZE
}

pub fn tile_flat_world_center(tile_id: CanonicalTileId) -> DVec3 {
    tile_flat_world_bounds(tile_id).0
}

pub fn tile_flat_world_bounds(tile_id: CanonicalTileId) -> (DVec3, Vec2) {
    let bounds = get_tile_lnglat_bounds(tile_id);
    let south_west = world_from_lng_lat_alt(bounds.0.x, bounds.0.y, 0.0);
    let north_east = world_from_lng_lat_alt(bounds.1.x, bounds.1.y, 0.0);
    let min = south_west.min(north_east);
    let max = south_west.max(north_east);
    let size = max - min;

    ((min + max) * 0.5, size.xy().as_vec2() * 0.5)
}

pub fn lng_lat_from_world_xy(world_xy: DVec2) -> DVec2 {
    let mercator_x = world_xy.x / MERCATOR_WORLD_SIZE;
    let mercator_y = -world_xy.y / MERCATOR_WORLD_SIZE;
    let lng = mercator_x * 360.0 - 180.0;
    let lat = (std::f64::consts::PI * (1.0 - 2.0 * mercator_y))
        .sinh()
        .atan()
        .to_degrees();

    dvec2(lng, lat)
}

pub fn tile_world_units_per_meter(tile_id: CanonicalTileId) -> f64 {
    let bounds = get_tile_lnglat_bounds(tile_id);
    let center = (bounds.0 + bounds.1) * 0.5;

    MercatorCoordinate::from_lng_lat(LngLat::new(center.x, center.y), 0.0)
        .meter_in_mercator_coordinate_units()
        * MERCATOR_WORLD_SIZE
}
