use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    osm2streets_ingest::cli::run().await
}
