use crate::utils::postgres_binary_copy::{BinaryCopyInBuffer, BinaryCopyNull};
use anyhow::{Context, Result, bail};
use gtfs_structures::{
    Agency, Availability, Calendar, CalendarDate, DirectionType, Exception, FeedInfo, GtfsReader,
    LocationType, PickupDropOffType, RawStopTime, RawTrip, Route, RouteType, Shape, Stop,
    TimepointType,
};
use sqlx::{Postgres, Transaction};
use std::io::Cursor;
use tracing::info;

/// Parses a GTFS ZIP and replaces rows for the given version inside the caller's transaction.
pub async fn import_feed_version(
    tx: &mut Transaction<'_, Postgres>,
    version_id: i64,
    zip_body: Vec<u8>,
) -> Result<()> {
    let gtfs = GtfsReader::default()
        .trim_fields(false)
        .read_stop_times(true)
        .raw()
        .read_from_reader(Cursor::new(zip_body))
        .with_context(|| format!("failed to parse GTFS artifact for version {}", version_id))?;

    let parsed = ParsedGtfsFiles::from_raw(gtfs)?;

    delete_existing_gtfs_rows(tx, version_id).await?;
    copy_gtfs_to_tables(tx, &parsed, version_id).await?;

    Ok(())
}

// Parsing

struct ParsedGtfsFiles {
    agencies: Vec<Agency>,
    stops: Vec<Stop>,
    routes: Vec<Route>,
    trips: Vec<RawTrip>,
    stop_times: Vec<RawStopTime>,
    shapes: Vec<Shape>,
    calendar: Vec<Calendar>,
    calendar_dates: Vec<CalendarDate>,
    feed_info: Vec<FeedInfo>,
}

impl ParsedGtfsFiles {
    fn from_raw(gtfs: gtfs_structures::RawGtfs) -> Result<Self> {
        fn mandatory_file<T>(
            file_name: &str,
            result: Result<Vec<T>, gtfs_structures::Error>,
        ) -> Result<Vec<T>> {
            result.with_context(|| format!("failed to read required GTFS file {}", file_name))
        }

        fn optional_file<T>(
            file_name: &str,
            result: Option<Result<Vec<T>, gtfs_structures::Error>>,
        ) -> Result<Vec<T>> {
            match result {
                Some(result) => result
                    .with_context(|| format!("failed to read optional GTFS file {}", file_name)),
                None => Ok(Vec::new()),
            }
        }

        Ok(Self {
            agencies: mandatory_file("agency.txt", gtfs.agencies)?,
            stops: mandatory_file("stops.txt", gtfs.stops)?,
            routes: mandatory_file("routes.txt", gtfs.routes)?,
            trips: mandatory_file("trips.txt", gtfs.trips)?,
            stop_times: mandatory_file("stop_times.txt", gtfs.stop_times)?,
            shapes: optional_file("shapes.txt", gtfs.shapes)?,
            calendar: optional_file("calendar.txt", gtfs.calendar)?,
            calendar_dates: optional_file("calendar_dates.txt", gtfs.calendar_dates)?,
            feed_info: optional_file("feed_info.txt", gtfs.feed_info)?,
        })
    }
}

// Copy / Mutators

struct ImportSpec {
    name: &'static str,
    target_table: &'static str,
    columns: &'static [&'static str],
    build_copy_body: fn(&ParsedGtfsFiles, i64) -> Result<(Vec<u8>, u64)>,
}

async fn copy_gtfs_to_tables(
    tx: &mut Transaction<'_, Postgres>,
    parsed: &ParsedGtfsFiles,
    version_id: i64,
) -> Result<()> {
    for spec in IMPORT_SPECS {
        let copy_body = (spec.build_copy_body)(parsed, version_id)
            .with_context(|| format!("failed to build binary COPY body for {}", spec.name))?;
        copy_records_to_table(tx, spec, copy_body).await?;
    }

    Ok(())
}

