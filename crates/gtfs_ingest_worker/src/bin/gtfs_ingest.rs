use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use gtfs_ingest_worker::gtfs::{
    ArtifactStoreConfig, GtfsIngestClient, GtfsIngestConfig, export_tiling, sync_tiling,
    upsert_feed_sources_seed,
};
use gtfs_ingest_worker::model::SeedFile;
use std::path::PathBuf;
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
    #[arg(long, env = "POSTGRES_GTFS_URL")]
    pub database_url: String,

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
    #[arg(long, env = "POSTGRES_GTFS_URL")]
    pub database_url: String,

    #[arg(long, default_value_t = DEFAULT_SYNC_TILING_PARALLELISM)]
    pub parallelism: usize,
}

#[derive(Debug, Args, Clone)]
pub struct ExportTilingArgs {
    #[arg(long, env = "POSTGRES_GTFS_URL")]
    pub database_url: String,

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
    info!(
        seed_file = %args.seed_file.display(),
        "Seeding GTFS feed sources from seed file {}",
        args.seed_file.display()
    );

    let seed: SeedFile = serde_yaml::from_reader(
        std::fs::File::open(&args.seed_file)
            .with_context(|| format!("failed to open seed file {}", args.seed_file.display()))?,
    )
    .with_context(|| format!("failed to parse seed file {}", args.seed_file.display()))?;

    upsert_feed_sources_seed(&args.database_url, &seed, args.delete_existing)
        .await
        .with_context(|| {
            format!(
                "failed to upsert GTFS sources from seed file {}",
                args.seed_file.display()
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
    let outcome = sync_tiling(&args.database_url, args.parallelism)
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
    let outcome = export_tiling(
        &args.database_url,
        args.source_slug.as_deref(),
        &args.output_file,
        args.parallelism,
    )
    .await
    .context("failed to export GTFS tiling")?;

    info!(?outcome, "completed GTFS export-tiling command");
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
