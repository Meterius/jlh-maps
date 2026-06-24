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
    pub id: i32,
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
    pub id: i32,
    pub content_sha256: String,
    pub http_etag: Option<String>,
    pub http_last_modified: Option<String>,
}

/// GTFS stop row with derived route ids, used by the read-only GTFS client.
#[derive(Debug, Clone, FromRow)]
pub struct FeedAggregatedStop {
    pub version_id: i32,
    pub stop_id: String,
    pub stop_code: Option<String>,
    pub stop_name: Option<String>,
    pub stop_desc: Option<String>,
    pub stop_lat: Option<f64>,
    pub stop_lon: Option<f64>,
    pub zone_id: Option<String>,
    pub stop_url: Option<String>,
    pub location_type: Option<i32>,
    pub parent_station: Option<String>,
    pub wheelchair_boarding: Option<i32>,
    pub platform_code: Option<String>,
    pub route_ids: Vec<String>,
    pub depth: i32,
}

/// GTFS route row used by the read-only GTFS client.
#[derive(Debug, Clone, FromRow)]
pub struct FeedRoute {
    pub version_id: i32,
    pub route_id: String,
    pub agency_id: Option<String>,
    pub route_short_name: Option<String>,
    pub route_long_name: Option<String>,
    pub route_desc: Option<String>,
    pub route_type: Option<i32>,
    pub route_url: Option<String>,
    pub route_color: Option<String>,
    pub route_text_color: Option<String>,
}
