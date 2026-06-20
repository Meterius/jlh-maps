use anyhow::{Context, Result, bail};
use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::region::Region;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

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

    pub async fn get_feed_artifact_stream<W>(&self, key: &str, writer: &mut W) -> Result<()>
    where
        W: AsyncWrite + Send + Unpin,
    {
        let status_code = self
            .bucket
            .get_object_to_writer(key, writer)
            .await
            .with_context(|| format!("failed to stream GTFS artifact from s3://{}", key))?;

        if !(200..300).contains(&status_code) {
            bail!(
                "S3 download for GTFS artifact {} returned status {}",
                key,
                status_code
            );
        }

        writer
            .flush()
            .await
            .with_context(|| format!("failed to flush GTFS artifact stream from s3://{}", key))?;

        Ok(())
    }
}
