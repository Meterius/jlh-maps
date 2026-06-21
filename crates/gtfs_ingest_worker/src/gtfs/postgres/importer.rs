use crate::gtfs::parser;
use crate::utils::postgres_binary_copy::BinaryCopyInWriter;
use anyhow::{Context, Result, bail};
use gtfs_structures::{
    Agency, Availability, Calendar, CalendarDate, DirectionType, Exception, FeedInfo, LocationType,
    PickupDropOffType, RawStopTime, RawTrip, Route, RouteType, Shape, Stop, TimepointType,
};
use sqlx::{PgConnection, Postgres, Transaction};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{Read, Seek};
use std::ops::DerefMut;
use std::path::Path;
use tracing::info;
use zip::ZipArchive;

/// Parses a GTFS ZIP and replaces rows for the given version inside the caller's transaction.
pub async fn import_feed_version<R>(
    tx: &mut Transaction<'_, Postgres>,
    version_id: i64,
    zip_archive: ZipArchive<R>,
) -> Result<()>
where
    R: Read + Seek,
{
    let mut gtfs_zip = GtfsZip::new(zip_archive).context("failed to inspect GTFS ZIP")?;

    delete_existing_derived_gtfs_rows(tx, version_id).await?;
    delete_existing_gtfs_rows(tx, version_id).await?;
    copy_gtfs_to_tables(tx, &mut gtfs_zip, version_id).await?;
    update_derived_gtfs_tables(tx, version_id).await?;

    Ok(())
}

// Parsing

#[derive(Debug, Clone)]
struct GtfsZipMember {
    index: usize,
}

struct GtfsZip<R>
where
    R: Read + Seek,
{
    archive: ZipArchive<R>,
    members: HashMap<&'static str, GtfsZipMember>,
}

impl<R> GtfsZip<R>
where
    R: Read + Seek,
{
    fn new(mut archive: ZipArchive<R>) -> Result<Self> {
        let mut members = HashMap::new();

        for index in 0..archive.len() {
            let file = archive
                .by_index(index)
                .with_context(|| format!("failed to read GTFS ZIP member metadata at {index}"))?;
            let Some(file_name) = Path::new(file.name()).file_name() else {
                continue;
            };

            for spec in IMPORT_SPECS {
                if file_name == OsStr::new(spec.file_name) {
                    members.insert(spec.file_name, GtfsZipMember { index });
                    break;
                }
            }
        }

        for spec in IMPORT_SPECS.iter().filter(|spec| spec.required) {
            if !members.contains_key(spec.file_name) {
                bail!("GTFS artifact is missing required file {}", spec.file_name);
            }
        }

        Ok(Self { archive, members })
    }

    fn contains(&self, spec: &ImportSpec) -> bool {
        self.members.contains_key(spec.file_name)
    }

    fn open_member(&mut self, spec: &ImportSpec) -> Result<zip::read::ZipFile<'_, R>> {
        let member = self
            .members
            .get(spec.file_name)
            .with_context(|| format!("GTFS artifact is missing file {}", spec.file_name))?;

        self.archive
            .by_index(member.index)
            .with_context(|| format!("failed to open GTFS file {}", spec.file_name))
    }
}

// Copy / Mutators

struct ImportSpec {
    kind: ImportKind,
    name: &'static str,
    file_name: &'static str,
    required: bool,
    target_table: &'static str,
    columns: &'static [&'static str],
}

#[derive(Clone, Copy)]
enum ImportKind {
    Agency,
    Stops,
    Routes,
    Trips,
    StopTimes,
    Shapes,
    Calendar,
    CalendarDates,
    FeedInfo,
}

async fn copy_gtfs_to_tables<R>(
    tx: &mut Transaction<'_, Postgres>,
    gtfs_zip: &mut GtfsZip<R>,
    version_id: i64,
) -> Result<()>
where
    R: Read + Seek,
{
    for spec in IMPORT_SPECS {
        copy_records_to_table(tx, spec, gtfs_zip, version_id).await?;
    }

    Ok(())
}

