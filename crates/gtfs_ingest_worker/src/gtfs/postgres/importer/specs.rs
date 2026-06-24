use super::GtfsZip;
use super::encoding::{
    availability_code, direction_type_code, exception_code, format_color, format_gtfs_date,
    location_type_code, pickup_drop_off_code, route_type_code, timepoint_code,
};
use super::translation::{TranslationKind, TranslationMaps};
use crate::gtfs::parser;
use crate::utils::postgres_binary_copy::BinaryCopyInWriter;
use anyhow::{Context, Result};
use gtfs_structures::{
    Agency, Calendar, CalendarDate, FeedInfo, RawStopTime, RawTrip, Route, Shape, Stop,
};
use sqlx::PgConnection;
use std::io::{Read, Seek};
use std::ops::DerefMut;

#[derive(Clone, Copy)]
pub struct ImportSpec {
    pub kind: ImportKind,
    pub name: &'static str,
    pub file_name: &'static str,
    pub required: bool,
    pub target_table: &'static str,
    pub columns: &'static [&'static str],
}

#[derive(Clone, Copy)]
pub enum ImportKind {
    Agency,
    Stops,
    Routes,
    Trips,
    StopTimes,
    ShapeItems,
    ShapePoints,
    Calendar,
    CalendarDates,
    FeedInfo,
}

pub const IMPORT_SPECS: &[ImportSpec] = &[
    AGENCY_IMPORT_SPEC,
    STOPS_IMPORT_SPEC,
    ROUTES_IMPORT_SPEC,
    CALENDAR_IMPORT_SPEC,
    CALENDAR_DATES_IMPORT_SPEC,
    SHAPE_ITEMS_IMPORT_SPEC,
    SHAPE_POINTS_IMPORT_SPEC,
    TRIPS_IMPORT_SPEC,
    STOP_TIMES_IMPORT_SPEC,
    FEED_INFO_IMPORT_SPEC,
];

pub async fn write_records<C, R>(
    writer: &mut BinaryCopyInWriter<C>,
    spec: &ImportSpec,
    gtfs_zip: &mut GtfsZip<R>,
    translations: &mut TranslationMaps,
    version_id: i32,
) -> Result<u64>
where
    C: DerefMut<Target = PgConnection>,
    R: Read + Seek,
{
    let reader = gtfs_zip.open_member(spec)?;

    match spec.kind {
        ImportKind::Agency => {
            write_agency_records(writer, reader, spec.file_name, translations, version_id).await
        }
        ImportKind::Stops => {
            write_stops_records(writer, reader, spec.file_name, translations, version_id).await
        }
        ImportKind::Routes => {
            write_routes_records(writer, reader, spec.file_name, translations, version_id).await
        }
        ImportKind::Trips => {
            write_trips_records(writer, reader, spec.file_name, translations, version_id).await
        }
        ImportKind::StopTimes => {
            write_stop_times_records(writer, reader, spec.file_name, translations, version_id).await
        }
        ImportKind::ShapeItems => {
            write_shape_item_records(writer, reader, spec.file_name, translations, version_id).await
        }
        ImportKind::ShapePoints => {
            write_shape_point_records(writer, reader, spec.file_name, translations, version_id)
                .await
        }
        ImportKind::Calendar => {
            write_calendar_records(writer, reader, spec.file_name, translations, version_id).await
        }
        ImportKind::CalendarDates => {
            write_calendar_dates_records(writer, reader, spec.file_name, translations, version_id)
                .await
        }
        ImportKind::FeedInfo => {
            write_feed_info_records(writer, reader, spec.file_name, translations, version_id).await
        }
    }
}

// Spec Implementations

pub const AGENCY_IMPORT_SPEC: ImportSpec = ImportSpec {
    kind: ImportKind::Agency,
    name: "agency",
    file_name: "agency.txt",
    required: true,
    target_table: "gtfs.agency",
    columns: AGENCY_COLUMNS,
};

const AGENCY_COLUMNS: &[&str] = &[
    "version_id",
    "item_id",
    "item_gtfs_id",
    "agency_name",
    "agency_url",
    "agency_timezone",
    "agency_lang",
    "agency_phone",
];

