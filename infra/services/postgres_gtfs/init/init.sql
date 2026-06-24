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

    active_version_id   INTEGER,

    created_at          TIMESTAMPTZ NOT NULL,
    updated_at          TIMESTAMPTZ NOT NULL
);

CREATE TABLE gtfs_meta.feed_versions
(
    id                 SERIAL PRIMARY KEY,
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
    version_id      INTEGER NOT NULL REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    item_id         INTEGER NOT NULL,
    item_gtfs_id    TEXT,
    agency_name     TEXT,
    agency_url      TEXT,
    agency_timezone TEXT,
    agency_lang     TEXT,
    agency_phone    TEXT,
    PRIMARY KEY (version_id, item_id),
    UNIQUE (version_id, item_gtfs_id)
);

CREATE TABLE gtfs.stops
(
    version_id          INTEGER NOT NULL REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    item_id             INTEGER NOT NULL,
    item_gtfs_id        TEXT    NOT NULL,
    stop_code           TEXT,
    stop_name           TEXT,
    stop_desc           TEXT,
    stop_lat            DOUBLE PRECISION,
    stop_lon            DOUBLE PRECISION,
    zone_id             TEXT,
    stop_url            TEXT,
    location_type       INTEGER,
    parent_station_item_id INTEGER,
    wheelchair_boarding INTEGER,
    platform_code       TEXT,
    PRIMARY KEY (version_id, item_id),
    UNIQUE (version_id, item_gtfs_id)
);

CREATE INDEX gtfs_stops_version_parent_station_item_idx ON gtfs.stops (version_id, parent_station_item_id)
    WHERE parent_station_item_id IS NOT NULL;

CREATE TABLE gtfs.stop_route_refs
(
    version_id INTEGER NOT NULL REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    stop_item_id  INTEGER NOT NULL,
    route_item_id INTEGER NOT NULL,
    PRIMARY KEY (version_id, stop_item_id, route_item_id)
);

CREATE VIEW gtfs.stop_route_agg_refs AS
SELECT
    stop.version_id,
    stop.item_id AS stop_item_id,
    ARRAY(
        SELECT DISTINCT route_ref.route_item_id
        FROM (
            SELECT own_ref.route_item_id
            FROM gtfs.stop_route_refs own_ref
            WHERE own_ref.version_id = stop.version_id
              AND own_ref.stop_item_id = stop.item_id

            UNION

            SELECT child_ref.route_item_id
            FROM gtfs.stops child_stop
            JOIN gtfs.stop_route_refs child_ref
              ON child_ref.version_id = child_stop.version_id
             AND child_ref.stop_item_id = child_stop.item_id
            WHERE child_stop.version_id = stop.version_id
              AND child_stop.parent_station_item_id = stop.item_id
        ) route_ref
        ORDER BY route_ref.route_item_id
    ) AS route_item_ids
FROM gtfs.stops stop;

CREATE TABLE gtfs.routes
(
    version_id       INTEGER NOT NULL REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    item_id          INTEGER NOT NULL,
    item_gtfs_id     TEXT    NOT NULL,
    agency_item_id   INTEGER,
    route_short_name TEXT,
    route_long_name  TEXT,
    route_desc       TEXT,
    route_type       INTEGER,
    route_url        TEXT,
    route_color      TEXT,
    route_text_color TEXT,
    PRIMARY KEY (version_id, item_id),
    UNIQUE (version_id, item_gtfs_id)
);

CREATE TABLE gtfs.trips
(
    version_id    INTEGER NOT NULL REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    item_id       INTEGER NOT NULL,
    item_gtfs_id  TEXT   NOT NULL,
    route_item_id INTEGER,
    service_item_id INTEGER,
    trip_headsign TEXT,
    direction_id  INTEGER,
    block_id      TEXT,
    shape_item_id INTEGER,
    PRIMARY KEY (version_id, item_id),
    UNIQUE (version_id, item_gtfs_id)
);

CREATE TABLE gtfs.stop_times_seq
(
    version_id           INTEGER NOT NULL REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    trip_item_id         INTEGER NOT NULL,
    arrival_times        INTEGER[] NOT NULL,
    departure_times      INTEGER[] NOT NULL,
    stop_item_ids        INTEGER[] NOT NULL,
    stop_sequences       SMALLINT[] NOT NULL,
    pickup_types         SMALLINT[] NOT NULL,
    drop_off_types       SMALLINT[] NOT NULL,
    shape_dist_traveleds DOUBLE PRECISION[] NOT NULL,
    timepoints           SMALLINT[] NOT NULL,
    PRIMARY KEY (version_id, trip_item_id)
);

