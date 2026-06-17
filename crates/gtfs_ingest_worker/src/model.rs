use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SeedFile {
    pub sources: Vec<SeedSource>,
}

#[derive(Debug, Deserialize)]
pub struct SeedSource {
    /// Stable source key used by CLI commands and artifact paths.
    pub slug: String,
    pub name: String,
    /// Website for the source, if available.
    pub source_url: Option<String>,
    /// Required direct URL for the feed source GTFS ZIP file.
    pub direct_download_url: String,
    pub license_url: Option<String>,
    pub attribution: Option<String>,
}
