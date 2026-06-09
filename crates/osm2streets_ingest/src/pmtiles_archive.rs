use crate::bounds::{Bounds, SlippyTile, tile_bounds};
use crate::cli::MvtToPmtilesArgs;
use anyhow::{Context, Result, bail};
use geozero::mvt::Tile as MvtTile;
use indicatif::{ProgressBar, ProgressStyle};
use pmtiles::{Compression, PmTilesWriter, TileCoord, TileId, TileType};
use prost::Message;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct MvtInput {
    tile: SlippyTile,
    coord: TileCoord,
    tile_id: u64,
    path: PathBuf,
}

pub fn mvt_to_pmtiles(args: &MvtToPmtilesArgs) -> Result<()> {
    if !args.input_dir.is_dir() {
        bail!(
            "input directory does not exist or is not a directory: {}",
            args.input_dir.display()
        );
    }
    if args.output_pmtiles.is_dir() {
        bail!(
            "output path is a directory, expected a .pmtiles file path: {}",
            args.output_pmtiles.display()
        );
    }

    let tiles = discover_mvt_tiles(&args.input_dir)?;
    if tiles.is_empty() {
        bail!("no .mvt tiles found in {}", args.input_dir.display());
    }

    let metadata = metadata_for_tiles(&tiles, &args.output_pmtiles)?;
    let stats = stats_for_tiles(&tiles)?;

    if let Some(parent) = args.output_pmtiles.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let output = File::create(&args.output_pmtiles)
        .with_context(|| format!("failed to create {}", args.output_pmtiles.display()))?;
    let mut writer = PmTilesWriter::new(TileType::Mvt)
        .tile_compression(Compression::Gzip)
        .min_zoom(stats.min_zoom)
        .max_zoom(stats.max_zoom)
        .bounds(
            stats.bounds.west,
            stats.bounds.south,
            stats.bounds.east,
            stats.bounds.north,
        )
        .center_zoom(stats.min_zoom)
        .center(
            (stats.bounds.west + stats.bounds.east) * 0.5,
            (stats.bounds.south + stats.bounds.north) * 0.5,
        )
        .metadata(&metadata)
        .create(output)
        .with_context(|| format!("failed to initialize {}", args.output_pmtiles.display()))?;

    let progress = ProgressBar::new(tiles.len() as u64);
    progress.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} {msg}",
        )?
        .progress_chars("#>-"),
    );
    progress.set_message("writing pmtiles");

    for tile in &tiles {
        let data = std::fs::read(&tile.path)
            .with_context(|| format!("failed to read MVT tile {}", tile.path.display()))?;
        writer.add_tile(tile.coord, &data).with_context(|| {
            format!(
                "failed to add MVT tile {} from {}",
                tile.tile.id(),
                tile.path.display()
            )
        })?;
        progress.inc(1);
    }

    writer
        .finalize()
        .with_context(|| format!("failed to finalize {}", args.output_pmtiles.display()))?;
    progress.finish_with_message("wrote pmtiles");

    Ok(())
}

fn discover_mvt_tiles(input_dir: &Path) -> Result<Vec<MvtInput>> {
    let mut tiles = Vec::new();

    for z_entry in std::fs::read_dir(input_dir)
        .with_context(|| format!("failed to read input directory {}", input_dir.display()))?
    {
        let z_path = z_entry
            .with_context(|| format!("failed to read directory entry in {}", input_dir.display()))?
            .path();
        if !z_path.is_dir() {
            continue;
        }
        let z = parse_path_component::<u8>(&z_path, "zoom")?;

        for x_entry in std::fs::read_dir(&z_path)
            .with_context(|| format!("failed to read zoom directory {}", z_path.display()))?
        {
            let x_path = x_entry
                .with_context(|| format!("failed to read directory entry in {}", z_path.display()))?
                .path();
            if !x_path.is_dir() {
                continue;
            }
            let x = parse_path_component::<u32>(&x_path, "x")?;

            for y_entry in std::fs::read_dir(&x_path)
                .with_context(|| format!("failed to read x directory {}", x_path.display()))?
            {
                let path = y_entry
                    .with_context(|| {
                        format!("failed to read directory entry in {}", x_path.display())
                    })?
                    .path();
                if !is_mvt_file(&path) {
                    continue;
                }

                let y = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .with_context(|| format!("failed to parse tile y from {}", path.display()))?
                    .parse::<u32>()
                    .with_context(|| format!("invalid tile y in {}", path.display()))?;
                let coord = TileCoord::new(z, x, y).with_context(|| {
                    format!(
                        "invalid PMTiles coordinate {z}/{x}/{y} from {}",
                        path.display()
                    )
                })?;
                let tile = SlippyTile { z, x, y };
                tiles.push(MvtInput {
                    tile,
                    coord,
                    tile_id: TileId::from(coord).value(),
                    path,
                });
            }
        }
    }

    tiles.sort_by(|a, b| a.tile_id.cmp(&b.tile_id).then_with(|| a.path.cmp(&b.path)));

    for pair in tiles.windows(2) {
        let [prev, next] = pair else {
            continue;
        };
        if prev.tile == next.tile {
            bail!(
                "duplicate MVT tile {} at {} and {}",
                prev.tile.id(),
                prev.path.display(),
                next.path.display()
            );
        }
    }

    Ok(tiles)
}

