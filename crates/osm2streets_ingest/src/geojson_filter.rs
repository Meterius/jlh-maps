use crate::bounds::Bounds;
use anyhow::{Context, Result};
use geojson::{Feature, FeatureCollection, GeoJson, Geometry, Value};

#[derive(Debug, Clone, Copy, PartialEq)]
struct GeometryBounds {
    west: f64,
    south: f64,
    east: f64,
    north: f64,
}

impl GeometryBounds {
    fn from_geometry(geometry: &Geometry) -> Option<Self> {
        let mut bounds = None;
        add_value_bounds(&geometry.value, &mut bounds);
        bounds
    }

    fn add_position(&mut self, position: &[f64]) {
        if position.len() < 2 {
            return;
        }

        self.west = self.west.min(position[0]);
        self.south = self.south.min(position[1]);
        self.east = self.east.max(position[0]);
        self.north = self.north.max(position[1]);
    }

    fn intersects(self, target: Bounds) -> bool {
        target.intersects_inclusive(Bounds {
            west: self.west,
            south: self.south,
            east: self.east,
            north: self.north,
        })
    }
}

pub fn filter_geojson_to_intersecting_features(raw: &str, target: Bounds) -> Result<String> {
    let geojson = raw
        .parse::<GeoJson>()
        .context("failed to parse GeoJSON before intersection filtering")?;

    let collection = match geojson {
        GeoJson::FeatureCollection(mut collection) => {
            collection.features =
                filter_features_to_intersecting_bounds(collection.features, target);
            collection
        }
        GeoJson::Feature(feature) => FeatureCollection {
            bbox: None,
            features: filter_features_to_intersecting_bounds(vec![feature], target),
            foreign_members: None,
        },
        GeoJson::Geometry(geometry) => {
            let feature = Feature {
                bbox: None,
                geometry: Some(geometry),
                id: None,
                properties: None,
                foreign_members: None,
            };
            FeatureCollection {
                bbox: None,
                features: filter_features_to_intersecting_bounds(vec![feature], target),
                foreign_members: None,
            }
        }
    };

    serde_json::to_string_pretty(&GeoJson::FeatureCollection(collection))
        .context("failed to serialize filtered GeoJSON")
}

pub fn filter_features_to_intersecting_bounds(
    features: Vec<Feature>,
    target: Bounds,
) -> Vec<Feature> {
    features
        .into_iter()
        .filter(|feature| feature_intersects_bounds(feature, target))
        .collect()
}

pub fn feature_intersects_bounds(feature: &Feature, target: Bounds) -> bool {
    feature
        .geometry
        .as_ref()
        .and_then(GeometryBounds::from_geometry)
        .is_some_and(|bounds| bounds.intersects(target))
}

fn add_value_bounds(value: &Value, bounds: &mut Option<GeometryBounds>) {
    match value {
        Value::Point(position) => add_position_bounds(position, bounds),
        Value::MultiPoint(positions) | Value::LineString(positions) => {
            for position in positions {
                add_position_bounds(position, bounds);
            }
        }
        Value::MultiLineString(lines) | Value::Polygon(lines) => {
            for line in lines {
                for position in line {
                    add_position_bounds(position, bounds);
                }
            }
        }
        Value::MultiPolygon(polygons) => {
            for polygon in polygons {
                for line in polygon {
                    for position in line {
                        add_position_bounds(position, bounds);
                    }
                }
            }
        }
        Value::GeometryCollection(geometries) => {
            for geometry in geometries {
                add_value_bounds(&geometry.value, bounds);
            }
        }
    }
}

fn add_position_bounds(position: &[f64], bounds: &mut Option<GeometryBounds>) {
    if position.len() < 2 {
        return;
    }

    match bounds {
        Some(bounds) => bounds.add_position(position),
        None => {
            *bounds = Some(GeometryBounds {
                west: position[0],
                south: position[1],
                east: position[0],
                north: position[1],
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_whole_geometry_when_bbox_intersects_target() {
        let target: Bounds = "0,0,1,1".parse().unwrap();
        let raw = r#"{"type":"FeatureCollection","features":[{"type":"Feature","properties":{"id":1},"geometry":{"type":"LineString","coordinates":[[-1,0.5],[2,0.5]]}},{"type":"Feature","properties":{"id":2},"geometry":{"type":"Point","coordinates":[2,2]}}]}"#;

        let filtered = filter_geojson_to_intersecting_features(raw, target).unwrap();
        let GeoJson::FeatureCollection(collection) = filtered.parse::<GeoJson>().unwrap() else {
            panic!("expected feature collection");
        };

        assert_eq!(collection.features.len(), 1);
        assert_eq!(
            collection.features[0].geometry.as_ref().unwrap().value,
            Value::LineString(vec![vec![-1.0, 0.5], vec![2.0, 0.5]])
        );
    }

    #[test]
    fn keeps_boundary_touching_point() {
        let target: Bounds = "0,0,1,1".parse().unwrap();
        let feature = Feature {
            bbox: None,
            geometry: Some(Geometry::new(Value::Point(vec![1.0, 0.5]))),
            id: None,
            properties: None,
            foreign_members: None,
        };

        assert!(feature_intersects_bounds(&feature, target));
    }
}
