use crate::bounds::{Bounds, SlippyTile, tile_bounds};
use crate::cli::Osm2StreetsArgs;
use crate::geojson_filter::filter_geojson_to_intersecting_features;
use crate::split::SplitPbfMetadata;
use anyhow::{Context, Result, bail};
use indicatif::{ProgressBar, ProgressStyle};
use osm2streets::{DrivingSide, Filter};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::Once;

thread_local! {
    static SUPPRESS_OPTIONAL_RENDER_PANIC: Cell<bool> = Cell::new(false);
}

static INSTALL_OPTIONAL_RENDER_PANIC_HOOK: Once = Once::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Osm2StreetsMetadata {
    pub chunk: SlippyTile,
    #[serde(default)]
    pub target_bounds: Option<Bounds>,
    #[serde(default)]
    pub extraction_bounds: Option<Bounds>,
    #[serde(default)]
    pub bounds_buffer_meters: Option<f64>,
}

impl Osm2StreetsMetadata {
    pub fn target_bounds(&self) -> Bounds {
        self.target_bounds
            .unwrap_or_else(|| tile_bounds(self.chunk))
    }

    pub fn extraction_bounds(&self) -> Bounds {
        self.extraction_bounds
            .unwrap_or_else(|| self.target_bounds())
    }
}

pub fn osm2streets(args: &Osm2StreetsArgs) -> Result<()> {
    let input_files = if args.input_path.is_dir() {
        let mut files = std::fs::read_dir(&args.input_path)
            .with_context(|| {
                format!(
                    "failed to read input directory {}",
                    args.input_path.display()
                )
            })?
            .map(|entry| -> Result<_> {
                let path = entry
                    .with_context(|| {
                        format!(
                            "failed to read directory entry in {}",
                            args.input_path.display()
                        )
                    })?
                    .path();
                Ok(path)
            })
            .filter_map(|path| match path {
                Ok(path) if is_pbf_file(&path) => Some(Ok(path)),
                Ok(_) => None,
                Err(err) => Some(Err(err)),
            })
            .collect::<Result<Vec<_>>>()?;
        files.sort();
        files
    } else {
        if !is_pbf_file(&args.input_path) {
            bail!(
                "input file is not a .pbf file: {}",
                args.input_path.display()
            );
        }
        vec![args.input_path.clone()]
    };

    if input_files.is_empty() {
        bail!(
            "no .pbf files found in input directory {}",
            args.input_path.display()
        );
    }

    let config = osm2streets::MapConfig {
        driving_side: DrivingSide::Right,
        override_driving_side: None,
        country_code: String::new(),
        bikes_can_use_bus_lanes: true,
        inferred_sidewalks: true,
        parallel_street_parking_spot_length: geom::Distance::meters(8.0),
        vehicle_width_for_parking_spots: geom::Distance::meters(3.0),
        turn_on_red: true,
        include_railroads: true,
        inferred_kerbs: false,
        date_time: None,
    };

    install_optional_render_panic_hook();

    let progress = ProgressBar::new(input_files.len() as u64);
    progress.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} {msg}",
        )?
        .progress_chars("#>-"),
    );
    progress.set_message("converting osm2streets chunks");

    input_files
        .par_iter()
        .map(|input_file| {
            let result = convert_input_file(input_file, &args.output_dir, &config);
            progress.inc(1);
            result
        })
        .collect::<Result<Vec<_>>>()?;

    progress.finish_with_message("converted osm2streets chunks");

    Ok(())
}

