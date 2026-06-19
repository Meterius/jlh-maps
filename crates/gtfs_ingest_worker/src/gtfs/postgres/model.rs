use sqlx::FromRow;

/// Feed source data used to download and create a new feed version
#[derive(Debug, Clone, FromRow)]
pub struct FeedSourceDownloadInfo {
    pub id: i64,
    pub slug: String,
    pub direct_download_url: String,
}

/// Feed version data used to import and promote a feed version
#[derive(Debug, Clone, FromRow)]
pub struct FeedVersionImportInfo {
    pub id: i64,
    pub source_id: i64,
    pub download_url: String,
    pub content_sha256: String,
    pub file_bytes: i64,
    pub file_path: String,
    pub status: String,
}

/// Active version fields needed to make conditional feed download requests.
#[derive(Debug, Clone, FromRow)]
pub struct FeedVersionDownloadInfo {
    pub id: i64,
    pub content_sha256: String,
    pub http_etag: Option<String>,
    pub http_last_modified: Option<String>,
}
