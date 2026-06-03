use crate::app::maplibre_gl_js::types::{CanonicalTileId, MlTile, MlTileFeature, MlTileLayer};
use geo_types::{
    Coord, Geometry as GeoGeometry, LineString, MultiLineString, MultiPoint, MultiPolygon, Point,
    Polygon,
};
use geojson::{Geometry, Value};
use mvt_reader::Reader;
use mvt_reader::feature::Value as MvtValue;
use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;

const GENERATED_FEATURE_ID_BIT: u64 = 1 << 63;

pub fn parse_tile(
    tile_id: CanonicalTileId,
    tile_data: Vec<u8>,
    revision: u64,
) -> Result<MlTile, String> {
    let reader =
        Reader::new(tile_data).map_err(|err| format!("Failed to decode MVT tile: {err:?}"))?;
    let layer_metadata = reader
        .get_layer_metadata()
        .map_err(|err| format!("Failed to read MVT layer metadata: {err:?}"))?;

    let mut layers = HashMap::with_capacity(layer_metadata.len());

    for layer in layer_metadata {
        let extent = f64::from(layer.extent.max(1));
        let raw_features = reader
            .get_features_as::<f64>(layer.layer_index)
            .map_err(|err| format!("Failed to read MVT layer features: {err:?}"))?;
        let mut used_feature_ids = HashSet::with_capacity(raw_features.len());
        let mut features = HashMap::with_capacity(raw_features.len());

        for (feature_index, raw_feature) in raw_features.into_iter().enumerate() {
            let id = feature_id(raw_feature.id, feature_index, &mut used_feature_ids);
            features.insert(
                id,
                MlTileFeature {
                    id,
                    geometry: Geometry::new(mvt_geometry_to_geojson_value(
                        tile_id,
                        extent,
                        raw_feature.geometry,
                    )),
                    properties: raw_feature
                        .properties
                        .unwrap_or_default()
                        .into_iter()
                        .map(|(key, value)| (key, mvt_value_to_json(value)))
                        .collect(),
                },
            );
        }

        layers.insert(
            layer.name.clone(),
            MlTileLayer {
                id: layer.name,
                features,
            },
        );
    }

    Ok(MlTile {
        id: tile_id,
        revision,
        layers,
    })
}

fn feature_id(
    raw_feature_id: Option<u64>,
    feature_index: usize,
    used_feature_ids: &mut HashSet<u64>,
) -> u64 {
    if let Some(id) = raw_feature_id
        && used_feature_ids.insert(id)
    {
        return id;
    }

    let mut generated_id = feature_index as u64 & !GENERATED_FEATURE_ID_BIT;
    loop {
        let id = GENERATED_FEATURE_ID_BIT | generated_id;
        if used_feature_ids.insert(id) {
            return id;
        }
        generated_id = generated_id.saturating_add(1) & !GENERATED_FEATURE_ID_BIT;
    }
}

fn mvt_value_to_json(value: MvtValue) -> serde_json::Value {
    match value {
        MvtValue::String(value) => serde_json::Value::String(value),
        MvtValue::Float(value) => serde_json::Value::from(value),
        MvtValue::Double(value) => serde_json::Value::from(value),
        MvtValue::Int(value) => serde_json::Value::from(value),
        MvtValue::UInt(value) => serde_json::Value::from(value),
        MvtValue::SInt(value) => serde_json::Value::from(value),
        MvtValue::Bool(value) => serde_json::Value::from(value),
        MvtValue::Null => serde_json::Value::Null,
    }
}

