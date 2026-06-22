use crate::gtfs::postgres::{FeedAggregatedStop, FeedRoute, FeedVersionImportInfo};

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

#[derive(Debug, Clone)]
pub struct AggregatedStop {
    pub version_id: i64,
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
    pub children: Vec<AggregatedStop>,
}

impl AggregatedStop {
    pub(crate) fn from_postgres_rows(rows: Vec<FeedAggregatedStop>) -> Option<Self> {
        let root = rows.iter().find(|row| row.depth == 0)?;
        Some(Self::from_postgres_row(root, &rows))
    }

    fn from_postgres_row(row: &FeedAggregatedStop, rows: &[FeedAggregatedStop]) -> Self {
        let children = rows
            .iter()
            .filter(|candidate| {
                candidate.parent_station.as_deref() == Some(row.stop_id.as_str())
                    && candidate.depth > row.depth
            })
            .map(|child| Self::from_postgres_row(child, rows))
            .collect();

        Self {
            version_id: row.version_id,
            stop_id: row.stop_id.clone(),
            stop_code: row.stop_code.clone(),
            stop_name: row.stop_name.clone(),
            stop_desc: row.stop_desc.clone(),
            stop_lat: row.stop_lat,
            stop_lon: row.stop_lon,
            zone_id: row.zone_id.clone(),
            stop_url: row.stop_url.clone(),
            location_type: row.location_type,
            parent_station: row.parent_station.clone(),
            wheelchair_boarding: row.wheelchair_boarding,
            platform_code: row.platform_code.clone(),
            route_ids: row.route_ids.clone(),
            children,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Route {
    pub version_id: i64,
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

impl From<FeedRoute> for Route {
    fn from(record: FeedRoute) -> Self {
        Self {
            version_id: record.version_id,
            route_id: record.route_id,
            agency_id: record.agency_id,
            route_short_name: record.route_short_name,
            route_long_name: record.route_long_name,
            route_desc: record.route_desc,
            route_type: record.route_type,
            route_url: record.route_url,
            route_color: record.route_color,
            route_text_color: record.route_text_color,
        }
    }
}