fn convert_input_file(
    input_file: &Path,
    output_dir: &Path,
    config: &osm2streets::MapConfig,
) -> Result<()> {
    let metadata_path = input_file.with_extension("pbf.meta.json");

    let metadata = serde_json::from_slice::<SplitPbfMetadata>(
        &std::fs::read(&metadata_path)
            .with_context(|| format!("failed to read metadata file {}", metadata_path.display()))?,
    )
    .with_context(|| format!("failed to parse metadata file {}", metadata_path.display()))?;

    let data = std::fs::read(input_file)
        .with_context(|| format!("failed to read PBF file {}", input_file.display()))?;

    let target_bounds = metadata.target_bounds();

    let mut timer = abstutil::Timer::throwaway();
    let (mut street_network, _doc) =
        streets_reader::osm_to_street_network(&data, None, config.clone(), &mut timer)
            .with_context(|| {
                format!(
                    "failed to convert PBF to osm2streets network for {}",
                    input_file.display()
                )
            })?;

    let transformations = osm2streets::Transformation::standard_for_clipped_areas();
    street_network.apply_transformations(transformations, &mut timer);

    let layers = [
        (
            "network",
            filter_layer(
                "network",
                street_network.to_geojson(&Filter::All)?,
                target_bounds,
            )?,
        ),
        (
            "intersection_markings",
            filter_layer(
                "intersection_markings",
                render_optional_layer(&metadata.chunk, "intersection_markings", || {
                    street_network.to_intersection_markings_geojson(&Filter::All)
                })?,
                target_bounds,
            )?,
        ),
        // (
        //     "lane_markings",
        //     render_optional_layer(&metadata.chunk, "lane_markings", || {
        //         street_network.to_lane_markings_geojson(&Filter::All)
        //     })?,
        // ),
        (
            "lanes",
            filter_layer(
                "lanes",
                render_optional_layer(&metadata.chunk, "lanes", || {
                    street_network
                        .to_lane_polygons_geojson(&Filter::All)
                        .context("failed to render lane polygons GeoJSON")
                })?,
                target_bounds,
            )?,
        ),
    ];

    let chunk_dir = output_dir.join(&metadata.chunk.id());

    std::fs::create_dir_all(&chunk_dir)
        .with_context(|| format!("failed to create output directory {}", chunk_dir.display()))?;

    for (layer_name, layer) in layers {
        let output_path = chunk_dir.join(format!("{layer_name}.geojson"));

        std::fs::write(&output_path, layer)
            .with_context(|| format!("failed to write {}", output_path.display()))?;
    }

    let output_metadata_path = chunk_dir.join("meta.json");

    let output_metadata = serde_json::to_string(&Osm2StreetsMetadata {
        chunk: metadata.chunk,
        target_bounds: Some(target_bounds),
        extraction_bounds: Some(metadata.extraction_bounds()),
        bounds_buffer_meters: Some(metadata.bounds_buffer_meters()),
    })
    .context("failed to serialize osm2streets metadata")?;

    std::fs::write(&output_metadata_path, output_metadata)
        .with_context(|| format!("failed to write {}", output_metadata_path.display()))?;

    Ok(())
}

fn is_pbf_file(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("pbf"))
}

fn filter_layer(layer_name: &str, raw: String, target_bounds: Bounds) -> Result<String> {
    filter_geojson_to_intersecting_features(&raw, target_bounds)
        .with_context(|| format!("failed to filter {layer_name} GeoJSON to target bounds"))
}

fn render_optional_layer<F>(chunk: &SlippyTile, layer_name: &str, render: F) -> Result<String>
where
    F: FnOnce() -> Result<String>,
{
    SUPPRESS_OPTIONAL_RENDER_PANIC.with(|flag| flag.set(true));
    let result = catch_unwind(AssertUnwindSafe(render));
    SUPPRESS_OPTIONAL_RENDER_PANIC.with(|flag| flag.set(false));

    match result {
        Ok(Ok(layer)) => Ok(layer),
        Ok(Err(err)) => Err(err).with_context(|| format!("failed to render {layer_name} GeoJSON")),
        Err(panic) => {
            eprintln!(
                "skipping {layer_name}.geojson for chunk {} after renderer panic: {}",
                chunk.id(),
                panic_message(&panic)
            );
            Ok(empty_feature_collection())
        }
    }
}

fn install_optional_render_panic_hook() {
    INSTALL_OPTIONAL_RENDER_PANIC_HOOK.call_once(|| {
        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let suppress = SUPPRESS_OPTIONAL_RENDER_PANIC.with(|flag| flag.get());
            if !suppress {
                previous_hook(info);
            }
        }));
    });
}

fn panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|message| message.to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic payload".to_string())
}

fn empty_feature_collection() -> String {
    r#"{"type":"FeatureCollection","features":[]}"#.to_string()
}