async fn copy_records_to_table<R>(
    tx: &mut Transaction<'_, Postgres>,
    spec: &ImportSpec,
    gtfs_zip: &mut GtfsZip<R>,
    version_id: i64,
) -> Result<()>
where
    R: Read + Seek,
{
    if !gtfs_zip.contains(spec) {
        if spec.required {
            bail!("GTFS artifact is missing required file {}", spec.file_name);
        }

        info!(
            version_id,
            target_table = spec.target_table,
            file_name = spec.file_name,
            rows = 0_u64,
            "skipped missing optional GTFS file"
        );
        return Ok(());
    }

    let copy_sql = format!(
        "COPY {} ({}) FROM STDIN WITH (FORMAT binary)",
        spec.target_table,
        spec.columns.join(", ")
    );

    let copy = tx
        .copy_in_raw(&copy_sql)
        .await
        .with_context(|| format!("failed to start COPY for {}", spec.target_table))?;

    let mut writer = BinaryCopyInWriter::new(copy, spec.columns.len())
        .with_context(|| format!("failed to create binary COPY writer for {}", spec.name))?;

    if let Err(error) = write_records(&mut writer, spec, gtfs_zip, version_id).await {
        let abort_message = error.to_string();
        let _ = writer.abort(abort_message).await;
        return Err(error).with_context(|| {
            format!(
                "failed to stream binary COPY rows for {}",
                spec.target_table
            )
        });
    }

    let row_count = writer.row_count();
    let copied_rows = writer
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
        version_id,
        target_table = spec.target_table,
        rows = copied_rows,
        "copied GTFS rows to durable table"
    );

    Ok(())
}