CREATE TABLE gtfs.shapes_seq
(
    version_id             INTEGER NOT NULL REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    shape_item_id          INTEGER NOT NULL,
    item_gtfs_id           TEXT    NOT NULL,
    point_count            INTEGER NOT NULL,
    geom                   geometry(LineString, 4326),
    shape_pt_sequences     INTEGER[] NOT NULL,
    shape_dist_traveleds   DOUBLE PRECISION[] NOT NULL,
    PRIMARY KEY (version_id, shape_item_id),
    UNIQUE (version_id, item_gtfs_id)
);

CREATE TABLE gtfs.calendar
(
    version_id   INTEGER NOT NULL REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    item_id      INTEGER NOT NULL,
    item_gtfs_id TEXT   NOT NULL,
    monday     BOOLEAN,
    tuesday    BOOLEAN,
    wednesday  BOOLEAN,
    thursday   BOOLEAN,
    friday     BOOLEAN,
    saturday   BOOLEAN,
    sunday     BOOLEAN,
    start_date TEXT,
    end_date   TEXT,
    PRIMARY KEY (version_id, item_id),
    UNIQUE (version_id, item_gtfs_id)
);

CREATE TABLE gtfs.calendar_dates
(
    version_id     INTEGER NOT NULL REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    service_item_id INTEGER NOT NULL,
    date           TEXT   NOT NULL,
    exception_type INTEGER,
    PRIMARY KEY (version_id, service_item_id, date)
);

CREATE TABLE gtfs.feed_info
(
    version_id          INTEGER NOT NULL REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    item_id             INTEGER NOT NULL,
    feed_publisher_name TEXT,
    feed_publisher_url  TEXT,
    feed_lang           TEXT,
    default_lang        TEXT,
    feed_start_date     TEXT,
    feed_end_date       TEXT,
    feed_version        TEXT,
    feed_contact_email  TEXT,
    feed_contact_url    TEXT,
    PRIMARY KEY (version_id, item_id)
);

--- Tiling Tables ---

CREATE TABLE gtfs_tiling.source_tilings
(
    version_id   INTEGER PRIMARY KEY REFERENCES gtfs_meta.feed_versions (id) ON DELETE CASCADE,
    generated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE gtfs_tiling.stop_points
(
    version_id   INTEGER               NOT NULL,
    feature_id   BIGINT                NOT NULL,
    stop_item_id INTEGER               NOT NULL,
    geom         geometry(Point, 4326) NOT NULL,

    PRIMARY KEY (version_id, feature_id),
    UNIQUE (version_id, stop_item_id),
    FOREIGN KEY (version_id)
        REFERENCES gtfs_tiling.source_tilings (version_id)
        ON DELETE CASCADE
);

CREATE INDEX gtfs_tiling_stop_points_geom_gix
    ON gtfs_tiling.stop_points USING GIST (geom);

CREATE TABLE gtfs_tiling.trip_lines
(
    version_id    INTEGER                    NOT NULL,
    feature_id    BIGINT                     NOT NULL,
    route_item_id INTEGER                    NOT NULL,
    geom          geometry(LineString, 4326) NOT NULL,

    PRIMARY KEY (version_id, feature_id),
    FOREIGN KEY (version_id)
        REFERENCES gtfs_tiling.source_tilings (version_id)
        ON DELETE CASCADE
);

CREATE INDEX gtfs_tiling_trip_lines_route_idx
    ON gtfs_tiling.trip_lines (version_id, route_item_id);

CREATE TABLE gtfs_tiling.trip_line_refs
(
    version_id           INTEGER NOT NULL,
    route_item_id        INTEGER NOT NULL,
    trip_item_id         INTEGER NOT NULL,
    trip_line_feature_id BIGINT  NOT NULL,

    PRIMARY KEY (version_id, trip_item_id),
    FOREIGN KEY (version_id)
        REFERENCES gtfs_tiling.source_tilings (version_id)
        ON DELETE CASCADE
);

CREATE INDEX gtfs_tiling_trip_line_refs_route_idx
    ON gtfs_tiling.trip_line_refs (version_id, route_item_id);

CREATE INDEX gtfs_tiling_trip_line_refs_line_idx
    ON gtfs_tiling.trip_line_refs (version_id, trip_line_feature_id);