async fn copy_records_to_table(
    tx: &mut Transaction<'_, Postgres>,
    spec: &ImportSpec,
    (copy_body, row_count): (Vec<u8>, u64),
) -> Result<()> {
    let copy_sql = format!(
        "COPY {} ({}) FROM STDIN WITH (FORMAT binary)",
        spec.target_table,
        spec.columns.join(", ")
    );

    let mut copy = tx
        .copy_in_raw(&copy_sql)
        .await
        .with_context(|| format!("failed to start COPY for {}", spec.target_table))?;

    if let Err(error) = copy.send(copy_body.as_slice()).await {
        let _ = copy.abort(error.to_string()).await;
        return Err(error)
            .with_context(|| format!("failed to stream COPY for {}", spec.target_table));
    }

    let copied_rows = copy
        .finish()
        .await
        .with_context(|| format!("failed to finish COPY for {}", spec.target_table))?;

    if copied_rows != row_count {
        bail!(
            "COPY row count mismatch for {}: parser produced {}, Postgres accepted {}",
            spec.target_table,
            row_count,
            copied_rows
        );
    }

    info!(
        target_table = spec.target_table,
        rows = copied_rows,
        "copied GTFS rows to durable table"
    );

    Ok(())
}

async fn delete_existing_gtfs_rows(
    tx: &mut Transaction<'_, Postgres>,
    version_id: i64,
) -> Result<()> {
    for table_name in IMPORT_SPECS.iter().map(|spec| spec.target_table) {
        sqlx::query(&format!("DELETE FROM {table_name} WHERE version_id = $1"))
            .bind(version_id)
            .execute(&mut **tx)
            .await
            .with_context(|| format!("failed to clear previous rows from {}", table_name))?;
    }

    Ok(())
}

// Copy Specifications

const IMPORT_SPECS: &[ImportSpec] = &[
    ImportSpec {
        name: "agency",
        target_table: "gtfs.agency",
        columns: AGENCY_COLUMNS,
        build_copy_body: build_agency_copy_body,
    },
    ImportSpec {
        name: "stops",
        target_table: "gtfs.stops",
        columns: STOPS_COLUMNS,
        build_copy_body: build_stops_copy_body,
    },
    ImportSpec {
        name: "routes",
        target_table: "gtfs.routes",
        columns: ROUTES_COLUMNS,
        build_copy_body: build_routes_copy_body,
    },
    ImportSpec {
        name: "trips",
        target_table: "gtfs.trips",
        columns: TRIPS_COLUMNS,
        build_copy_body: build_trips_copy_body,
    },
    ImportSpec {
        name: "stop_times",
        target_table: "gtfs.stop_times",
        columns: STOP_TIMES_COLUMNS,
        build_copy_body: build_stop_times_copy_body,
    },
    ImportSpec {
        name: "shapes",
        target_table: "gtfs.shapes",
        columns: SHAPES_COLUMNS,
        build_copy_body: build_shapes_copy_body,
    },
    ImportSpec {
        name: "calendar",
        target_table: "gtfs.calendar",
        columns: CALENDAR_COLUMNS,
        build_copy_body: build_calendar_copy_body,
    },
    ImportSpec {
        name: "calendar_dates",
        target_table: "gtfs.calendar_dates",
        columns: CALENDAR_DATES_COLUMNS,
        build_copy_body: build_calendar_dates_copy_body,
    },
    ImportSpec {
        name: "feed_info",
        target_table: "gtfs.feed_info",
        columns: FEED_INFO_COLUMNS,
        build_copy_body: build_feed_info_copy_body,
    },
];

const AGENCY_COLUMNS: &[&str] = &[
    "version_id",
    "agency_id",
    "agency_name",
    "agency_url",
    "agency_timezone",
    "agency_lang",
    "agency_phone",
];

const STOPS_COLUMNS: &[&str] = &[
    "version_id",
    "stop_id",
    "stop_code",
    "stop_name",
    "stop_desc",
    "stop_lat",
    "stop_lon",
    "zone_id",
    "stop_url",
    "location_type",
    "parent_station",
    "wheelchair_boarding",
    "platform_code",
    "geom",
];

const ROUTES_COLUMNS: &[&str] = &[
    "version_id",
    "route_id",
    "agency_id",
    "route_short_name",
    "route_long_name",
    "route_desc",
    "route_type",
    "route_url",
    "route_color",
    "route_text_color",
];

const TRIPS_COLUMNS: &[&str] = &[
    "version_id",
    "route_id",
    "service_id",
    "trip_id",
    "trip_headsign",
    "direction_id",
    "block_id",
    "shape_id",
];

const STOP_TIMES_COLUMNS: &[&str] = &[
    "version_id",
    "trip_id",
    "arrival_time",
    "departure_time",
    "stop_id",
    "stop_sequence",
    "pickup_type",
    "drop_off_type",
    "shape_dist_traveled",
    "timepoint",
];

const SHAPES_COLUMNS: &[&str] = &[
    "version_id",
    "shape_id",
    "shape_pt_lat",
    "shape_pt_lon",
    "shape_pt_sequence",
    "shape_dist_traveled",
    "geom",
];

