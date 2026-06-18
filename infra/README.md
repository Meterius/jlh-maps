# Infrastructure

Docker Compose stack for the map data services used by `jlh_maps`.
The base `compose.yaml` contains shared service definitions, common networks, and
common named volumes. Environment-specific overlays provide routing mode,
published ports, service config mounts, and production hardening.

Run commands from this `infra` directory unless otherwise noted.

## Services

Service-specific Dockerfiles, nginx configs, Postgres configs, and Traefik
dynamic routing files live under `services/`.

### Compose Files

| File | Purpose |
| --- | --- |
| `compose.yaml` | Shared service definitions, service network isolation, common artifact bind mounts, and common named volumes. Some service config mounts are intentionally supplied by overlays. |
| `compose.local.yaml` | Local Docker Desktop override with file-provider Traefik routing, local DNS, local host ports, and local-only public network access. |
| `compose.prod.mono.yaml` | Single-server production override with HTTPS Traefik routing under fixed service subdomains and hardened service settings. |
| `compose.jobs.yaml` | One-shot import jobs, such as loading OSM data into PostGIS. |

### Services

| Service | Purpose | Local endpoint | Data/setup dependency                                                                                                                                                                         |
| --- | --- | --- |-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `dnsmasq` | Local DNS forwarder. Resolves `${ROOT_DOMAIN}` and its subdomains to `${LOCAL_SERVER_IP}` for LAN testing. | DNS on `${LOCAL_SERVER_IP}:53` | Requires devices under test to use `${LOCAL_SERVER_IP}` as their DNS server.                                                                                                                  |
| `traefik` | Reverse proxy for the HTTP services in this stack. | `http://localhost:80`; dashboard on `http://localhost:8081` |                                                                                                                                                                                               |
| `postgres_osm` | PostGIS PostgreSQL database. Stores OSM data imported by the `postgres_osm_importer` job. | PostgreSQL on `localhost:5433` | Automatically initialized from `services/postgres_osm/init/init.sql`; persisted in the `postgres_osm_data` Docker volume.                                                                              |
| `postgres_gtfs` | PostGIS PostgreSQL database for GTFS feed-source metadata and future imported schedule data. | PostgreSQL on `localhost:5434` | Automatically initialized from `services/postgres_gtfs/init/init.sql`; persisted in the `postgres_gtfs_data` Docker volume. |
| `gtfs_artifact_store` | Garage S3-compatible object storage for immutable GTFS feed-version ZIP artifacts. | S3 API on `localhost:3900`; admin API on `localhost:3903` | Initialized as a single-node Garage deployment from `services/gtfs_artifact_store/garage.toml`; persisted in the `gtfs_artifact_store_meta` and `gtfs_artifact_store_data` Docker volumes. |
| `omt_tileserver_gl` | Serves OpenMapTiles vector tiles and styles through TileServer GL. | `http://tiles.jlh_maps.localhost` | Requires `${OPENMAPTILES_DIR}/data`, `${OPENMAPTILES_DIR}/style`, and `${OPENMAPTILES_DIR}/build`; Populated by output of https://github.com/Meterius/jlh-sys-design-playground-openmaptiles. |
| `static_tile_server` | Static nginx server for Sentinel-2 raster tiles and the local osm2streets PMTiles archive. | `http://static.jlh_maps.localhost/raster/sen2/tilejson.json` | Requires `${SAT_RASTER_TILE_JSON_DIR}` for Sentinel-2 raster tiles and `${OSM2STREETS_PMTILES_PATH}` for roads PMTiles. |
| `static_frontend` | Static nginx server for the built Vite frontend. | `http://localhost` or `http://${ROOT_DOMAIN}` | Requires `${JLH_MAPS_DIST_DIR}` to point at the built `packages/jlh_maps/dist` directory. |
| `core_service` | Rust API for looking up imported OSM element metadata from `postgres_osm`. | `http://api.jlh_maps.localhost` | Requires the `unitable` table produced by the OSM import job of `postgres_osm`.                                                                                                               |
| `valhalla` | Valhalla routing service backed by a prebuilt routing graph. | `http://valhalla.jlh_maps.localhost` | Requires generated Valhalla files under `valhalla/custom_files`.                                                                                                                              |

