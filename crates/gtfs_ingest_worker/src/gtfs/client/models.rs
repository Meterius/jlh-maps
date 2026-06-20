use crate::gtfs::postgres::FeedVersionImportInfo;

/// Client-facing view of a GTFS feed version lifecycle row.
#[derive(Debug, Clone)]
pub struct FeedVersion {
    pub id: i64,
    pub source_id: i64,
    pub download_url: String,
    pub content_sha256: String,
    pub file_bytes: i64,
    pub file_path: String,
    pub status: String,
}

impl From<FeedVersionImportInfo> for FeedVersion {
    fn from(record: FeedVersionImportInfo) -> Self {
        Self {
            id: record.id,
            source_id: record.source_id,
            download_url: record.download_url,
            content_sha256: record.content_sha256,
            file_bytes: record.file_bytes,
            file_path: record.file_path,
            status: record.status,
        }
    }
}
