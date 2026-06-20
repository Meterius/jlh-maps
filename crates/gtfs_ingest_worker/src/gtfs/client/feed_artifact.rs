use crate::gtfs::postgres;
use anyhow::Context;
use reqwest::StatusCode;
use reqwest::header::{
    ETAG, HeaderMap, HeaderName, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED, USER_AGENT,
};
use sha2::{Digest, Sha256};

pub struct DownloadedFeed {
    pub body: Vec<u8>,
    pub http_etag: Option<String>,
    pub http_last_modified: Option<String>,
}

pub enum DownloadFeedOutcome {
    NotModified,
    Downloaded(DownloadedFeed),
}

pub async fn download_feed(
    client: &reqwest::Client,
    download_url: &str,
    active_download_cache: Option<&postgres::FeedVersionDownloadInfo>,
) -> anyhow::Result<DownloadFeedOutcome> {
    let mut request = client.get(download_url).header(
        USER_AGENT,
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
    );

    if let Some(active_download_cache) = active_download_cache {
        if let Some(http_etag) = active_download_cache.http_etag.as_deref() {
            request = request.header(IF_NONE_MATCH, http_etag);
        }

        if let Some(http_last_modified) = active_download_cache.http_last_modified.as_deref() {
            request = request.header(IF_MODIFIED_SINCE, http_last_modified);
        }
    }

    let response = request
        .send()
        .await
        .with_context(|| format!("failed to request GTFS feed {}", download_url))?;

    if response.status() == StatusCode::NOT_MODIFIED {
        return Ok(DownloadFeedOutcome::NotModified);
    }

    let response = response
        .error_for_status()
        .with_context(|| format!("GTFS feed request failed for {}", download_url))?;

    let headers = response.headers().clone();

    let body = response
        .bytes()
        .await
        .with_context(|| format!("failed to read GTFS feed {}", download_url))?;

    Ok(DownloadFeedOutcome::Downloaded(DownloadedFeed {
        body: body.to_vec(),
        http_etag: header_to_string(&headers, ETAG),
        http_last_modified: header_to_string(&headers, LAST_MODIFIED),
    }))
}

fn header_to_string(headers: &HeaderMap, header_name: HeaderName) -> Option<String> {
    headers
        .get(header_name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

pub fn sha256_hex(body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body);
    format!("{:x}", hasher.finalize())
}

pub fn feed_artifact_path(source_slug: &str, content_sha256: &str) -> String {
    format!("feed-sources/{source_slug}/versions/{content_sha256}.zip")
}
