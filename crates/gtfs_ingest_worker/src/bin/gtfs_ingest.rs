use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use gtfs_ingest_worker::model::SeedFile;
use gtfs_ingest_worker::postgres_gtfs::upsert_feed_sources_seed;
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::{EnvFilter, filter::LevelFilter};

#[derive(Debug, Parser)]
#[command(name = "gtfs_ingest")]
#[command(about = "GTFS schedule source seeding, ingestion, and PMTiles export")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    SeedSources(SeedSourcesArgs),
}

#[derive(Debug, Args, Clone)]
pub struct SeedSourcesArgs {
    #[arg(long, env = "POSTGRES_GTFS_URL")]
    pub database_url: String,

    #[arg(long)]
    pub seed_file: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let cli = Cli::parse();

    match cli.command {
        Command::SeedSources(args) => seed_sources(args).await,
    }
}

async fn seed_sources(args: SeedSourcesArgs) -> Result<()> {
    info!(
        "Seeding GTFS feed sources from seed file {}",
        args.seed_file.display()
    );

    let seed: SeedFile = serde_yaml::from_reader(
        std::fs::File::open(&args.seed_file)
            .with_context(|| format!("failed to open seed file {}", args.seed_file.display()))?,
    )
    .with_context(|| format!("failed to parse seed file {}", args.seed_file.display()))?;

    info!("Upserting GTFS feed sources");

    let pool = sqlx::PgPool::connect(&args.database_url).await?;

    upsert_feed_sources_seed(&pool, &seed)
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
