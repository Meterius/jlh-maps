use crate::bounds::{Bounds, SlippyTile, tile_bounds, tiles_for_bounds};
use crate::cli::SplitPbfArgs;
use anyhow::{Context, Result, bail};
use osmpbf::{BlobDecode, BlobReader};
use rayon::iter::IntoParallelRefIterator;
use rayon::iter::ParallelIterator;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitPbfMetadata {
    pub chunk: SlippyTile,
    #[serde(default)]
    pub target_bounds: Option<Bounds>,
    #[serde(default)]
    pub extraction_bounds: Option<Bounds>,
    #[serde(default)]
    pub bounds_buffer_meters: Option<f64>,
}

impl SplitPbfMetadata {
    pub fn target_bounds(&self) -> Bounds {
        self.target_bounds
            .unwrap_or_else(|| tile_bounds(self.chunk))
    }

    pub fn extraction_bounds(&self) -> Bounds {
        self.extraction_bounds
            .unwrap_or_else(|| self.target_bounds())
    }

    pub fn bounds_buffer_meters(&self) -> f64 {
        self.bounds_buffer_meters.unwrap_or(0.0)
    }
}

pub fn split_pbf(args: &SplitPbfArgs) -> Result<()> {
    if !args.input_pbf.exists() {
        bail!("input PBF does not exist: {}", args.input_pbf.display());
    }
    if args.bounds_buffer_meters < 0.0 {
        bail!(
            "bounds buffer must be >= 0 meters; got {}",
            args.bounds_buffer_meters
        );
    }

    if let Some(source_bounds) = pbf_header_bounds(&args.input_pbf)?
        && !args.bounds.intersects(source_bounds)
    {
        bail!(
            "requested bounds {} do not intersect source PBF bounds {} for {}; bounds must be west,south,east,north. The command you showed looks like south,west,north,east; for that bbox use {},{},{},{}",
            args.bounds,
            source_bounds,
            args.input_pbf.display(),
            args.bounds.south,
            args.bounds.west,
            args.bounds.north,
            args.bounds.east
        );
    }

    fs::create_dir_all(&args.output_dir)
        .with_context(|| format!("failed to create {}", args.output_dir.display()))?;

    let chunks = tiles_for_bounds(args.bounds, args.chunk_zoom);

    for chunks_chunk in chunks.chunks(4) {
        chunks_chunk
            .par_iter()
            .map(|chunk| split_chunk(args, *chunk))
            .collect::<Result<()>>()?;
    }

    Ok(())
}

fn split_chunk(args: &SplitPbfArgs, chunk: SlippyTile) -> Result<()> {
    let chunk_out_path = args.output_dir.join(format!("{}.osm.pbf", chunk.id()));
    let chunk_meta_out_path = chunk_out_path.with_extension("pbf.meta.json");

    let output = osmium_command_for_chunk(args, chunk, &chunk_out_path)
        .output()
        .with_context(|| {
            format!(
                "failed to run {} for chunk {}",
                args.osmium_command,
                chunk.id()
            )
        })?;
    std::io::stderr()
        .write_all(&output.stderr)
        .context("failed to forward osmium stderr")?;

    if !output.status.success() {
        bail!(
            "osmium failed for chunk {} with exit code {:?}",
            chunk.id(),
            output.status.code()
        );
    }

    let target_bounds = tile_bounds(chunk);
    let extraction_bounds = target_bounds.expand_meters(args.bounds_buffer_meters);
    let metadata = serde_json::to_string(&SplitPbfMetadata {
        chunk,
        target_bounds: Some(target_bounds),
        extraction_bounds: Some(extraction_bounds),
        bounds_buffer_meters: Some(args.bounds_buffer_meters),
    })
    .with_context(|| format!("failed to serialize metadata for chunk {}", chunk.id()))?;
    fs::write(&chunk_meta_out_path, metadata)
        .with_context(|| format!("failed to write {}", chunk_meta_out_path.display()))?;

    Ok(())
}

fn pbf_header_bounds(path: &Path) -> Result<Option<Bounds>> {
    let reader = BlobReader::from_path(path)
        .with_context(|| format!("failed to open PBF header {}", path.display()))?;

    for blob in reader {
        let blob =
            blob.with_context(|| format!("failed to read PBF blob in {}", path.display()))?;
        if let BlobDecode::OsmHeader(header) = blob
            .decode()
            .with_context(|| format!("failed to decode PBF header in {}", path.display()))?
        {
            return header
                .bbox()
                .map(|bbox| {
                    Bounds {
                        west: bbox.left.min(bbox.right),
                        south: bbox.bottom.min(bbox.top),
                        east: bbox.left.max(bbox.right),
                        north: bbox.bottom.max(bbox.top),
                    }
                    .validate()
                })
                .transpose()
                .with_context(|| format!("invalid PBF header bounds in {}", path.display()));
        }
    }

    Ok(None)
}

pub fn osmium_command_for_chunk(
    args: &SplitPbfArgs,
    chunk: SlippyTile,
    output_path: &Path,
) -> Command {
    let extraction_bounds = tile_bounds(chunk).expand_meters(args.bounds_buffer_meters);
    let bbox = extraction_bounds.to_string();

    let mut command = Command::new(&args.osmium_command);

    command
        .args([
            "extract".into(),
            "--bbox".into(),
            bbox.into(),
            "--set-bounds".into(),
            args.input_pbf.as_os_str().to_owned(),
            "-o".into(),
            output_path.as_os_str().to_owned(),
            "--overwrite".into(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    command
}