fn mvt_geometry_to_geojson_value(
    tile_id: CanonicalTileId,
    extent: f64,
    geometry: GeoGeometry<f64>,
) -> Value {
    match geometry {
        GeoGeometry::Point(point) => Value::Point(point_position(tile_id, extent, point)),
        GeoGeometry::Line(line) => Value::LineString(vec![
            coord_position(tile_id, extent, line.start),
            coord_position(tile_id, extent, line.end),
        ]),
        GeoGeometry::LineString(line_string) => {
            Value::LineString(line_string_positions(tile_id, extent, &line_string))
        }
        GeoGeometry::Polygon(polygon) => {
            Value::Polygon(polygon_positions(tile_id, extent, &polygon))
        }
        GeoGeometry::MultiPoint(multi_point) => {
            Value::MultiPoint(multi_point_positions(tile_id, extent, &multi_point))
        }
        GeoGeometry::MultiLineString(multi_line_string) => Value::MultiLineString(
            multi_line_string_positions(tile_id, extent, &multi_line_string),
        ),
        GeoGeometry::MultiPolygon(multi_polygon) => {
            Value::MultiPolygon(multi_polygon_positions(tile_id, extent, &multi_polygon))
        }
        GeoGeometry::GeometryCollection(geometry_collection) => Value::GeometryCollection(
            geometry_collection
                .0
                .into_iter()
                .map(|geometry| {
                    Geometry::new(mvt_geometry_to_geojson_value(tile_id, extent, geometry))
                })
                .collect(),
        ),
        GeoGeometry::Rect(rect) => {
            Value::Polygon(polygon_positions(tile_id, extent, &rect.to_polygon()))
        }
        GeoGeometry::Triangle(triangle) => Value::Polygon(vec![
            triangle
                .to_array()
                .into_iter()
                .chain([triangle.v1()])
                .map(|coord| coord_position(tile_id, extent, coord))
                .collect(),
        ]),
    }
}

fn multi_point_positions(
    tile_id: CanonicalTileId,
    extent: f64,
    multi_point: &MultiPoint<f64>,
) -> Vec<Vec<f64>> {
    multi_point
        .0
        .iter()
        .map(|point| point_position(tile_id, extent, *point))
        .collect()
}

fn multi_line_string_positions(
    tile_id: CanonicalTileId,
    extent: f64,
    multi_line_string: &MultiLineString<f64>,
) -> Vec<Vec<Vec<f64>>> {
    multi_line_string
        .0
        .iter()
        .map(|line_string| line_string_positions(tile_id, extent, line_string))
        .collect()
}

fn multi_polygon_positions(
    tile_id: CanonicalTileId,
    extent: f64,
    multi_polygon: &MultiPolygon<f64>,
) -> Vec<Vec<Vec<Vec<f64>>>> {
    multi_polygon
        .0
        .iter()
        .map(|polygon| polygon_positions(tile_id, extent, polygon))
        .collect()
}

fn polygon_positions(
    tile_id: CanonicalTileId,
    extent: f64,
    polygon: &Polygon<f64>,
) -> Vec<Vec<Vec<f64>>> {
    std::iter::once(polygon.exterior())
        .chain(polygon.interiors())
        .map(|line_string| line_string_positions(tile_id, extent, line_string))
        .collect()
}

fn line_string_positions(
    tile_id: CanonicalTileId,
    extent: f64,
    line_string: &LineString<f64>,
) -> Vec<Vec<f64>> {
    line_string
        .0
        .iter()
        .map(|coord| coord_position(tile_id, extent, *coord))
        .collect()
}

fn point_position(tile_id: CanonicalTileId, extent: f64, point: Point<f64>) -> Vec<f64> {
    coord_position(tile_id, extent, point.0)
}

fn coord_position(tile_id: CanonicalTileId, extent: f64, coord: Coord<f64>) -> Vec<f64> {
    let tile_count = 2f64.powi(tile_id.z as i32);
    let world_x = (f64::from(tile_id.x) + coord.x / extent) / tile_count;
    let world_y = (f64::from(tile_id.y) + coord.y / extent) / tile_count;
    let lng = world_x * 360.0 - 180.0;
    let lat = (PI * (1.0 - 2.0 * world_y)).sinh().atan().to_degrees();

    vec![lng, lat]
}
