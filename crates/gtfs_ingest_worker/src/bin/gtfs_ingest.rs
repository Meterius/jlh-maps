use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use gtfs_ingest_worker::gtfs::client::{ArtifactStoreConfig, GtfsIngestClient, GtfsIngestConfig};
use gtfs_ingest_worker::model::SeedFile;
use std::path::{Path, PathBuf};
use tracing::info;
use tracing_subscriber::{EnvFilter, filter::LevelFilter};

const DEFAULT_SYNC_SOURCES_PARALLELISM: usize = 8;
const DEFAULT_SYNC_TILING_PARALLELISM: usize = 8;
const DEFAULT_EXPORT_TILING_PARALLELISM: usize = 16;

#[derive(Debug, Parser)]
#[command(name = "gtfs_ingest")]
#[command(about = "GTFS schedule source seeding, syncing, and artifact import")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    SeedSources(SeedSourcesArgs),
    SyncSources(SyncSourcesArgs),
    SyncTiling(SyncTilingArgs),
    ExportTiling(ExportTilingArgs),
}

#[derive(Debug, Args, Clone)]
pub struct SeedSourcesArgs {
    #[command(flatten)]
    pub client: ClientArgs,

    #[arg(long)]
    pub seed_file: PathBuf,

    #[arg(long, default_value_t = false)]
    pub delete_existing: bool,
}

#[derive(Debug, Args, Clone)]
pub struct SyncSourcesArgs {
    #[command(flatten)]
    pub client: ClientArgs,

    #[arg(long, default_value_t = DEFAULT_SYNC_SOURCES_PARALLELISM)]
    pub parallelism: usize,
}

#[derive(Debug, Args, Clone)]
pub struct SyncTilingArgs {
    #[command(flatten)]
    pub client: ClientArgs,

    #[arg(long, default_value_t = DEFAULT_SYNC_TILING_PARALLELISM)]
    pub parallelism: usize,
}

#[derive(Debug, Args, Clone)]
pub struct ExportTilingArgs {
    #[command(flatten)]
    pub client: ClientArgs,

    #[arg(long)]
    pub source_slug: Option<String>,

    #[arg(long)]
    pub output_file: PathBuf,

    #[arg(long, default_value_t = DEFAULT_EXPORT_TILING_PARALLELISM)]
    pub parallelism: usize,
}

#[derive(Debug, Args, Clone)]
pub struct ClientArgs {
    #[arg(long, env = "POSTGRES_GTFS_URL")]
    pub database_url: String,

    #[command(flatten)]
    pub artifact_store: ArtifactStoreArgs,
}

#[derive(Debug, Args, Clone)]
pub struct ArtifactStoreArgs {
    #[arg(long, env = "GTFS_ARTIFACT_S3_ENDPOINT")]
    pub endpoint: String,

    #[arg(long, env = "GTFS_ARTIFACT_S3_REGION")]
    pub region: String,

    #[arg(long, env = "GTFS_ARTIFACT_S3_BUCKET")]
    pub bucket: String,

    #[arg(long, env = "GTFS_ARTIFACT_S3_ACCESS_KEY_ID")]
    pub access_key_id: String,

    #[arg(long, env = "GTFS_ARTIFACT_S3_SECRET_ACCESS_KEY")]
    pub secret_access_key: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let cli = Cli::parse();

    match cli.command {
        Command::SeedSources(args) => seed_sources(args).await,
        Command::SyncSources(args) => sync_sources(args).await,
        Command::SyncTiling(args) => sync_tiling_command(args).await,
        Command::ExportTiling(args) => export_tiling_command(args).await,
    }
}

async fn seed_sources(args: SeedSourcesArgs) -> Result<()> {
    let SeedSourcesArgs {
        client,
        seed_file,
        delete_existing,
    } = args;

    info!(
        seed_file = %seed_file.display(),
        "Seeding GTFS feed sources from seed file {}",
        seed_file.display()
    );

    let seed: SeedFile = serde_yaml::from_reader(
        std::fs::File::open(&seed_file)
            .with_context(|| format!("failed to open seed file {}", seed_file.display()))?,
    )
    .with_context(|| format!("failed to parse seed file {}", seed_file.display()))?;

    let client = connect_client(client).await?;
    client
        .upsert_feed_sources_seed(&seed, delete_existing)
        .await
        .with_context(|| {
            format!(
                "failed to upsert GTFS sources from seed file {}",
                seed_file.display()
            )
        })?;

    info!(
        "Completed upserting {} GTFS feed sources",
        seed.sources.len()
    );

    Ok(())
}