### `compose.jobs.yaml`

| Service/job | Purpose | Inputs                                                                     | Output                                                                                       |
| --- | --- |----------------------------------------------------------------------------|----------------------------------------------------------------------------------------------|
| `gtfs_ingest_seed_sources` | One-shot GTFS feed-source seed job. It upserts configured sources into `postgres_gtfs`. | `config/gtfs_feed_sources_seed.yaml` | Populates `gtfs_meta.feed_sources` in `postgres_gtfs`. |
| `gtfs_ingest_sync_sources` | One-shot GTFS source sync job. It downloads changed feed sources, uploads the ZIP artifact to Garage, imports the feed into `postgres_gtfs`, and promotes the newest imported version. | `gtfs_meta.feed_sources`; direct download URLs; `gtfs_artifact_store` | Populates `gtfs_meta.feed_versions` and `gtfs.*` schedule tables; stores immutable feed ZIPs under `s3://$GTFS_ARTIFACT_S3_BUCKET/feed-sources/...`. |
| `gtfs_ingest_sync_tiling` | One-shot GTFS tiling sync job. It refreshes persisted stop geometries for any source whose active version is not the currently tiled version. | `gtfs_meta.feed_sources.active_version_id`; `gtfs.stops` | Populates `gtfs_tiling.source_tilings` and `gtfs_tiling.stop_points`. |
| `postgres_osm_importer` | One-shot OSM import job. It runs `osm2pgsql` in flex mode and loads OSM data into `postgres_osm`. | `jobs/postgres_osm_importer/style.lua`; `https://download.geofabrik.de/europe` | Populates the `unitable` table for `postgres_osm`. |

Run the import job with the main stack file included so the `postgres_osm`
dependency is available:

```powershell
just run-job local postgres_osm_importer
```

Seed GTFS feed sources after starting or creating the base database service:

```powershell
just run-job local gtfs_ingest_seed_sources
```

Run the GTFS source sync after seeding sources:

```powershell
just run-job local gtfs_ingest_sync_sources
```

Regenerate GTFS tiling geometries after a source sync promotes a new active
version:

```powershell
just run-job local gtfs_ingest_sync_tiling
```

For manual retries, rerun the same sync job. To invoke the command explicitly
through Compose while keeping the target's env files and overlay stack, use
`exec`:

```powershell
just exec local -f compose.jobs.yaml run --rm gtfs_ingest_sync_sources sync-sources
```

`run-job` takes the same target names as `run`, so `local` includes
`.local.env` and `compose.local.yaml`, while `prod-mono` includes
`.prod.mono.env` and `compose.prod.mono.yaml`. Use the same target that was
used to start the stack so job dependencies, env files, and overlay services
match the active Compose shape.

`run` and `run-job` both accept optional arguments for their underlying Docker
Compose subcommand:

```powershell
just run prod-mono -d --build
just run-job local gtfs_ingest_seed_sources --no-deps
```

For arbitrary Docker Compose commands with the target's env files and Compose
file stack already inserted, use `exec`:

```powershell
just exec local ps
just exec local logs -f postgres_gtfs
just exec prod-mono config
```

Generate osm2streets PMTiles with `crates/osm2streets_ingest`, then set
`OSM2STREETS_PMTILES_PATH` to the generated `tiles.pmtiles` file.

## External Data And Setup

### Environment

Environment variables are consumed by the Compose files, not inherently by a
specific env file. The commands below load `.env` first and then the target
overlay env file, so overlay env files can override base values when local and
production paths or domains differ.

Variables used by `compose.yaml`:

