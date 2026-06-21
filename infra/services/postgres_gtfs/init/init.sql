CREATE EXTENSION IF NOT EXISTS postgis;

CREATE SCHEMA IF NOT EXISTS gtfs_meta;
CREATE SCHEMA IF NOT EXISTS gtfs;
CREATE SCHEMA IF NOT EXISTS gtfs_tiling;

--- Meta Tables ---

CREATE TABLE gtfs_meta.feed_sources
(
    id                  BIGSERIAL PRIMARY KEY,

    slug                TEXT        NOT NULL UNIQUE,
    name                TEXT        NOT NULL,

    source_url          TEXT,
    direct_download_url TEXT        NOT NULL,

    license_url         TEXT,
    attribution         TEXT,

    active_version_id   BIGINT,

    created_at          TIMESTAMPTZ NOT NULL,
    updated_at          TIMESTAMPTZ NOT NULL
);

CREATE TABLE gtfs_meta.feed_versions
(
    id                 BIGSERIAL PRIMARY KEY,
    source_id          BIGINT      NOT NULL REFERENCES gtfs_meta.feed_sources (id) ON DELETE CASCADE,

    download_url       TEXT        NOT NULL,

    content_sha256     CHAR(64)    NOT NULL,
    file_bytes         BIGINT      NOT NULL,
    file_path          TEXT        NOT NULL,

    http_etag          TEXT,
    http_last_modified TEXT,

    status             TEXT        NOT NULL CHECK (
        status IN (
                   'downloaded',
                   'import_failed',
                   'imported',
                   'active'
            )
        ),

    error_message      TEXT,

    fetched_at         TIMESTAMPTZ NOT NULL,
    imported_at        TIMESTAMPTZ,
    promoted_at        TIMESTAMPTZ
);

ALTER TABLE gtfs_meta.feed_sources
    ADD CONSTRAINT feed_sources_active_version_fk
        FOREIGN KEY (active_version_id)
            REFERENCES gtfs_meta.feed_versions (id);

--- Data Tables --

CREATE TABLE gtfs.agency
(
    version_id      BIGINT NOT NULL REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    agency_id       TEXT,
    agency_name     TEXT,
    agency_url      TEXT,
    agency_timezone TEXT,
    agency_lang     TEXT,
    agency_phone    TEXT
);

CREATE TABLE gtfs.stops
(
    version_id          BIGINT NOT NULL REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    stop_id             TEXT   NOT NULL,
    stop_code           TEXT,
    stop_name           TEXT,
    stop_desc           TEXT,
    stop_lat            DOUBLE PRECISION,
    stop_lon            DOUBLE PRECISION,
    zone_id             TEXT,
    stop_url            TEXT,
    location_type       INTEGER,
    parent_station      TEXT,
    wheelchair_boarding INTEGER,
    platform_code       TEXT,
    PRIMARY KEY (version_id, stop_id)
);

CREATE INDEX gtfs_stops_version_parent_station_idx ON gtfs.stops (version_id, parent_station)
    WHERE parent_station IS NOT NULL;

CREATE TABLE gtfs.stop_route_refs
(
    version_id BIGINT NOT NULL REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    stop_id    TEXT   NOT NULL,
    route_id   TEXT   NOT NULL,
    PRIMARY KEY (version_id, stop_id, route_id)
);

CREATE VIEW gtfs.stop_route_agg_refs AS
SELECT
    stop.version_id,
    stop.stop_id,
    ARRAY(
        SELECT DISTINCT route_ref.route_id
        FROM (
            SELECT own_ref.route_id
            FROM gtfs.stop_route_refs own_ref
            WHERE own_ref.version_id = stop.version_id
              AND own_ref.stop_id = stop.stop_id

            UNION

            SELECT child_ref.route_id
            FROM gtfs.stops child_stop
            JOIN gtfs.stop_route_refs child_ref
              ON child_ref.version_id = child_stop.version_id
             AND child_ref.stop_id = child_stop.stop_id
            WHERE child_stop.version_id = stop.version_id
              AND child_stop.parent_station = stop.stop_id
        ) route_ref
        ORDER BY route_ref.route_id
    ) AS route_ids