pub async fn write_agency_records<C>(
    writer: &mut BinaryCopyInWriter<C>,
    reader: impl Read,
    file_name: &str,
    translations: &mut TranslationMaps,
    version_id: i32,
) -> Result<u64>
where
    C: DerefMut<Target = PgConnection>,
{
    let start_row_count = writer.row_count();

    for agency in parser::parse_csv::<_, Agency>(reader, file_name)? {
        let agency = agency?;
        let item_id = agency
            .id
            .as_deref()
            .map(|id| translations.get_or_insert(TranslationKind::Agency, id))
            .unwrap_or_else(|| translations.allocate_item_id());
        writer
            .write_row((
                version_id,
                item_id,
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

pub const STOPS_IMPORT_SPEC: ImportSpec = ImportSpec {
    kind: ImportKind::Stops,
    name: "stops",
    file_name: "stops.txt",
    required: true,
    target_table: "gtfs.stops",
    columns: STOPS_COLUMNS,
};

const STOPS_COLUMNS: &[&str] = &[
    "version_id",
    "item_id",
    "item_gtfs_id",
    "stop_code",
    "stop_name",
    "stop_desc",
    "stop_lat",
    "stop_lon",
    "zone_id",
    "stop_url",
    "location_type",
    "parent_station_item_id",
    "wheelchair_boarding",
    "platform_code",
];

pub async fn write_stops_records<C>(
    writer: &mut BinaryCopyInWriter<C>,
    reader: impl Read,
    file_name: &str,
    translations: &mut TranslationMaps,
    version_id: i32,
) -> Result<u64>
where
    C: DerefMut<Target = PgConnection>,
{
    let start_row_count = writer.row_count();

    for stop in parser::parse_csv::<_, Stop>(reader, file_name)? {
        let stop = stop?;
        let item_id = translations.get_or_insert(TranslationKind::Stop, &stop.id);
        let parent_station_item_id =
            translations.optional_reference(TranslationKind::Stop, stop.parent_station.as_deref());
        writer
            .write_row((
                version_id,
                item_id,
                &stop.id,
                stop.code.as_deref(),
                stop.name.as_deref(),
                stop.description.as_deref(),
                stop.latitude,
                stop.longitude,
                stop.zone_id.as_deref(),
                stop.url.as_deref(),
                location_type_code(stop.location_type),
                parent_station_item_id,
                availability_code(stop.wheelchair_boarding),
                stop.platform_code.as_deref(),
            ))
            .await?;
    }

    Ok(writer.row_count() - start_row_count)
}

pub const ROUTES_IMPORT_SPEC: ImportSpec = ImportSpec {
    kind: ImportKind::Routes,
    name: "routes",
    file_name: "routes.txt",
    required: true,
    target_table: "gtfs.routes",
    columns: ROUTES_COLUMNS,
};

const ROUTES_COLUMNS: &[&str] = &[
    "version_id",
    "item_id",
    "item_gtfs_id",
    "agency_item_id",
    "route_short_name",
    "route_long_name",
    "route_desc",
    "route_type",
    "route_url",
    "route_color",
    "route_text_color",
];

pub async fn write_routes_records<C>(
    writer: &mut BinaryCopyInWriter<C>,
    reader: impl Read,
    file_name: &str,
    translations: &mut TranslationMaps,
    version_id: i32,
) -> Result<u64>
where
    C: DerefMut<Target = PgConnection>,
{
    let start_row_count = writer.row_count();

    for route in parser::parse_csv::<_, Route>(reader, file_name)? {
        let route = route?;
        let item_id = translations.get_or_insert(TranslationKind::Route, &route.id);
        let agency_item_id =
            translations.optional_reference(TranslationKind::Agency, route.agency_id.as_deref());
        writer
            .write_row((
                version_id,
                item_id,
                &route.id,
                agency_item_id,
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

pub const TRIPS_IMPORT_SPEC: ImportSpec = ImportSpec {
    kind: ImportKind::Trips,
    name: "trips",
    file_name: "trips.txt",
    required: true,
    target_table: "gtfs.trips",
    columns: TRIPS_COLUMNS,
};

const TRIPS_COLUMNS: &[&str] = &[
    "version_id",
    "item_id",
    "item_gtfs_id",
    "route_item_id",
    "service_item_id",
    "trip_headsign",
    "direction_id",
    "block_id",
    "shape_item_id",
];

pub async fn write_trips_records<C>(
    writer: &mut BinaryCopyInWriter<C>,
    reader: impl Read,
    file_name: &str,
    translations: &mut TranslationMaps,
    version_id: i32,
) -> Result<u64>
where
    C: DerefMut<Target = PgConnection>,
{
    let start_row_count = writer.row_count();

    for trip in parser::parse_csv::<_, RawTrip>(reader, file_name)? {
        let trip = trip?;
        let item_id = translations.get_or_insert(TranslationKind::Trip, &trip.id);
        let route_item_id =
            translations.optional_reference(TranslationKind::Route, Some(trip.route_id.as_str()));
        let service_item_id = translations
            .optional_reference(TranslationKind::Service, Some(trip.service_id.as_str()));
        let shape_item_id =
            translations.optional_reference(TranslationKind::Shape, trip.shape_id.as_deref());
        writer
            .write_row((
                version_id,
                item_id,
                &trip.id,
                route_item_id,
                service_item_id,
                trip.trip_headsign.as_deref(),
                trip.direction_id.map(direction_type_code),
                trip.block_id.as_deref(),
                shape_item_id,
            ))
            .await?;
    }

    Ok(writer.row_count() - start_row_count)
}

pub const STOP_TIMES_IMPORT_SPEC: ImportSpec = ImportSpec {
    kind: ImportKind::StopTimes,
    name: "stop_times",
    file_name: "stop_times.txt",
    required: true,
    target_table: "gtfs.stop_times",
    columns: STOP_TIMES_COLUMNS,
};

const STOP_TIMES_COLUMNS: &[&str] = &[
    "version_id",
    "trip_item_id",
    "arrival_time",
    "departure_time",
    "stop_item_id",
    "stop_sequence",
    "pickup_type",
    "drop_off_type",
    "shape_dist_traveled",
    "timepoint",
];

pub async fn write_stop_times_records<C>(
    writer: &mut BinaryCopyInWriter<C>,
    reader: impl Read,
    file_name: &str,
    translations: &mut TranslationMaps,
    version_id: i32,
) -> Result<u64>
where
    C: DerefMut<Target = PgConnection>,
{
    let start_row_count = writer.row_count();

    for stop_time in parser::parse_csv::<_, RawStopTime>(reader, file_name)? {
        let stop_time = stop_time?;
        let trip_item_id =
            translations.get_or_insert(TranslationKind::Trip, stop_time.trip_id.as_str());
        let stop_item_id = translations
            .optional_reference(TranslationKind::Stop, Some(stop_time.stop_id.as_str()));
        let arrival_time = stop_time
            .arrival_time
            .map(i32::try_from)
            .transpose()
            .context("GTFS arrival_time exceeds Postgres INTEGER range")?;
        let departure_time = stop_time
            .departure_time
            .map(i32::try_from)
            .transpose()
            .context("GTFS departure_time exceeds Postgres INTEGER range")?;

        writer
            .write_row((
                version_id,
                trip_item_id,
                arrival_time,
                departure_time,
                stop_item_id,
                i16::try_from(stop_time.stop_sequence)
                    .context("GTFS stop_sequence exceeds Postgres SMALLINT range")?,
                pickup_drop_off_code(stop_time.pickup_type),
                pickup_drop_off_code(stop_time.drop_off_type),
                stop_time.shape_dist_traveled.map(f64::from),
                timepoint_code(stop_time.timepoint),
            ))
            .await?;
    }

    Ok(writer.row_count() - start_row_count)
}

pub const SHAPE_ITEMS_IMPORT_SPEC: ImportSpec = ImportSpec {
    kind: ImportKind::ShapeItems,
    name: "shapes",
    file_name: "shapes.txt",
    required: false,
    target_table: "gtfs.shapes",
    columns: SHAPES_COLUMNS,
};

const SHAPES_COLUMNS: &[&str] = &["version_id", "item_id", "item_gtfs_id"];

pub async fn write_shape_item_records<C>(
    writer: &mut BinaryCopyInWriter<C>,
    reader: impl Read,
    file_name: &str,
    translations: &mut TranslationMaps,
    version_id: i32,
) -> Result<u64>
where
    C: DerefMut<Target = PgConnection>,
{
    let start_row_count = writer.row_count();
    let mut seen_shape_ids = std::collections::HashSet::new();

    for shape in parser::parse_csv::<_, Shape>(reader, file_name)? {
        let shape = shape?;
        if !seen_shape_ids.insert(shape.id.clone()) {
            continue;
        }

        let item_id = translations.get_or_insert(TranslationKind::Shape, &shape.id);
        writer.write_row((version_id, item_id, &shape.id)).await?;
    }

    Ok(writer.row_count() - start_row_count)
}

pub const SHAPE_POINTS_IMPORT_SPEC: ImportSpec = ImportSpec {
    kind: ImportKind::ShapePoints,
    name: "shape_points",
    file_name: "shapes.txt",
    required: false,
    target_table: "gtfs.shape_points",
    columns: SHAPE_POINTS_COLUMNS,
};

const SHAPE_POINTS_COLUMNS: &[&str] = &[
    "version_id",
    "shape_item_id",
    "shape_pt_lat",
    "shape_pt_lon",
    "shape_pt_sequence",
    "shape_dist_traveled",
];

pub async fn write_shape_point_records<C>(
    writer: &mut BinaryCopyInWriter<C>,
    reader: impl Read,
    file_name: &str,
    translations: &mut TranslationMaps,
    version_id: i32,
) -> Result<u64>
where
    C: DerefMut<Target = PgConnection>,
{
    let start_row_count = writer.row_count();

    for shape in parser::parse_csv::<_, Shape>(reader, file_name)? {
        let shape = shape?;
        let shape_item_id = translations.get_or_insert(TranslationKind::Shape, &shape.id);
        writer
            .write_row((
                version_id,
                shape_item_id,
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

pub const CALENDAR_IMPORT_SPEC: ImportSpec = ImportSpec {
    kind: ImportKind::Calendar,
    name: "calendar",
    file_name: "calendar.txt",
    required: false,
    target_table: "gtfs.calendar",
    columns: CALENDAR_COLUMNS,
};

const CALENDAR_COLUMNS: &[&str] = &[
    "version_id",
    "item_id",
    "item_gtfs_id",
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

pub async fn write_calendar_records<C>(
    writer: &mut BinaryCopyInWriter<C>,
    reader: impl Read,
    file_name: &str,
    translations: &mut TranslationMaps,
    version_id: i32,
) -> Result<u64>
where
    C: DerefMut<Target = PgConnection>,
{
    let start_row_count = writer.row_count();

    for calendar in parser::parse_csv::<_, Calendar>(reader, file_name)? {
        let calendar = calendar?;
        let item_id = translations.get_or_insert(TranslationKind::Service, &calendar.id);
        writer
            .write_row((
                version_id,
                item_id,
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

pub const CALENDAR_DATES_IMPORT_SPEC: ImportSpec = ImportSpec {
    kind: ImportKind::CalendarDates,
    name: "calendar_dates",
    file_name: "calendar_dates.txt",
    required: false,
    target_table: "gtfs.calendar_dates",
    columns: CALENDAR_DATES_COLUMNS,
};

const CALENDAR_DATES_COLUMNS: &[&str] =
    &["version_id", "service_item_id", "date", "exception_type"];

pub async fn write_calendar_dates_records<C>(
    writer: &mut BinaryCopyInWriter<C>,
    reader: impl Read,
    file_name: &str,
    translations: &mut TranslationMaps,
    version_id: i32,
) -> Result<u64>
where
    C: DerefMut<Target = PgConnection>,
{
    let start_row_count = writer.row_count();

    for calendar_date in parser::parse_csv::<_, CalendarDate>(reader, file_name)? {
        let calendar_date = calendar_date?;
        let service_item_id =
            translations.get_or_insert(TranslationKind::Service, &calendar_date.service_id);
        writer
            .write_row((
                version_id,
                service_item_id,
                format_gtfs_date(calendar_date.date),
                exception_code(calendar_date.exception_type),
            ))
            .await?;
    }

    Ok(writer.row_count() - start_row_count)
}

pub const FEED_INFO_IMPORT_SPEC: ImportSpec = ImportSpec {
    kind: ImportKind::FeedInfo,
    name: "feed_info",
    file_name: "feed_info.txt",
    required: false,
    target_table: "gtfs.feed_info",
    columns: FEED_INFO_COLUMNS,
};

const FEED_INFO_COLUMNS: &[&str] = &[
    "version_id",
    "item_id",
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

pub async fn write_feed_info_records<C>(
    writer: &mut BinaryCopyInWriter<C>,
    reader: impl Read,
    file_name: &str,
    translations: &mut TranslationMaps,
    version_id: i32,
) -> Result<u64>
where
    C: DerefMut<Target = PgConnection>,
{
    let start_row_count = writer.row_count();

    for feed_info in parser::parse_csv::<_, FeedInfo>(reader, file_name)? {
        let feed_info = feed_info?;
        let item_id = translations.allocate_item_id();
        writer
            .write_row((
                version_id,
                item_id,
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
