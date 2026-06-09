use crate::bounds::Bounds;
use crate::mvt::geojson_to_mvt;
use crate::osm2streets::osm2streets;
use crate::pmtiles_archive::mvt_to_pmtiles;
use crate::split::split_pbf;
use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

pub const DEFAULT_MVT_EXTENT: u16 = 4096;
pub const DEFAULT_MVT_BUFFER: u16 = 64;
pub const DEFAULT_MVT_TOLERANCE: f64 = 0.0;

#[derive(Debug, Parser)]
#[command(name = "osm2streets_ingest")]
#[command(about = "Rust ingestion pipeline for osm2streets MBTiles generation")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Split an OSM PBF into slippy-tile chunks and OSM XML inputs.
    SplitPbf(SplitPbfArgs),
    /// Generates
    Osm2Streets(Osm2StreetsArgs),
    /// Convert osm2streets GeoJSON chunk outputs into Mapbox Vector Tiles.
    GeojsonToMvt(GeojsonToMvtArgs),
    /// Convert an MVT tile directory into a PMTiles archive.
    MvtToPmtiles(MvtToPmtilesArgs),
}

#[derive(Debug, Args, Clone)]
pub struct SplitPbfArgs {
    #[arg(long)]
    pub input_pbf: PathBuf,
    #[arg(long)]
    pub bounds: Bounds,
    #[arg(long, default_value_t = 13)]
    pub chunk_zoom: u8,
    #[arg(long)]
    pub output_dir: PathBuf,
    #[arg(long, default_value_t = 100.0)]
    pub bounds_buffer_meters: f64,
    #[arg(long, default_value = "osmium")]
    pub osmium_command: String,
}

#[derive(Debug, Args, Clone)]
pub struct Osm2StreetsArgs {
    #[arg(long)]
    pub input_path: PathBuf,
    #[arg(long)]
    pub output_dir: PathBuf,
}

#[derive(Debug, Args, Clone)]
pub struct GeojsonToMvtArgs {
    #[arg(long)]
    pub input_dir: PathBuf,
    #[arg(long)]
    pub output_dir: PathBuf,
    #[arg(long)]
    pub zoom: u8,
    /// Vector tile extent. Higher values reduce integer coordinate quantization.
    #[arg(long, default_value_t = DEFAULT_MVT_EXTENT)]
    pub extent: u16,
    /// Vector tile buffer in extent units. Keeps seam geometry without unbounded coordinates.
    #[arg(long, default_value_t = DEFAULT_MVT_BUFFER)]
    pub buffer: u16,
    /// GeoJSON-VT simplification tolerance in extent units. Defaults to 0 to preserve lane geometry.
    #[arg(long, default_value_t = DEFAULT_MVT_TOLERANCE)]
    pub tolerance: f64,
}

#[derive(Debug, Args, Clone)]
pub struct MvtToPmtilesArgs {
    #[arg(long)]
    pub input_dir: PathBuf,
    #[arg(long)]
    pub output_pmtiles: PathBuf,
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::SplitPbf(args) => {
            split_pbf(&args)?;
        }
        Command::Osm2Streets(args) => {
            osm2streets(&args)?;
        }
        Command::GeojsonToMvt(args) => {
            geojson_to_mvt(&args)?;
        }
        Command::MvtToPmtiles(args) => {
            mvt_to_pmtiles(&args)?;
        }
    }
    Ok(())
}