const CALENDAR_COLUMNS: &[&str] = &[
    "version_id",
    "service_id",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
    "start_date",
    "end_date",
];

const CALENDAR_DATES_COLUMNS: &[&str] = &["version_id", "service_id", "date", "exception_type"];

const FEED_INFO_COLUMNS: &[&str] = &[
    "version_id",
    "feed_publisher_name",
    "feed_publisher_url",
    "feed_lang",
    "default_lang",
    "feed_start_date",
    "feed_end_date",
    "feed_version",
    "feed_contact_email",
    "feed_contact_url",
];

fn build_agency_copy_body(files: &ParsedGtfsFiles, version_id: i64) -> Result<(Vec<u8>, u64)> {
    let mut body = BinaryCopyInBuffer::new(AGENCY_COLUMNS.len())?;

    for agency in &files.agencies {
        body.write_row((
            version_id,
            agency.id.as_deref(),
            &agency.name,
            &agency.url,
            &agency.timezone,
            agency.lang.as_deref(),
            agency.phone.as_deref(),
        ))?;
    }

    Ok(body.finish())
}

fn build_stops_copy_body(files: &ParsedGtfsFiles, version_id: i64) -> Result<(Vec<u8>, u64)> {
    let mut body = BinaryCopyInBuffer::new(STOPS_COLUMNS.len())?;

    for stop in &files.stops {
        body.write_row((
            version_id,
            &stop.id,
            stop.code.as_deref(),
            stop.name.as_deref(),
            stop.description.as_deref(),
            stop.latitude,
            stop.longitude,
            stop.zone_id.as_deref(),
            stop.url.as_deref(),
            location_type_code(stop.location_type),
            stop.parent_station.as_deref(),
            availability_code(stop.wheelchair_boarding),
            stop.platform_code.as_deref(),
            BinaryCopyNull,
        ))?;
    }

    Ok(body.finish())
}

fn build_routes_copy_body(files: &ParsedGtfsFiles, version_id: i64) -> Result<(Vec<u8>, u64)> {
    let mut body = BinaryCopyInBuffer::new(ROUTES_COLUMNS.len())?;

    for route in &files.routes {
        body.write_row((
            version_id,
            &route.id,
            route.agency_id.as_deref(),
            route.short_name.as_deref(),
            route.long_name.as_deref(),
            route.desc.as_deref(),
            route_type_code(route.route_type),
            route.url.as_deref(),
            route.color.map(format_color),
            route.text_color.map(format_color),
        ))?;
    }

    Ok(body.finish())
}

fn build_trips_copy_body(files: &ParsedGtfsFiles, version_id: i64) -> Result<(Vec<u8>, u64)> {
    let mut body = BinaryCopyInBuffer::new(TRIPS_COLUMNS.len())?;

    for trip in &files.trips {
        body.write_row((
            version_id,
            &trip.route_id,
            &trip.service_id,
            &trip.id,
            trip.trip_headsign.as_deref(),
            trip.direction_id.map(direction_type_code),
            trip.block_id.as_deref(),
            trip.shape_id.as_deref(),
        ))?;
    }

    Ok(body.finish())
}

fn build_stop_times_copy_body(files: &ParsedGtfsFiles, version_id: i64) -> Result<(Vec<u8>, u64)> {
    let mut body = BinaryCopyInBuffer::new(STOP_TIMES_COLUMNS.len())?;

    for stop_time in &files.stop_times {
        body.write_row((
            version_id,
            &stop_time.trip_id,
            stop_time.arrival_time.map(format_gtfs_time),
            stop_time.departure_time.map(format_gtfs_time),
            &stop_time.stop_id,
            i32::try_from(stop_time.stop_sequence)
                .context("GTFS stop_sequence exceeds Postgres INTEGER range")?,
            pickup_drop_off_code(stop_time.pickup_type),
            pickup_drop_off_code(stop_time.drop_off_type),
            stop_time.shape_dist_traveled.map(f64::from),
            timepoint_code(stop_time.timepoint),
        ))?;
    }

    Ok(body.finish())
}

fn build_shapes_copy_body(files: &ParsedGtfsFiles, version_id: i64) -> Result<(Vec<u8>, u64)> {
    let mut body = BinaryCopyInBuffer::new(SHAPES_COLUMNS.len())?;

    for shape in &files.shapes {
        body.write_row((
            version_id,
            &shape.id,
            shape.latitude,
            shape.longitude,
            i32::try_from(shape.sequence)
                .context("GTFS shape_pt_sequence exceeds Postgres INTEGER range")?,
            shape.dist_traveled.map(f64::from),
            BinaryCopyNull,
        ))?;
    }

    Ok(body.finish())
}

