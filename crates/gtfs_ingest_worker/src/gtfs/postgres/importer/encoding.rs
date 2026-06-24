use crate::gtfs::postgres::importer::specs::{IMPORT_SPECS, ImportSpec};
use anyhow::{Context, bail};
use gtfs_structures::{
    Availability, DirectionType, Exception, LocationType, PickupDropOffType, RouteType,
    TimepointType,
};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{Read, Seek};
use std::path::Path;
use zip::ZipArchive;

#[derive(Debug, Clone)]
pub struct GtfsZipMember {
    index: usize,
}

pub struct GtfsZip<R>
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
    pub fn new(mut archive: ZipArchive<R>) -> anyhow::Result<Self> {
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

    pub fn contains(&self, spec: &ImportSpec) -> bool {
        self.members.contains_key(spec.file_name)
    }

    pub fn open_member(&mut self, spec: &ImportSpec) -> anyhow::Result<zip::read::ZipFile<'_, R>> {
        let member = self
            .members
            .get(spec.file_name)
            .with_context(|| format!("GTFS artifact is missing file {}", spec.file_name))?;

        self.archive
            .by_index(member.index)
            .with_context(|| format!("failed to open GTFS file {}", spec.file_name))
    }
}

// Value Transforms

pub fn format_gtfs_date(date: chrono::NaiveDate) -> String {
    date.format("%Y%m%d").to_string()
}

pub fn format_color(color: rgb::RGB8) -> String {
    format!("{:02X}{:02X}{:02X}", color.r, color.g, color.b)
}

pub fn location_type_code(value: LocationType) -> i32 {
    match value {
        LocationType::StopPoint => 0,
        LocationType::StopArea => 1,
        LocationType::StationEntrance => 2,
        LocationType::GenericNode => 3,
        LocationType::BoardingArea => 4,
        LocationType::Unknown(value) => i32::from(value),
    }
}

pub fn route_type_code(value: RouteType) -> i32 {
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

pub fn pickup_drop_off_code(value: PickupDropOffType) -> i16 {
    match value {
        PickupDropOffType::Regular => 0,
        PickupDropOffType::NotAvailable => 1,
        PickupDropOffType::ArrangeByPhone => 2,
        PickupDropOffType::CoordinateWithDriver => 3,
        PickupDropOffType::Unknown(value) => value,
    }
}

pub fn timepoint_code(value: TimepointType) -> i16 {
    match value {
        TimepointType::Approximate => 0,
        TimepointType::Exact => 1,
    }
}

pub fn availability_code(value: Availability) -> i32 {
    match value {
        Availability::InformationNotAvailable => 0,
        Availability::Available => 1,
        Availability::NotAvailable => 2,
        Availability::Unknown(value) => i32::from(value),
    }
}

pub fn exception_code(value: Exception) -> i32 {
    match value {
        Exception::Added => 1,
        Exception::Deleted => 2,
    }
}

pub fn direction_type_code(value: DirectionType) -> i32 {
    match value {
        DirectionType::Outbound => 0,
        DirectionType::Inbound => 1,
    }
}