```dotenv
POSTGRES_OSM_USER=...
POSTGRES_OSM_PASSWORD=...
POSTGRES_OSM_DB=...

POSTGRES_GTFS_USER=...
POSTGRES_GTFS_PASSWORD=...
POSTGRES_GTFS_DB=...

GTFS_ARTIFACT_STORE_RPC_SECRET=...
GTFS_ARTIFACT_STORE_ADMIN_TOKEN=...
GTFS_ARTIFACT_STORE_METRICS_TOKEN=...
GTFS_ARTIFACT_S3_BUCKET=...
GTFS_ARTIFACT_S3_ACCESS_KEY_ID=...
GTFS_ARTIFACT_S3_SECRET_ACCESS_KEY=...

OPENMAPTILES_DIR=...
SAT_RASTER_TILE_JSON_DIR=...
OSM2STREETS_PMTILES_PATH=...
JLH_MAPS_DIST_DIR=...
VALHALLA_CUSTOM_FILES_DIR=...
```

`OPENMAPTILES_DIR`, `SAT_RASTER_TILE_JSON_DIR`,
`OSM2STREETS_PMTILES_PATH`, and `VALHALLA_CUSTOM_FILES_DIR` point to data
prepared outside this repository or generated by one-shot jobs.
`JLH_MAPS_DIST_DIR` points to the built Vite app output. For Docker Desktop
use, they must be paths Docker can mount. `OSM2STREETS_PMTILES_PATH` is
required and must point to the generated roads PMTiles file.

The `GTFS_ARTIFACT_STORE_*` variables configure Garage's internal RPC/admin
secrets. Generate real production values outside the repository; the checked-in
Garage config contains only placeholders that are overridden by these
environment variables. `GTFS_ARTIFACT_S3_*` defines the default S3 bucket and
credentials Garage creates on first startup for GTFS feed artifacts.

Variables used by `compose.jobs.yaml`:

```dotenv
POSTGRES_OSM_PASSWORD=...
POSTGRES_OSM_DB=...
POSTGRES_OSM_USER=...

POSTGRES_GTFS_USER=...
POSTGRES_GTFS_PASSWORD=...
POSTGRES_GTFS_DB=...

GTFS_ARTIFACT_S3_BUCKET=...
GTFS_ARTIFACT_S3_ACCESS_KEY_ID=...
GTFS_ARTIFACT_S3_SECRET_ACCESS_KEY=...
```

`GTFS_ARTIFACT_S3_ENDPOINT` and `GTFS_ARTIFACT_S3_REGION` are intentionally
hard-coded in the GTFS sync job environment as
`http://gtfs_artifact_store:3900` and `garage`, because those are internal
Compose service details rather than deployment secrets.

Variables used by `compose.local.yaml`:

```dotenv
ROOT_DOMAIN=...
LOCAL_SERVER_IP=...
LOCAL_DNS_UPSTREAM=... # optional; defaults to 1.1.1.1
LOCAL_DNS_UPSTREAM_SECONDARY=... # optional; defaults to 9.9.9.9
```

Variables used by `compose.prod.mono.yaml`:

```dotenv
ROOT_DOMAIN=...
TRAEFIK_ACME_EMAIL=...
VALHALLA_SERVER_THREADS=... # optional; defaults to 2
```

The current local env files are loaded this way:

| Setup | Env files loaded |
| --- | --- |
| Local serving stack | `.env`, then `.local.env` |
| Production mono serving stack | `.env`, then `.prod.mono.env` |
| Local one-off jobs | `.env`, then `.local.env` |
| Production mono one-off jobs | `.env`, then `.prod.mono.env` |

These files are intentionally gitignored because they can contain local paths
and production secrets. The artifact path variables currently live in `.env`
because local and prod share the same mappings in this checkout; move any of
them into `.local.env` or `.prod.mono.env` if an environment needs a different
host path.

`ROOT_DOMAIN` is the local domain suffix to test through Traefik, for example
`jlh-maps.test`. `LOCAL_SERVER_IP` must be the LAN IP of the machine running
Docker, for example `192.168.1.50`; do not use `127.0.0.1` for phones or other
devices on the network.

`compose.local.yaml` starts `dnsmasq` on TCP/UDP port 53. It answers
`${ROOT_DOMAIN}` and all subdomains with `${LOCAL_SERVER_IP}` and forwards all
other DNS requests upstream. By default the upstream resolvers are `1.1.1.1`
and `9.9.9.9`; override them with `LOCAL_DNS_UPSTREAM` and
`LOCAL_DNS_UPSTREAM_SECONDARY` if needed.

