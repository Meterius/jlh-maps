use sqlx::FromRow;

/// Feed source data used to download and create a new feed version
#[derive(Debug, Clone, FromRow)]
pub struct FeedSourceDownloadInfo {
    /// Stable database id used for version ownership and locking.
    pub id: i64,
    /// Human-readable source key used in artifact paths and CLI arguments.
    pub slug: String,
    /// Required direct GTFS ZIP URL used for downloads.
    pub direct_download_url: String,
}

/// Feed version data used to import and promote a feed version
#[derive(Debug, Clone, FromRow)]
pub struct FeedVersionInfo {
    pub id: i64,
    pub source_id: i64,
    pub download_url: String,
    /// SHA-256 hash of the immutable GTFS ZIP artifact.
    pub content_sha256: String,
    pub file_bytes: i64,
    /// Object-store key for the immutable GTFS ZIP artifact.
    pub file_path: String,
    /// Lifecycle state: downloaded, import_failed, imported, or active.
    pub status: String,
}