async fn write_records<C, R>(
    writer: &mut BinaryCopyInWriter<C>,
    spec: &ImportSpec,
    gtfs_zip: &mut GtfsZip<R>,
    version_id: i64,
) -> Result<u64>
where
    C: DerefMut<Target = PgConnection>,
    R: Read + Seek,
{
    let reader = gtfs_zip.open_member(spec)?;

    match spec.kind {
        ImportKind::Agency => {
            write_agency_records(writer, reader, spec.file_name, version_id).await
        }
        ImportKind::Stops => write_stops_records(writer, reader, spec.file_name, version_id).await,
        ImportKind::Routes => {
            write_routes_records(writer, reader, spec.file_name, version_id).await
        }
        ImportKind::Trips => write_trips_records(writer, reader, spec.file_name, version_id).await,
        ImportKind::StopTimes => {
            write_stop_times_records(writer, reader, spec.file_name, version_id).await
        }
        ImportKind::Shapes => {
            write_shapes_records(writer, reader, spec.file_name, version_id).await
        }
        ImportKind::Calendar => {
            write_calendar_records(writer, reader, spec.file_name, version_id).await
        }
        ImportKind::CalendarDates => {
            write_calendar_dates_records(writer, reader, spec.file_name, version_id).await
        }
        ImportKind::FeedInfo => {
            write_feed_info_records(writer, reader, spec.file_name, version_id).await
        }
    }
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

async fn delete_existing_derived_gtfs_rows(
    tx: &mut Transaction<'_, Postgres>,
    version_id: i64,
) -> Result<()> {
    sqlx::query("DELETE FROM gtfs.stop_route_refs WHERE version_id = $1")
        .bind(version_id)
        .execute(&mut **tx)
        .await
        .context("failed to clear previous rows from gtfs.stop_route_refs")?;

    Ok(())
}

async fn update_derived_gtfs_tables(
    tx: &mut Transaction<'_, Postgres>,
    version_id: i64,
) -> Result<()> {
    let rows = sqlx::query(
        r#"
        INSERT INTO gtfs.stop_route_refs (
            version_id,
            stop_id,
            route_id
        )
        SELECT DISTINCT
            stop_time.version_id,
            stop_time.stop_id,
            trip.route_id
        FROM gtfs.stop_times stop_time
        JOIN gtfs.trips trip
          ON trip.version_id = stop_time.version_id
         AND trip.trip_id = stop_time.trip_id
        WHERE stop_time.version_id = $1
          AND stop_time.stop_id IS NOT NULL
          AND trip.route_id IS NOT NULL
        "#,
    )
    .bind(version_id)
    .execute(&mut **tx)
    .await
    .context("failed to update derived GTFS stop-trip references")?
    .rows_affected();

    info!(
        version_id,
        target_table = "gtfs.stop_route_refs",
        rows,
        "updated derived GTFS table"
    );

    Ok(())
}

// Copy Specifications

const IMPORT_SPECS: &[ImportSpec] = &[
    ImportSpec {
        kind: ImportKind::Agency,
        name: "agency",
        file_name: "agency.txt",
        required: true,
        target_table: "gtfs.agency",
        columns: AGENCY_COLUMNS,
    },
    ImportSpec {
        kind: ImportKind::Stops,
        name: "stops",
        file_name: "stops.txt",
        required: true,
        target_table: "gtfs.stops",
        columns: STOPS_COLUMNS,
    },
    ImportSpec {
        kind: ImportKind::Routes,
        name: "routes",
        file_name: "routes.txt",
        required: true,
        target_table: "gtfs.routes",
        columns: ROUTES_COLUMNS,
    },
    ImportSpec {
        kind: ImportKind::Trips,
        name: "trips",
        file_name: "trips.txt",
        required: true,
        target_table: "gtfs.trips",
        columns: TRIPS_COLUMNS,
    },
    ImportSpec {
        kind: ImportKind::StopTimes,
        name: "stop_times",
        file_name: "stop_times.txt",
        required: true,
        target_table: "gtfs.stop_times",
        columns: STOP_TIMES_COLUMNS,
    },
    ImportSpec {
        kind: ImportKind::Shapes,
        name: "shapes",
        file_name: "shapes.txt",
        required: false,
        target_table: "gtfs.shapes",
        columns: SHAPES_COLUMNS,
    },
    ImportSpec {
        kind: ImportKind::Calendar,
        name: "calendar",
        file_name: "calendar.txt",
        required: false,
        target_table: "gtfs.calendar",
        columns: CALENDAR_COLUMNS,
    },
    ImportSpec {
        kind: ImportKind::CalendarDates,
        name: "calendar_dates",
        file_name: "calendar_dates.txt",
        required: false,
        target_table: "gtfs.calendar_dates",
        columns: CALENDAR_DATES_COLUMNS,
    },
    ImportSpec {
        kind: ImportKind::FeedInfo,
        name: "feed_info",
        file_name: "feed_info.txt",
        required: false,
        target_table: "gtfs.feed_info",
        columns: FEED_INFO_COLUMNS,
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

async fn write_agency_records<C>(
    writer: &mut BinaryCopyInWriter<C>,
    reader: impl Read,
    file_name: &str,
    version_id: i64,
) -> Result<u64>
where
    C: DerefMut<Target = PgConnection>,
{
    let start_row_count = writer.row_count();

    for agency in parser::parse_csv::<_, Agency>(reader, file_name)? {
        let agency = agency?;
        writer
            .write_row((
                version_id,
                agency.id.as_deref(),
                &agency.name,
                &agency.url,
                &agency.timezone,
                agency.lang.as_deref(),
                agency.phone.as_deref(),
            ))
            .await?;
    }

    Ok(writer.row_count() - start_row_count)
}

async fn write_stops_records<C>(
    writer: &mut BinaryCopyInWriter<C>,
    reader: impl Read,
    file_name: &str,
    version_id: i64,
) -> Result<u64>
where
    C: DerefMut<Target = PgConnection>,
{
    let start_row_count = writer.row_count();

    for stop in parser::parse_csv::<_, Stop>(reader, file_name)? {
        let stop = stop?;
        writer
            .write_row((
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
            ))
            .await?;
    }

    Ok(writer.row_count() - start_row_count)
}

async fn write_routes_records<C>(
    writer: &mut BinaryCopyInWriter<C>,
    reader: impl Read,
    file_name: &str,
    version_id: i64,
) -> Result<u64>
where
    C: DerefMut<Target = PgConnection>,
{
    let start_row_count = writer.row_count();

    for route in parser::parse_csv::<_, Route>(reader, file_name)? {
        let route = route?;
        writer
            .write_row((
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
            ))
            .await?;
    }

    Ok(writer.row_count() - start_row_count)
}

async fn write_trips_records<C>(
    writer: &mut BinaryCopyInWriter<C>,
    reader: impl Read,
    file_name: &str,
    version_id: i64,
) -> Result<u64>
where
    C: DerefMut<Target = PgConnection>,
{
    let start_row_count = writer.row_count();

    for trip in parser::parse_csv::<_, RawTrip>(reader, file_name)? {
        let trip = trip?;
        writer
            .write_row((
                version_id,
                &trip.route_id,
                &trip.service_id,
                &trip.id,
                trip.trip_headsign.as_deref(),
                trip.direction_id.map(direction_type_code),
                trip.block_id.as_deref(),
                trip.shape_id.as_deref(),
            ))
            .await?;
    }

    Ok(writer.row_count() - start_row_count)
}

async fn write_stop_times_records<C>(
    writer: &mut BinaryCopyInWriter<C>,
    reader: impl Read,
    file_name: &str,
    version_id: i64,
) -> Result<u64>
where
    C: DerefMut<Target = PgConnection>,
{
    let start_row_count = writer.row_count();

    for stop_time in parser::parse_csv::<_, RawStopTime>(reader, file_name)? {
        let stop_time = stop_time?;
        writer
            .write_row((
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
            ))
            .await?;
    }

    Ok(writer.row_count() - start_row_count)
}

async fn write_shapes_records<C>(
    writer: &mut BinaryCopyInWriter<C>,
    reader: impl Read,
    file_name: &str,
    version_id: i64,
) -> Result<u64>
where
    C: DerefMut<Target = PgConnection>,
{
    let start_row_count = writer.row_count();

    for shape in parser::parse_csv::<_, Shape>(reader, file_name)? {
        let shape = shape?;
        writer
            .write_row((
                version_id,
                &shape.id,
                shape.latitude,
                shape.longitude,
                i32::try_from(shape.sequence)
                    .context("GTFS shape_pt_sequence exceeds Postgres INTEGER range")?,
                shape.dist_traveled.map(f64::from),
            ))
            .await?;
    }

    Ok(writer.row_count() - start_row_count)
}

async fn write_calendar_records<C>(
    writer: &mut BinaryCopyInWriter<C>,
    reader: impl Read,
    file_name: &str,
    version_id: i64,
) -> Result<u64>
where
    C: DerefMut<Target = PgConnection>,
{
    let start_row_count = writer.row_count();

    for calendar in parser::parse_csv::<_, Calendar>(reader, file_name)? {
        let calendar = calendar?;
        writer
            .write_row((
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
            ))
            .await?;
    }

    Ok(writer.row_count() - start_row_count)
}

async fn write_calendar_dates_records<C>(
    writer: &mut BinaryCopyInWriter<C>,
    reader: impl Read,
    file_name: &str,
    version_id: i64,
) -> Result<u64>
where
    C: DerefMut<Target = PgConnection>,
{
    let start_row_count = writer.row_count();

    for calendar_date in parser::parse_csv::<_, CalendarDate>(reader, file_name)? {
        let calendar_date = calendar_date?;
        writer
            .write_row((
                version_id,
                &calendar_date.service_id,
                format_gtfs_date(calendar_date.date),
                exception_code(calendar_date.exception_type),
            ))
            .await?;
    }

    Ok(writer.row_count() - start_row_count)
}

async fn write_feed_info_records<C>(
    writer: &mut BinaryCopyInWriter<C>,
    reader: impl Read,
    file_name: &str,
    version_id: i64,
) -> Result<u64>
where
    C: DerefMut<Target = PgConnection>,
{
    let start_row_count = writer.row_count();

    for feed_info in parser::parse_csv::<_, FeedInfo>(reader, file_name)? {
        let feed_info = feed_info?;
        writer
            .write_row((
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
            ))
            .await?;
    }

    Ok(writer.row_count() - start_row_count)
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
