use crate::bounds::{Bounds, SlippyTile, tile_bounds};
use crate::cli::GeojsonToMvtArgs;
use crate::geojson_filter::feature_intersects_bounds;
use crate::osm2streets::Osm2StreetsMetadata;
use anyhow::{Context, Result, bail};
use geojson::{Feature, FeatureCollection, GeoJson};
use geojson_vt_rs::{TileOptions, geojson_to_tile};
use geozero::GeozeroDatasource;
use geozero::geojson::GeoJsonString;
use geozero::mvt::{MvtWriter, Tile as MvtTile};
use indicatif::{ProgressBar, ProgressStyle};
use prost::Message;
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const MAX_MVT_ZOOM: u8 = 24;

#[derive(Debug, Clone)]
struct ChunkInput {
    dir: PathBuf,
    chunk: SlippyTile,
    extraction_bounds: Bounds,
    layers: Vec<LayerInput>,
}

#[derive(Debug, Clone)]
struct LayerInput {
    name: String,
    path: PathBuf,
}

#[derive(Debug, Clone)]
struct TileJob {
    tile: SlippyTile,
    source_chunk_indices: Vec<usize>,
}

pub fn geojson_to_mvt(args: &GeojsonToMvtArgs) -> Result<()> {
    if args.zoom > MAX_MVT_ZOOM {
        bail!("zoom must be <= {MAX_MVT_ZOOM}; got {}", args.zoom);
    }
    validate_tile_options(args)?;
    if !args.input_dir.is_dir() {
        bail!(
            "input directory does not exist or is not a directory: {}",
            args.input_dir.display()
        );
    }

    let chunks = discover_chunks(&args.input_dir)?;
    if chunks.is_empty() {
        bail!(
            "no osm2streets chunk subdirectories with meta.json found in {}",
            args.input_dir.display()
        );
    }

    let jobs = tile_jobs_for_chunks(&chunks, args.zoom)?;
    if jobs.is_empty() {
        bail!(
            "no MVT tile jobs produced from {} chunk directories at zoom {}",
            chunks.len(),
            args.zoom
        );
    }

    std::fs::create_dir_all(&args.output_dir)
        .with_context(|| format!("failed to create {}", args.output_dir.display()))?;

    let progress = ProgressBar::new(jobs.len() as u64);
    progress.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} {msg}",
        )?
        .progress_chars("#>-"),
    );
    progress.set_message("writing mvt tiles");

    jobs.par_iter()
        .map(|job| {
            let result = write_tile_job(job, &chunks, args);
            progress.inc(1);
            result
        })
        .collect::<Result<Vec<_>>>()?;

    progress.finish_with_message("wrote mvt tiles");

    Ok(())
}

fn validate_tile_options(args: &GeojsonToMvtArgs) -> Result<()> {
    if args.extent == 0 {
        bail!("MVT extent must be > 0");
    }
    if !args.tolerance.is_finite() || args.tolerance < 0.0 {
        bail!(
            "MVT simplification tolerance must be finite and >= 0; got {}",
            args.tolerance
        );
    }
    if args.buffer >= args.extent {
        bail!(
            "MVT buffer must be smaller than extent; got buffer {} and extent {}",
            args.buffer,
            args.extent
        );
    }
    Ok(())
}

fn discover_chunks(input_dir: &Path) -> Result<Vec<ChunkInput>> {
    let mut chunks = std::fs::read_dir(input_dir)
        .with_context(|| format!("failed to read input directory {}", input_dir.display()))?
        .map(|entry| -> Result<Option<ChunkInput>> {
            let dir = entry
                .with_context(|| {
                    format!("failed to read directory entry in {}", input_dir.display())
                })?
                .path();

            if !dir.is_dir() {
                return Ok(None);
            }

            let metadata_path = dir.join("meta.json");
            if !metadata_path.is_file() {
                return Ok(None);
            }

            let metadata = serde_json::from_slice::<Osm2StreetsMetadata>(
                &std::fs::read(&metadata_path)
                    .with_context(|| format!("failed to read {}", metadata_path.display()))?,
            )
            .with_context(|| format!("failed to parse {}", metadata_path.display()))?;

            let mut layers = std::fs::read_dir(&dir)
                .with_context(|| format!("failed to read chunk directory {}", dir.display()))?
                .map(|entry| -> Result<Option<LayerInput>> {
                    let path = entry
                        .with_context(|| {
                            format!("failed to read directory entry in {}", dir.display())
                        })?
                        .path();
                    if !is_geojson_file(&path) {
                        return Ok(None);
                    }

                    let name = path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .with_context(|| {
                            format!("failed to determine layer name from {}", path.display())
                        })?
                        .to_string();

                    Ok(Some(LayerInput { name, path }))
                })
                .filter_map(|layer| match layer {
                    Ok(Some(layer)) => Some(Ok(layer)),
                    Ok(None) => None,
                    Err(err) => Some(Err(err)),
                })
                .collect::<Result<Vec<_>>>()?;

            layers.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.path.cmp(&b.path)));

            Ok(Some(ChunkInput {
                dir,
                chunk: metadata.chunk,
                extraction_bounds: metadata.extraction_bounds(),
                layers,
            }))
        })
        .filter_map(|chunk| match chunk {
            Ok(Some(chunk)) => Some(Ok(chunk)),
            Ok(None) => None,
            Err(err) => Some(Err(err)),
        })
        .collect::<Result<Vec<_>>>()?;

    chunks.sort_by(|a, b| {
        a.chunk
            .z
            .cmp(&b.chunk.z)
            .then_with(|| a.chunk.x.cmp(&b.chunk.x))
            .then_with(|| a.chunk.y.cmp(&b.chunk.y))
            .then_with(|| a.dir.cmp(&b.dir))
    });

    Ok(chunks)
}

