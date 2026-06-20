use crate::model::{SeedFile, SeedSource};
use anyhow::{Context, Result};
use reqwest::header::{ACCEPT, USER_AGENT};
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};

const MOBILITY_DATABASE_CATALOG_URL: &str = "https://files.mobilitydatabase.org/feeds_v2.csv";
const TRANSITLAND_REST_FEEDS_URL: &str = "https://transit.land/api/v2/rest/feeds";

#[derive(Debug, Clone)]
pub struct TransitlandFeedFilters {
    pub api_key: String,
    pub bbox: Option<String>,
    pub limit: usize,
    pub max_pages: Option<usize>,
    pub fetch_error: Option<bool>,
    pub license_redistribution_allowed: Option<String>,
    pub license_commercial_use_allowed: Option<String>,
    pub require_static_current_url: bool,
    pub require_no_authorization: bool,
}

#[derive(Debug, Clone)]
pub struct MobilityDatabaseFeedFilters {
    pub catalog_url: String,
    pub country_codes: Vec<String>,
    pub statuses: Vec<String>,
    pub require_official: bool,
    pub require_no_authentication: bool,
    pub require_download_url: bool,
}

impl Default for MobilityDatabaseFeedFilters {
    fn default() -> Self {
        Self {
            catalog_url: MOBILITY_DATABASE_CATALOG_URL.to_owned(),
            country_codes: Vec::new(),
            statuses: Vec::new(),
            require_official: false,
            require_no_authentication: false,
            require_download_url: true,
        }
    }
}

pub async fn discover_seed_file(
    client: &reqwest::Client,
    transitland_filters: Option<&TransitlandFeedFilters>,
    mobility_database_filters: &MobilityDatabaseFeedFilters,
) -> Result<SeedFile> {
    let transitland_sources = match transitland_filters {
        Some(transitland_filters) => query_transitland_seed_sources(client, transitland_filters)
            .await
            .context("failed to discover Transitland GTFS feeds")?,
        None => Vec::new(),
    };
    let mobility_database_sources =
        query_mobility_database_seed_sources(client, mobility_database_filters)
            .await
            .context("failed to discover Mobility Database GTFS feeds")?;

    Ok(merge_seed_sources(
        transitland_sources
            .into_iter()
            .chain(mobility_database_sources),
    ))
}

pub async fn query_transitland_seed_sources(
    client: &reqwest::Client,
    filters: &TransitlandFeedFilters,
) -> Result<Vec<SeedSource>> {
    let mut sources = Vec::new();
    let mut after: Option<i64> = None;
    let mut page_count = 0_usize;

    loop {
        if let Some(max_pages) = filters.max_pages
            && page_count >= max_pages
        {
            break;
        }

        let mut request = client
            .get(TRANSITLAND_REST_FEEDS_URL)
            .header(ACCEPT, "application/json")
            .header(
                USER_AGENT,
                concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
            )
            .query(&[
                ("apikey", filters.api_key.as_str()),
                ("spec", "gtfs"),
                ("limit", &filters.limit.to_string()),
            ]);

        if let Some(after) = after {
            request = request.query(&[("after", after.to_string())]);
        }

        if let Some(bbox) = filters.bbox.as_deref() {
            request = request.query(&[("bbox", bbox)]);
        }

        if let Some(fetch_error) = filters.fetch_error {
            request = request.query(&[("fetch_error", fetch_error.to_string())]);
        }

        if let Some(value) = filters.license_redistribution_allowed.as_deref() {
            request = request.query(&[("license_redistribution_allowed", value)]);
        }

        if let Some(value) = filters.license_commercial_use_allowed.as_deref() {
            request = request.query(&[("license_commercial_use_allowed", value)]);
        }

        let response_text = request
            .send()
            .await
            .context("failed to request Transitland feeds")?
            .error_for_status()
            .context("Transitland feeds request failed")?
            .text()
            .await
            .context("failed to read Transitland feeds response")?;

        let response: TransitlandFeedsResponse =
            serde_json::from_str(&response_text).context("failed to parse Transitland feeds")?;

        let feed_count = response.feeds.len();
        sources.extend(
            response
                .feeds
                .into_iter()
                .filter_map(|feed| transitland_feed_to_seed_source(feed, filters)),
        );

        page_count += 1;
        after = response.meta.and_then(|meta| meta.after);

        if feed_count == 0 || after.is_none() {
            break;
        }
    }

    Ok(sources)
}