async fn sync_sources(args: SyncSourcesArgs) -> Result<()> {
    let client = connect_client(args.client).await?;
    let outcome = client
        .sync_sources(args.parallelism)
        .await
        .context("failed to sync GTFS sources")?;

    info!(
        source_count = outcome.total_count(),
        succeeded_count = outcome.succeeded.len(),
        failed_count = outcome.failed.len(),
        failures = ?outcome.failed,
        "completed GTFS sync-sources command"
    );

    if outcome.has_failures() {
        bail!(
            "GTFS sync-sources failed for {} of {} sources",
            outcome.failed.len(),
            outcome.total_count()
        );
    }

    Ok(())
}

async fn sync_tiling_command(args: SyncTilingArgs) -> Result<()> {
    let client = connect_client(args.client).await?;
    let outcome = client
        .sync_tiling(args.parallelism)
        .await
        .context("failed to sync GTFS tiling")?;

    info!(
        source_count = outcome.total_count(),
        succeeded_count = outcome.succeeded.len(),
        failed_count = outcome.failed.len(),
        failures = ?outcome.failed,
        "completed GTFS sync-tiling command"
    );

    if outcome.has_failures() {
        bail!(
            "GTFS sync-tiling failed for {} of {} sources",
            outcome.failed.len(),
            outcome.total_count()
        );
    }

    Ok(())
}

async fn export_tiling_command(args: ExportTilingArgs) -> Result<()> {
    let client = connect_client(args.client).await?;

    if let Some(parent) = args.output_file.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create GTFS PMTiles output directory {}",
                parent.display()
            )
        })?;
    }

    let temp_file = temp_output_path(&args.output_file)?;
    let file = std::fs::File::create(&temp_file).with_context(|| {
        format!(
            "failed to create temporary GTFS PMTiles file {}",
            temp_file.display()
        )
    })?;

    let outcome = match client
        .export_tiling(args.source_slug.as_deref(), file, args.parallelism)
        .await
    {
        Ok(outcome) => outcome,
        Err(error) => {
            let _ = std::fs::remove_file(&temp_file);
            return Err(error).context("failed to export GTFS tiling");
        }
    };

    replace_file(&temp_file, &args.output_file)?;

    info!(
        ?outcome,
        output_file = %args.output_file.display(),
        "completed GTFS export-tiling command"
    );
    Ok(())
}

async fn connect_client(args: ClientArgs) -> Result<GtfsIngestClient> {
    GtfsIngestClient::connect(GtfsIngestConfig {
        database_url: args.database_url,
        artifact_store: ArtifactStoreConfig {
            endpoint: args.artifact_store.endpoint,
            region: args.artifact_store.region,
            bucket: args.artifact_store.bucket,
            access_key_id: args.artifact_store.access_key_id,
            secret_access_key: args.artifact_store.secret_access_key,
        },
    })
    .await
}

fn init_tracing() {
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}

fn temp_output_path(output_file: &Path) -> Result<PathBuf> {
    let file_name = output_file
        .file_name()
        .with_context(|| {
            format!(
                "failed to get file name from output file path {}",
                output_file.display()
            )
        })?
        .to_string_lossy();

    Ok(output_file.with_file_name(format!(".{file_name}.tmp")))
}

fn replace_file(temp_file: &Path, output_file: &Path) -> Result<()> {
    match std::fs::rename(temp_file, output_file) {
        Ok(()) => Ok(()),
        Err(error) if output_file.exists() => {
            std::fs::remove_file(output_file).with_context(|| {
                format!("failed to remove previous file {}", output_file.display())
            })?;
            std::fs::rename(temp_file, output_file).with_context(|| {
                format!(
                    "failed to move file from {} to {} after removing previous output: {}",
                    temp_file.display(),
                    output_file.display(),
                    error
                )
            })
        }
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to move file from {} to {}",
                temp_file.display(),
                output_file.display()
            )
        }),
    }
}
