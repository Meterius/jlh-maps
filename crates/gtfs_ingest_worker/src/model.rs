use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SeedFile {
    pub sources: Vec<SeedSource>,
}

#[derive(Debug, Deserialize)]
pub struct SeedSource {
    pub slug: String,
    pub name: String,
    pub source_url: String,
    pub direct_download_url: Option<String>,
    pub license_url: Option<String>,
    pub attribution: Option<String>,
    pub fetch_enabled: Option<bool>,
}