With `ROOT_DOMAIN=jlh-maps.test`, local Traefik accepts both the existing
`.localhost` names and these LAN-test names:

| Host | Service |
| --- | --- |
| `jlh-maps.test` | `static_frontend` |
| `tiles.jlh-maps.test` | `omt_tileserver_gl` |
| `static.jlh-maps.test` | `static_tile_server` |
| `api.jlh-maps.test` | `core_service` |
| `valhalla.jlh-maps.test` | `valhalla` |

Point a device's DNS server at `${LOCAL_SERVER_IP}` to use those names from the
LAN. If port 53 is already occupied on the Docker host, stop the conflicting DNS
service or move this stack to a host where DNS port 53 can be published.

With `ROOT_DOMAIN=example.com`, `compose.prod.mono.yaml` routes these fixed
service subdomains through Traefik:

| Host | Service |
| --- | --- |
| `example.com` | `static_frontend` |
| `tiles.example.com` | `omt_tileserver_gl` |
| `static.example.com` | `static_tile_server` |
| `api.example.com` | `core_service` |
| `valhalla.example.com` | `valhalla` |

Create an `A`/`AAAA` record for the apex domain and explicit records for the
service subdomains, or use an apex record plus a wildcard record such as
`*.example.com`, pointing at the production server. Traefik then selects the
target service from the request `Host` header; no nginx routing layer is needed
for this subdomain layout.

Valhalla CORS is configured for `https://${ROOT_DOMAIN}` and
`https://maps.${ROOT_DOMAIN}`. If the frontend is hosted somewhere else, adjust
the `valhalla_cors` middleware in `services/traefik/prod-mono.dynamic.yaml`.

### OpenMapTiles Vector Tiles

`omt_tileserver_gl` does not build vector tiles. Prepare an OpenMapTiles output
directory externally, then point `OPENMAPTILES_DIR` at it.

Required layout:

```text
${OPENMAPTILES_DIR}/
  data/
  style/
    config.json
  build/
```

`style/config.json` must reference the MBTiles and style assets available in
the mounted `data`, `style`, and `build` directories.

`compose.yaml` mounts the same three directories from `OPENMAPTILES_DIR`:

- `${OPENMAPTILES_DIR}/data` mounted at `/data`
- `${OPENMAPTILES_DIR}/style` mounted at `/style`
- `${OPENMAPTILES_DIR}/build` mounted at `/build`

### Vite Frontend

`static_frontend` serves the static output of the Vite application. Build it before
starting the local stack if `${JLH_MAPS_DIST_DIR}` points at the default
`../packages/jlh_maps/dist` directory:

```powershell
Push-Location ..\packages\jlh_maps
npm run build:local
Pop-Location
```

For production, `${JLH_MAPS_DIST_DIR}` must contain the contents
of `packages/jlh_maps/dist` at the directory root. Build that bundle with:

```powershell
Push-Location ..\packages\jlh_maps
npm run build:prod
Pop-Location
```

### Static Tiles

`static_tile_server` only serves files. It currently exposes Sentinel-2 raster
tiles under `/raster/sen2/` and the osm2streets PMTiles archive at
`/roads/tiles.pmtiles`.

The base Compose file mounts the tile data, while overlays provide the nginx
server config:

- `services/static_tile_server/nginx.local.conf` for local routing/CORS.
- `services/static_tile_server/nginx.prod.conf` for prod routing, where CORS is
  handled by Traefik.

Generate the raster tiles before starting the service. See `crates/sat_ingest`
for an example of converting satellite imagery to the expected raster tile
format.

`${SAT_RASTER_TILE_JSON_DIR}` must contain
`tilejson.json` and the generated `{z}/{x}/{y}.png` tree.
`${OSM2STREETS_PMTILES_PATH}` must point to the generated PMTiles file.

### GTFS Ingestion

GTFS ingestion uses `postgres_gtfs` for metadata and imported schedule rows,
and `gtfs_artifact_store` for immutable feed ZIP artifacts. The worker treats
`gtfs::client` in `crates/gtfs_ingest_worker/src/gtfs/client.rs` as the public
API. The artifact store adapter and the Postgres `model`, `core`, and
`importer` modules are crate-private modules under `src/gtfs/`, so callers
cannot upload an artifact, mutate version state, or promote a feed by skipping
consistency checks.