fn tile_jobs_for_chunks(chunks: &[ChunkInput], zoom: u8) -> Result<Vec<TileJob>> {
    let mut tiles = BTreeSet::new();

    for chunk in chunks {
        for tile in target_tiles_for_chunk(chunk.chunk, zoom)? {
            tiles.insert(tile);
        }
    }

    let mut jobs = Vec::new();
    for tile in tiles {
        let bounds = tile_bounds(tile);
        let source_chunk_indices = chunks
            .iter()
            .enumerate()
            .filter_map(|(idx, chunk)| {
                if chunk.extraction_bounds.intersects_inclusive(bounds) {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        if source_chunk_indices.is_empty() {
            bail!("no source chunks overlap output tile {}", tile.id());
        }

        jobs.push(TileJob {
            tile,
            source_chunk_indices,
        });
    }

    Ok(jobs)
}

fn target_tiles_for_chunk(chunk: SlippyTile, zoom: u8) -> Result<Vec<SlippyTile>> {
    if chunk.z > MAX_MVT_ZOOM {
        bail!(
            "chunk {} has zoom {}; max supported zoom is {MAX_MVT_ZOOM}",
            chunk.id(),
            chunk.z
        );
    }

    if zoom == chunk.z {
        return Ok(vec![chunk]);
    }

    if zoom < chunk.z {
        let delta = chunk.z - zoom;
        return Ok(vec![SlippyTile {
            z: zoom,
            x: chunk.x >> delta,
            y: chunk.y >> delta,
        }]);
    }

    let delta = zoom - chunk.z;
    let scale = 1_u32
        .checked_shl(delta as u32)
        .with_context(|| format!("zoom delta {delta} is too large for chunk {}", chunk.id()))?;
    let min_x = chunk
        .x
        .checked_mul(scale)
        .with_context(|| format!("x overflow while splitting chunk {}", chunk.id()))?;
    let min_y = chunk
        .y
        .checked_mul(scale)
        .with_context(|| format!("y overflow while splitting chunk {}", chunk.id()))?;

    let mut tiles = Vec::with_capacity((scale as usize).saturating_mul(scale as usize));
    for x_offset in 0..scale {
        for y_offset in 0..scale {
            tiles.push(SlippyTile {
                z: zoom,
                x: min_x + x_offset,
                y: min_y + y_offset,
            });
        }
    }

    Ok(tiles)
}

fn write_tile_job(job: &TileJob, chunks: &[ChunkInput], args: &GeojsonToMvtArgs) -> Result<()> {
    let mut layer_features = BTreeMap::<String, Vec<Feature>>::new();
    let mut seen_features = BTreeMap::<String, BTreeSet<String>>::new();
    let target_bounds = tile_bounds(job.tile);

    for chunk_index in &job.source_chunk_indices {
        let chunk = chunks.get(*chunk_index).with_context(|| {
            format!(
                "tile {} references missing chunk index {}",
                job.tile.id(),
                chunk_index
            )
        })?;

        for layer in &chunk.layers {
            let features = read_geojson_features(&layer.path).with_context(|| {
                format!(
                    "failed to read layer {} from chunk {} for tile {}",
                    layer.name,
                    chunk.chunk.id(),
                    job.tile.id()
                )
            })?;
            for feature in features {
                if !feature_intersects_bounds(&feature, target_bounds) {
                    continue;
                }

                let feature_key = serde_json::to_string(&feature).with_context(|| {
                    format!(
                        "failed to build dedupe key for layer {} from chunk {}",
                        layer.name,
                        chunk.chunk.id()
                    )
                })?;
                if !seen_features
                    .entry(layer.name.clone())
                    .or_default()
                    .insert(feature_key)
                {
                    continue;
                }

                layer_features
                    .entry(layer.name.clone())
                    .or_default()
                    .push(feature);
            }
        }
    }

    let options = TileOptions {
        tolerance: args.tolerance,
        extent: args.extent,
        buffer: args.buffer,
        ..TileOptions::default()
    };

    let mut mvt_tile = MvtTile::default();
    for (layer_name, features) in layer_features {
        if features.is_empty() {
            continue;
        }

        let geojson = GeoJson::FeatureCollection(FeatureCollection {
            bbox: None,
            features,
            foreign_members: None,
        });
        let tile = geojson_to_tile(
            &geojson, job.tile.z, job.tile.x, job.tile.y, &options, false, true,
        );
        if tile.features.features.is_empty() {
            continue;
        }

        let mvt_layer = feature_collection_to_mvt_layer(&layer_name, tile.features, args.extent)
            .with_context(|| {
                format!("failed to encode layer {layer_name} for {}", job.tile.id())
            })?;
        if !mvt_layer.features.is_empty() {
            mvt_tile.layers.push(mvt_layer);
        }
    }

    if mvt_tile.layers.is_empty() {
        return Ok(());
    }

    let tile_dir = args
        .output_dir
        .join(job.tile.z.to_string())
        .join(job.tile.x.to_string());
    std::fs::create_dir_all(&tile_dir)
        .with_context(|| format!("failed to create {}", tile_dir.display()))?;

    let output_path = tile_dir.join(format!("{}.mvt", job.tile.y));
    std::fs::write(&output_path, mvt_tile.encode_to_vec())
        .with_context(|| format!("failed to write {}", output_path.display()))?;

    Ok(())
}

fn read_geojson_features(path: &Path) -> Result<Vec<Feature>> {
    let geojson = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read GeoJSON file {}", path.display()))?
        .parse::<GeoJson>()
        .with_context(|| format!("failed to parse GeoJSON file {}", path.display()))?;

    Ok(match geojson {
        GeoJson::FeatureCollection(collection) => collection.features,
        GeoJson::Feature(feature) => vec![feature],
        GeoJson::Geometry(geometry) => vec![Feature {
            bbox: None,
            geometry: Some(geometry),
            id: None,
            properties: None,
            foreign_members: None,
        }],
    })
}

fn feature_collection_to_mvt_layer(
    layer_name: &str,
    feature_collection: FeatureCollection,
    extent: u16,
) -> Result<geozero::mvt::tile::Layer> {
    let geojson = serde_json::to_string(&GeoJson::FeatureCollection(feature_collection))
        .context("failed to serialize tile GeoJSON before MVT encoding")?;
    let mut geojson = GeoJsonString(geojson);
    let mut writer =
        MvtWriter::new_unscaled(extent as u32).context("failed to create unscaled MVT writer")?;
    geojson
        .process(&mut writer)
        .context("failed to process tile GeoJSON into MVT layer")?;
    Ok(writer.layer(layer_name))
}

fn is_geojson_file(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("geojson"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{DEFAULT_MVT_BUFFER, DEFAULT_MVT_EXTENT, DEFAULT_MVT_TOLERANCE};

    fn test_args() -> GeojsonToMvtArgs {
        GeojsonToMvtArgs {
            input_dir: PathBuf::from("input"),
            output_dir: PathBuf::from("output"),
            zoom: 15,
            extent: DEFAULT_MVT_EXTENT,
            buffer: DEFAULT_MVT_BUFFER,
            tolerance: DEFAULT_MVT_TOLERANCE,
        }
    }

    #[test]
    fn lower_zoom_uses_parent_tile_for_merging() {
        let tiles = target_tiles_for_chunk(
            SlippyTile {
                z: 13,
                x: 4400,
                y: 2686,
            },
            12,
        )
        .unwrap();
        assert_eq!(
            tiles,
            vec![SlippyTile {
                z: 12,
                x: 2200,
                y: 1343
            }]
        );
    }

    #[test]
    fn child_tiles_are_used_when_target_zoom_is_higher_than_chunk() {
        let tiles = target_tiles_for_chunk(
            SlippyTile {
                z: 13,
                x: 4400,
                y: 2686,
            },
            14,
        )
        .unwrap();
        assert_eq!(
            tiles,
            vec![
                SlippyTile {
                    z: 14,
                    x: 8800,
                    y: 5372
                },
                SlippyTile {
                    z: 14,
                    x: 8800,
                    y: 5373
                },
                SlippyTile {
                    z: 14,
                    x: 8801,
                    y: 5372
                },
                SlippyTile {
                    z: 14,
                    x: 8801,
                    y: 5373
                },
            ]
        );
    }

    #[test]
    fn default_mvt_options_preserve_geometry_precision() {
        let args = test_args();
        validate_tile_options(&args).unwrap();
        assert_eq!(args.extent, 4096);
        assert_eq!(args.buffer, 64);
        assert_eq!(args.tolerance, 0.0);
    }

    #[test]
    fn rejects_lossy_or_invalid_mvt_options() {
        let mut args = test_args();
        args.tolerance = -1.0;
        assert!(validate_tile_options(&args).is_err());

        let mut args = test_args();
        args.buffer = args.extent;
        assert!(validate_tile_options(&args).is_err());

        let mut args = test_args();
        args.extent = 0;
        assert!(validate_tile_options(&args).is_err());
    }
}