fn metadata_for_tiles(tiles: &[MvtInput], output_pmtiles: &Path) -> Result<String> {
    let stats = stats_for_tiles(tiles)?;
    let vector_layers = vector_layers_for_tiles(tiles)?;
    let name = output_pmtiles
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("osm2streets");

    serde_json::to_string(&json!({
        "name": name,
        "description": "osm2streets vector tiles",
        "version": "1",
        "type": "overlay",
        "format": "pbf",
        "minzoom": stats.min_zoom,
        "maxzoom": stats.max_zoom,
        "bounds": [
            stats.bounds.west,
            stats.bounds.south,
            stats.bounds.east,
            stats.bounds.north
        ],
        "center": [
            (stats.bounds.west + stats.bounds.east) * 0.5,
            (stats.bounds.south + stats.bounds.north) * 0.5,
            stats.min_zoom
        ],
        "vector_layers": vector_layers
    }))
    .context("failed to serialize PMTiles metadata")
}

fn vector_layers_for_tiles(tiles: &[MvtInput]) -> Result<Vec<Value>> {
    let mut layers = BTreeMap::<String, BTreeMap<String, String>>::new();

    for tile in tiles {
        let data = std::fs::read(&tile.path)
            .with_context(|| format!("failed to read MVT tile {}", tile.path.display()))?;
        let decoded = MvtTile::decode(&data[..]).with_context(|| {
            format!(
                "failed to decode MVT tile {}; expected uncompressed MVT protobuf",
                tile.path.display()
            )
        })?;

        for layer in decoded.layers {
            let fields = layers.entry(layer.name).or_default();
            for feature in layer.features {
                for tag in feature.tags.chunks_exact(2) {
                    let key_idx = tag[0] as usize;
                    let value_idx = tag[1] as usize;
                    let Some(key) = layer.keys.get(key_idx) else {
                        continue;
                    };
                    let Some(value) = layer.values.get(value_idx) else {
                        continue;
                    };
                    let value_type = mvt_value_type(value);
                    fields
                        .entry(key.clone())
                        .and_modify(|existing| {
                            if existing != value_type {
                                *existing = "String".to_string();
                            }
                        })
                        .or_insert_with(|| value_type.to_string());
                }
            }
        }
    }

    Ok(layers
        .into_iter()
        .map(|(id, fields)| {
            let fields = fields
                .into_iter()
                .map(|(key, value_type)| (key, Value::String(value_type)))
                .collect::<Map<_, _>>();
            json!({
                "id": id,
                "fields": fields,
                "description": ""
            })
        })
        .collect())
}

fn mvt_value_type(value: &geozero::mvt::tile::Value) -> &'static str {
    if value.bool_value.is_some() {
        "Boolean"
    } else if value.string_value.is_some() {
        "String"
    } else {
        "Number"
    }
}

#[derive(Debug, Clone, Copy)]
struct TileStats {
    min_zoom: u8,
    max_zoom: u8,
    bounds: Bounds,
}

fn stats_for_tiles(tiles: &[MvtInput]) -> Result<TileStats> {
    let first = tiles
        .first()
        .with_context(|| "cannot compute PMTiles stats without any tiles")?;
    let mut min_zoom = first.tile.z;
    let mut max_zoom = first.tile.z;
    let mut bounds = tile_bounds(first.tile);

    for tile in &tiles[1..] {
        min_zoom = min_zoom.min(tile.tile.z);
        max_zoom = max_zoom.max(tile.tile.z);

        let tile_bounds = tile_bounds(tile.tile);
        bounds.west = bounds.west.min(tile_bounds.west);
        bounds.south = bounds.south.min(tile_bounds.south);
        bounds.east = bounds.east.max(tile_bounds.east);
        bounds.north = bounds.north.max(tile_bounds.north);
    }

    Ok(TileStats {
        min_zoom,
        max_zoom,
        bounds,
    })
}

fn parse_path_component<T>(path: &Path, component_name: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    path.file_name()
        .and_then(|name| name.to_str())
        .with_context(|| format!("failed to parse {component_name} from {}", path.display()))?
        .parse::<T>()
        .map_err(|err| anyhow::anyhow!("invalid {component_name} in {}: {err}", path.display()))
}

fn is_mvt_file(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mvt"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_zoom_and_bounds_from_tiles() {
        let tiles = vec![
            test_tile(13, 4400, 2686),
            test_tile(13, 4401, 2687),
            test_tile(14, 8800, 5372),
        ];

        let stats = stats_for_tiles(&tiles).unwrap();
        assert_eq!(stats.min_zoom, 13);
        assert_eq!(stats.max_zoom, 14);
        assert!(stats.bounds.west < stats.bounds.east);
        assert!(stats.bounds.south < stats.bounds.north);
    }

    fn test_tile(z: u8, x: u32, y: u32) -> MvtInput {
        let coord = TileCoord::new(z, x, y).unwrap();
        MvtInput {
            tile: SlippyTile { z, x, y },
            coord,
            tile_id: TileId::from(coord).value(),
            path: PathBuf::from(format!("{z}/{x}/{y}.mvt")),
        }
    }
}
