use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

pub const WEB_MERCATOR_MAX_LAT: f64 = 85.051_128_78;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

impl Bounds {
    pub fn validate(self) -> Result<Self> {
        if self.west < -180.0
            || self.east > 180.0
            || self.south < -WEB_MERCATOR_MAX_LAT
            || self.north > WEB_MERCATOR_MAX_LAT
            || self.west >= self.east
            || self.south >= self.north
        {
            bail!(
                "bounds must be ordered west,south,east,north and be inside web-mercator limits: {self}"
            );
        }
        Ok(self)
    }

    pub fn expand_meters(self, meters: f64) -> Self {
        if meters <= 0.0 {
            return self;
        }

        let lat_delta = meters / 111_320.0;
        let center_lat = ((self.south + self.north) * 0.5).to_radians();
        let lon_meters = (111_320.0 * center_lat.cos().abs()).max(1.0);
        let lon_delta = meters / lon_meters;

        Self {
            west: (self.west - lon_delta).max(-180.0),
            south: (self.south - lat_delta).max(-WEB_MERCATOR_MAX_LAT),
            east: (self.east + lon_delta).min(180.0),
            north: (self.north + lat_delta).min(WEB_MERCATOR_MAX_LAT),
        }
    }

    pub fn intersects(self, other: Self) -> bool {
        self.west < other.east
            && self.east > other.west
            && self.south < other.north
            && self.north > other.south
    }

    pub fn intersects_inclusive(self, other: Self) -> bool {
        self.west <= other.east
            && self.east >= other.west
            && self.south <= other.north
            && self.north >= other.south
    }

    pub fn contains_lon_lat(self, lon: f64, lat: f64) -> bool {
        lon >= self.west && lon <= self.east && lat >= self.south && lat <= self.north
    }
}

impl fmt::Display for Bounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{},{},{},{}",
            self.west, self.south, self.east, self.north
        )
    }
}

impl FromStr for Bounds {
    type Err = anyhow::Error;

    fn from_str(input: &str) -> Result<Self> {
        let parts: Vec<f64> = input
            .split(',')
            .map(|part| {
                part.trim()
                    .parse::<f64>()
                    .with_context(|| format!("invalid numeric bound in {input:?}"))
            })
            .collect::<Result<_>>()?;
        if parts.len() != 4 {
            bail!("invalid bounds {input:?}; expected west,south,east,north");
        }

        Bounds {
            west: parts[0],
            south: parts[1],
            east: parts[2],
            north: parts[3],
        }
        .validate()
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SlippyTile {
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

impl SlippyTile {
    pub fn id(self) -> String {
        format!("{}-{}-{}", self.z, self.x, self.y)
    }
}

pub fn lon_to_tile_x(lon: f64, z: u8) -> u32 {
    let n = 1_u32 << z;
    let x = (((lon.clamp(-180.0, 180.0) + 180.0) / 360.0) * n as f64).floor();
    (x as i64).clamp(0, n as i64 - 1) as u32
}

pub fn lat_to_tile_y(lat: f64, z: u8) -> u32 {
    let n = 1_u32 << z;
    let lat_rad = lat
        .clamp(-WEB_MERCATOR_MAX_LAT, WEB_MERCATOR_MAX_LAT)
        .to_radians();
    let y = ((1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI) / 2.0
        * n as f64)
        .floor();
    (y as i64).clamp(0, n as i64 - 1) as u32
}

pub fn tile_bounds(tile: SlippyTile) -> Bounds {
    let n = 2_f64.powi(tile.z as i32);
    let west = tile.x as f64 / n * 360.0 - 180.0;
    let east = (tile.x as f64 + 1.0) / n * 360.0 - 180.0;
    let north = tile_y_to_lat(tile.y as f64, n);
    let south = tile_y_to_lat(tile.y as f64 + 1.0, n);
    Bounds {
        west,
        south,
        east,
        north,
    }
}

fn tile_y_to_lat(y: f64, n: f64) -> f64 {
    let value = std::f64::consts::PI * (1.0 - 2.0 * y / n);
    value.sinh().atan().to_degrees()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileRange {
    pub z: u8,
    pub min_x: u32,
    pub max_x: u32,
    pub min_y: u32,
    pub max_y: u32,
}

pub fn tile_range_for_bounds(bounds: Bounds, z: u8) -> TileRange {
    TileRange {
        z,
        min_x: lon_to_tile_x(bounds.west, z),
        max_x: lon_to_tile_x(bounds.east, z),
        min_y: lat_to_tile_y(bounds.north, z),
        max_y: lat_to_tile_y(bounds.south, z),
    }
}

pub fn tiles_for_bounds(bounds: Bounds, z: u8) -> Vec<SlippyTile> {
    let range = tile_range_for_bounds(bounds, z);
    let mut tiles = Vec::new();
    for x in range.min_x..=range.max_x {
        for y in range.min_y..=range.max_y {
            tiles.push(SlippyTile { z, x, y });
        }
    }
    tiles
}

pub fn count_tiles(bounds: Bounds, min_zoom: u8, max_zoom: u8) -> usize {
    (min_zoom..=max_zoom)
        .map(|z| {
            let range = tile_range_for_bounds(bounds, z);
            ((range.max_x - range.min_x + 1) as usize) * ((range.max_y - range.min_y + 1) as usize)
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_validates_bounds() {
        let bounds: Bounds = "13.36,52.49,13.46,52.535".parse().unwrap();
        assert_eq!(bounds.west, 13.36);
        assert!("13.46,52.49,13.36,52.535".parse::<Bounds>().is_err());
    }

    #[test]
    fn splits_berlin_bounds_into_expected_z13_chunks() {
        let bounds: Bounds = "13.36,52.49,13.46,52.535".parse().unwrap();
        let tiles = tiles_for_bounds(bounds, 13);
        let ids: Vec<String> = tiles.into_iter().map(SlippyTile::id).collect();
        assert_eq!(
            ids,
            vec![
                "13-4400-2686",
                "13-4400-2687",
                "13-4401-2686",
                "13-4401-2687",
                "13-4402-2686",
                "13-4402-2687"
            ]
        );
    }

    #[test]
    fn expands_bounds_by_meters() {
        let bounds: Bounds = "13.36,52.49,13.46,52.535".parse().unwrap();
        let expanded = bounds.expand_meters(100.0);
        assert!(expanded.west < bounds.west);
        assert!(expanded.south < bounds.south);
        assert!(expanded.east > bounds.east);
        assert!(expanded.north > bounds.north);
    }

    #[test]
    fn detects_intersecting_bounds() {
        let berlin: Bounds =
            "13.254900864675482,52.44406891160262,13.530024231365633,52.57054636605862"
                .parse()
                .unwrap();
        let swapped: Bounds =
            "52.44406891160262,13.254900864675482,52.57054636605862,13.530024231365633"
                .parse()
                .unwrap();
        assert!(!berlin.intersects(swapped));
        assert!(berlin.intersects(berlin.expand_meters(100.0)));
    }

    #[test]
    fn inclusive_intersection_counts_touching_boundaries() {
        let left: Bounds = "0,0,1,1".parse().unwrap();
        let right: Bounds = "1,0,2,1".parse().unwrap();
        assert!(!left.intersects(right));
        assert!(left.intersects_inclusive(right));
        assert!(left.contains_lon_lat(1.0, 0.5));
    }
}