The core version states are:

- `downloaded`: the feed ZIP was downloaded, hashed, uploaded to Garage, and a
  `gtfs_meta.feed_versions` row exists.
- `imported`: the version's GTFS files were copied directly into the durable
  `gtfs.*` tables with PostgreSQL binary `COPY` in one transaction.
- `active`: the version is the newest imported version for its source and
  `gtfs_meta.feed_sources.active_version_id` points to it.

If the worker exits during import, the transaction rolls back and another
worker can continue from `downloaded` or `import_failed`. The worker serializes
conflicting work with transaction-scoped Postgres advisory locks per feed source
and per feed version. There is no separate job-run table; the version status is
the recovery contract.

The current import path parses the feed with `gtfs-structures`, maps relevant
GTFS records to typed schema columns, and uses binary `COPY FROM STDIN` directly
against the durable GTFS tables. This handles vendor extra columns while
avoiding SQL-side string parsing, casts, and staging-table insert passes. The
current binary COPY buffer is in memory per GTFS file; if the full Germany feed
grows large enough to make memory pressure visible, the next refinement is to
stream encoded rows into the SQLx COPY sink in chunks.

`sync-sources` imports `stop_times.txt` as part of the fixed GTFS import path.

GTFS tiling is separate from feed ingestion. `sync-tiling` reads each source's
active version and compares it with `gtfs_tiling.source_tilings`. If the stored
tiling is stale, the worker deletes that source's previous tiling rows and
materializes the active version into `gtfs_tiling.stop_points` for stop POI
features.

The tiling transaction keeps at most one tiled version per feed source. If a
source has no active version, any stale tiling for that source is removed.
`source_tilings` only records which source version has materialized geometry;
feature counts are derived when syncing. MVT/PMTiles export is intentionally
not part of the current worker surface; only `sync-tiling` remains.

### OSM Data For PostGIS

The `postgres_osm_importer` job downloads data from Geofabrik and
imports it into `postgres_osm`. Edit the URL in `compose.jobs.yaml` if a
different extract is needed.

The job uses `jobs/postgres_osm_importer/style.lua`, which writes all OSM object
types into one `unitable` table with `attrs`, `tags`, and `geom` columns.
`core_service` depends on that table.

### Valhalla Routing Data

`valhalla/custom_files` is ignored by git and must be prepared externally. The
running service expects the files referenced by `valhalla/custom_files/valhalla.json`,
including:

- `berlin.osm.pbf`
- `valhalla.json`
- `valhalla_tiles/` or `valhalla_tiles.tar`
- `admins.sqlite`
- `timezones.sqlite`
- `default_speeds.json`

Generate these artifacts with Valhalla tooling for the same OSM extract you
want to route over, then place them under `infra/geo/valhalla/custom_files`
before starting the `valhalla` service.

`${VALHALLA_CUSTOM_FILES_DIR}` must contain these files at the directory root.
The local stack leaves the scripted Valhalla image entrypoint intact so it can
prepare or update files under `/custom_files`. The prod mono overlay runs
`valhalla_service` directly against the prebuilt
`${VALHALLA_CUSTOM_FILES_DIR}/valhalla.json` and mounts the directory read-only.

## Production Mono Paths

`compose.prod.mono.yaml` assumes one server running the Docker stack. The data
paths used by the base artifact variables must already exist and be populated
before starting the prod mono stack.

Traefik certificate state is the exception: it is stored in the
`traefik_letsencrypt` Compose-managed volume so ACME state survives container
recreation without requiring a host path.


## Running The Stack

After external data is in place:

```powershell
just run local
```

Equivalent Docker Compose command:

```powershell
docker compose --env-file .env --env-file .local.env -f compose.yaml -f compose.local.yaml up
```

For a single-server production stack:

```powershell
just run prod-mono
```

Equivalent Docker Compose command:

```powershell
docker compose --env-file .env --env-file .prod.mono.env -f compose.yaml -f compose.prod.mono.yaml up -d --build
```
