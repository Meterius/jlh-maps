use anyhow::{Context, Result, bail};
use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::region::Region;
use tokio::io::AsyncRead;

/// S3-like connection settings for the GTFS artifact object store.
#[derive(Debug, Clone)]
pub struct ArtifactStoreConfig {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
}

/// Artifact store for GTFS ZIP files.
#[derive(Debug, Clone)]
pub struct ArtifactStore {
    bucket: Box<Bucket>,
}

impl ArtifactStore {
    pub fn new(config: &ArtifactStoreConfig) -> Result<Self> {
        let region = Region::Custom {
            region: config.region.clone(),
            endpoint: config.endpoint.clone(),
        };

        let credentials = Credentials::new(
            Some(&config.access_key_id),
            Some(&config.secret_access_key),
            None,
            None,
            None,
        )
        .context("failed to build S3 credentials for GTFS artifact store")?;

        let bucket = Bucket::new(&config.bucket, region, credentials)
            .context("failed to create GTFS artifact bucket client")?
            .with_path_style();

        Ok(Self { bucket })
    }

    pub async fn put_feed_artifact_stream<R>(&self, key: &str, reader: &mut R) -> Result<()>
    where
        R: AsyncRead + Unpin,
    {
        let response = self
            .bucket
            .put_object_stream_with_content_type(reader, key, "application/zip")
            .await
            .with_context(|| format!("failed to stream GTFS artifact to s3://{}", key))?;

        if !(200..300).contains(&response.status_code()) {
            bail!(
                "S3 stream upload for GTFS artifact {} returned status {}",
                key,
                response.status_code()
            );
        }

        Ok(())
    }

    pub async fn get_feed_artifact(&self, key: &str) -> Result<Vec<u8>> {
        let response = self
            .bucket
            .get_object(key)
            .await
            .with_context(|| format!("failed to download GTFS artifact from s3://{}", key))?;

        if !(200..300).contains(&response.status_code()) {
            bail!(
                "S3 download for GTFS artifact {} returned status {}",
                key,
                response.status_code()
            );
        }

        Ok(response.to_vec())
    }
}