fn build_calendar_copy_body(files: &ParsedGtfsFiles, version_id: i64) -> Result<(Vec<u8>, u64)> {
    let mut body = BinaryCopyInBuffer::new(CALENDAR_COLUMNS.len())?;

    for calendar in &files.calendar {
        body.write_row((
            version_id,
            &calendar.id,
            calendar.monday,
            calendar.tuesday,
            calendar.wednesday,
            calendar.thursday,
            calendar.friday,
            calendar.saturday,
            calendar.sunday,
            format_gtfs_date(calendar.start_date),
            format_gtfs_date(calendar.end_date),
        ))?;
    }

    Ok(body.finish())
}

fn build_calendar_dates_copy_body(
    files: &ParsedGtfsFiles,
    version_id: i64,
) -> Result<(Vec<u8>, u64)> {
    let mut body = BinaryCopyInBuffer::new(CALENDAR_DATES_COLUMNS.len())?;

    for calendar_date in &files.calendar_dates {
        body.write_row((
            version_id,
            &calendar_date.service_id,
            format_gtfs_date(calendar_date.date),
            exception_code(calendar_date.exception_type),
        ))?;
    }

    Ok(body.finish())
}

fn build_feed_info_copy_body(files: &ParsedGtfsFiles, version_id: i64) -> Result<(Vec<u8>, u64)> {
    let mut body = BinaryCopyInBuffer::new(FEED_INFO_COLUMNS.len())?;

    for feed_info in &files.feed_info {
        body.write_row((
            version_id,
            &feed_info.name,
            &feed_info.url,
            &feed_info.lang,
            feed_info.default_lang.as_deref(),
            feed_info.start_date.map(format_gtfs_date),
            feed_info.end_date.map(format_gtfs_date),
            feed_info.version.as_deref(),
            feed_info.contact_email.as_deref(),
            feed_info.contact_url.as_deref(),
        ))?;
    }

    Ok(body.finish())
}

// Value Encodings

fn format_gtfs_time(seconds: u32) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn format_gtfs_date(date: chrono::NaiveDate) -> String {
    date.format("%Y%m%d").to_string()
}

fn format_color(color: rgb::RGB8) -> String {
    format!("{:02X}{:02X}{:02X}", color.r, color.g, color.b)
}

fn location_type_code(value: LocationType) -> i32 {
    match value {
        LocationType::StopPoint => 0,
        LocationType::StopArea => 1,
        LocationType::StationEntrance => 2,
        LocationType::GenericNode => 3,
        LocationType::BoardingArea => 4,
        LocationType::Unknown(value) => i32::from(value),
    }
}

fn route_type_code(value: RouteType) -> i32 {
    match value {
        RouteType::Tramway => 0,
        RouteType::Subway => 1,
        RouteType::Rail => 2,
        RouteType::Bus => 3,
        RouteType::Ferry => 4,
        RouteType::CableCar => 5,
        RouteType::Gondola => 6,
        RouteType::Funicular => 7,
        RouteType::Coach => 200,
        RouteType::Air => 1100,
        RouteType::Taxi => 1500,
        RouteType::Other(value) => i32::from(value),
    }
}

fn pickup_drop_off_code(value: PickupDropOffType) -> i32 {
    match value {
        PickupDropOffType::Regular => 0,
        PickupDropOffType::NotAvailable => 1,
        PickupDropOffType::ArrangeByPhone => 2,
        PickupDropOffType::CoordinateWithDriver => 3,
        PickupDropOffType::Unknown(value) => i32::from(value),
    }
}

fn timepoint_code(value: TimepointType) -> i32 {
    match value {
        TimepointType::Approximate => 0,
        TimepointType::Exact => 1,
    }
}

fn availability_code(value: Availability) -> i32 {
    match value {
        Availability::InformationNotAvailable => 0,
        Availability::Available => 1,
        Availability::NotAvailable => 2,
        Availability::Unknown(value) => i32::from(value),
    }
}

fn exception_code(value: Exception) -> i32 {
    match value {
        Exception::Added => 1,
        Exception::Deleted => 2,
    }
}

fn direction_type_code(value: DirectionType) -> i32 {
    match value {
        DirectionType::Outbound => 0,
        DirectionType::Inbound => 1,
    }
}