FROM gtfs.stops stop;

CREATE TABLE gtfs.routes
(
    version_id       BIGINT NOT NULL REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    route_id         TEXT   NOT NULL,
    agency_id        TEXT,
    route_short_name TEXT,
    route_long_name  TEXT,
    route_desc       TEXT,
    route_type       INTEGER,
    route_url        TEXT,
    route_color      TEXT,
    route_text_color TEXT,
    PRIMARY KEY (version_id, route_id)
);

CREATE TABLE gtfs.trips
(
    version_id    BIGINT NOT NULL REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    route_id      TEXT,
    service_id    TEXT,
    trip_id       TEXT   NOT NULL,
    trip_headsign TEXT,
    direction_id  INTEGER,
    block_id      TEXT,
    shape_id      TEXT,
    PRIMARY KEY (version_id, trip_id)
);

CREATE TABLE gtfs.stop_times
(
    version_id          BIGINT  NOT NULL REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    trip_id             TEXT    NOT NULL,
    arrival_time        TEXT,
    departure_time      TEXT,
    stop_id             TEXT,
    stop_sequence       INTEGER NOT NULL,
    pickup_type         INTEGER,
    drop_off_type       INTEGER,
    shape_dist_traveled DOUBLE PRECISION,
    timepoint           INTEGER,
    PRIMARY KEY (version_id, trip_id, stop_sequence)
);

CREATE INDEX gtfs_stop_times_version_stop_idx ON gtfs.stop_times (version_id, stop_id);

CREATE TABLE gtfs.shapes
(
    version_id          BIGINT  NOT NULL REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    shape_id            TEXT    NOT NULL,
    shape_pt_lat        DOUBLE PRECISION,
    shape_pt_lon        DOUBLE PRECISION,
    shape_pt_sequence   INTEGER NOT NULL,
    shape_dist_traveled DOUBLE PRECISION,
    PRIMARY KEY (version_id, shape_id, shape_pt_sequence)
);

CREATE TABLE gtfs.calendar
(
    version_id BIGINT NOT NULL REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    service_id TEXT   NOT NULL,
    monday     BOOLEAN,
    tuesday    BOOLEAN,
    wednesday  BOOLEAN,
    thursday   BOOLEAN,
    friday     BOOLEAN,
    saturday   BOOLEAN,
    sunday     BOOLEAN,
    start_date TEXT,
    end_date   TEXT,
    PRIMARY KEY (version_id, service_id)
);

CREATE TABLE gtfs.calendar_dates
(
    version_id     BIGINT NOT NULL REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    service_id     TEXT   NOT NULL,
    date           TEXT   NOT NULL,
    exception_type INTEGER,
    PRIMARY KEY (version_id, service_id, date)
);

CREATE TABLE gtfs.feed_info
(
    version_id          BIGINT NOT NULL REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    feed_publisher_name TEXT,
    feed_publisher_url  TEXT,
    feed_lang           TEXT,
    default_lang        TEXT,
    feed_start_date     TEXT,
    feed_end_date       TEXT,
    feed_version        TEXT,
    feed_contact_email  TEXT,
    feed_contact_url    TEXT
);

--- Tiling Tables ---

CREATE TABLE gtfs_tiling.source_tilings
(
    source_id    BIGINT PRIMARY KEY REFERENCES gtfs_meta.feed_sources (id) ON DELETE CASCADE,
    version_id   BIGINT      NOT NULL REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    generated_at TIMESTAMPTZ NOT NULL,

    UNIQUE (source_id, version_id)
);

CREATE TABLE gtfs_tiling.stop_points
(
    source_id  BIGINT                NOT NULL,
    version_id BIGINT                NOT NULL,
    stop_id    TEXT                  NOT NULL,
    geom       geometry(Point, 4326) NOT NULL,

    PRIMARY KEY (source_id, version_id, stop_id),
    FOREIGN KEY (source_id, version_id)
        REFERENCES gtfs_tiling.source_tilings (source_id, version_id)
        ON DELETE CASCADE
);

CREATE INDEX gtfs_tiling_stop_points_geom_gix
    ON gtfs_tiling.stop_points USING GIST (geom);