pub async fn query_mobility_database_seed_sources(
    client: &reqwest::Client,
    filters: &MobilityDatabaseFeedFilters,
) -> Result<Vec<SeedSource>> {
    let body = client
        .get(&filters.catalog_url)
        .header(ACCEPT, "text/csv")
        .header(
            USER_AGENT,
            concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await
        .with_context(|| format!("failed to request {}", filters.catalog_url))?
        .error_for_status()
        .with_context(|| {
            format!(
                "Mobility Database request failed for {}",
                filters.catalog_url
            )
        })?
        .bytes()
        .await
        .context("failed to read Mobility Database catalog")?;

    let mut reader = csv::Reader::from_reader(body.as_ref());
    let mut sources = Vec::new();

    for row in reader.deserialize::<MobilityDatabaseCsvFeed>() {
        let row = row.context("failed to parse Mobility Database catalog row")?;
        if let Some(source) = mobility_database_feed_to_seed_source(row, filters) {
            sources.push(source);
        }
    }

    Ok(sources)
}

fn merge_seed_sources(sources: impl IntoIterator<Item = SeedSource>) -> SeedFile {
    let mut by_slug = BTreeMap::new();
    let mut seen_download_urls = HashSet::new();

    for source in sources {
        let normalized_url = source.direct_download_url.trim().to_owned();
        if !seen_download_urls.insert(normalized_url) {
            continue;
        }

        by_slug.insert(source.slug.clone(), source);
    }

    SeedFile {
        sources: by_slug.into_values().collect(),
    }
}

fn transitland_feed_to_seed_source(
    feed: TransitlandFeed,
    filters: &TransitlandFeedFilters,
) -> Option<SeedSource> {
    let urls = feed.urls?;
    let direct_download_url = urls.static_current?.trim().to_owned();
    if filters.require_static_current_url && direct_download_url.is_empty() {
        return None;
    }

    if filters.require_no_authorization && feed.authorization.and_then(|auth| auth.kind).is_some() {
        return None;
    }

    let slug = feed
        .onestop_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("transitland-{}", feed.id));

    let name = feed
        .name
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            feed.associated_operators
                .into_iter()
                .find_map(|operator| operator.name)
        })
        .unwrap_or_else(|| slug.clone());

    Some(SeedSource {
        source_url: Some(format!("https://www.transit.land/feeds/{slug}")),
        slug,
        name,
        direct_download_url,
        license_url: feed.license.and_then(|license| license.url),
        attribution: None,
    })
}

fn mobility_database_feed_to_seed_source(
    feed: MobilityDatabaseCsvFeed,
    filters: &MobilityDatabaseFeedFilters,
) -> Option<SeedSource> {
    if !eq_ignore_ascii_whitespace(feed.data_type.as_deref(), "gtfs") {
        return None;
    }

    if filters.require_official && !eq_ignore_ascii_whitespace(feed.is_official.as_deref(), "true")
    {
        return None;
    }

    if filters.require_no_authentication
        && !matches!(
            feed.authentication_type.as_deref().map(str::trim),
            None | Some("") | Some("0")
        )
    {
        return None;
    }

    if !filters.country_codes.is_empty() {
        let country_code = feed.country_code.as_deref()?.trim().to_ascii_uppercase();
        if !filters
            .country_codes
            .iter()
            .any(|value| value == &country_code)
        {
            return None;
        }
    }

    if !filters.statuses.is_empty() {
        let status = feed.status.as_deref()?.trim();
        if !filters
            .statuses
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(status))
        {
            return None;
        }
    }

    let direct_download_url = feed
        .direct_download_url
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            feed.latest_url
                .as_deref()
                .filter(|value| !value.trim().is_empty())
        })?
        .trim()
        .to_owned();

    if filters.require_download_url && direct_download_url.is_empty() {
        return None;
    }

    let slug = feed.id.trim().to_owned();
    if slug.is_empty() {
        return None;
    }

    let name = feed
        .name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            feed.provider
                .as_deref()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or(&slug)
        .trim()
        .to_owned();

    Some(SeedSource {
        source_url: Some(format!("https://mobilitydatabase.org/feeds/{slug}")),
        slug,
        name,
        direct_download_url,
        license_url: feed.license_url.filter(|value| !value.trim().is_empty()),
        attribution: feed.provider.filter(|value| !value.trim().is_empty()),
    })
}

fn eq_ignore_ascii_whitespace(value: Option<&str>, expected: &str) -> bool {
    value
        .map(str::trim)
        .is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

#[derive(Debug, Deserialize)]
struct TransitlandFeedsResponse {
    feeds: Vec<TransitlandFeed>,
    meta: Option<TransitlandMeta>,
}

#[derive(Debug, Deserialize)]
struct TransitlandMeta {
    after: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct TransitlandFeed {
    id: i64,
    name: Option<String>,
    onestop_id: Option<String>,
    urls: Option<TransitlandFeedUrls>,
    license: Option<TransitlandLicense>,
    authorization: Option<TransitlandAuthorization>,
    #[serde(default)]
    associated_operators: Vec<TransitlandOperator>,
}

#[derive(Debug, Deserialize)]
struct TransitlandFeedUrls {
    static_current: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TransitlandLicense {
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TransitlandAuthorization {
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TransitlandOperator {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MobilityDatabaseCsvFeed {
    id: String,
    data_type: Option<String>,
    #[serde(rename = "location.country_code")]
    country_code: Option<String>,
    provider: Option<String>,
    is_official: Option<String>,
    name: Option<String>,
    #[serde(rename = "urls.direct_download")]
    direct_download_url: Option<String>,
    #[serde(rename = "urls.authentication_type")]
    authentication_type: Option<String>,
    #[serde(rename = "urls.latest")]
    latest_url: Option<String>,
    #[serde(rename = "urls.license")]
    license_url: Option<String>,
    status: Option<String>,
}
