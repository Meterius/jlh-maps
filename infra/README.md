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
| `omt_tileserver_gl` | Serves OpenMapTiles vector tiles and styles through TileServer GL. | `http://tiles.jlh_maps.localhost` | Requires `${OPENMAPTILES_DIR}/data`, `${OPENMAPTILES_DIR}/style`, and `${OPENMAPTILES_DIR}/build`; Populated by output of https://github.com/Meterius/jlh-sys-design-playground-openmaptiles. |
| `static_tile_server` | Static nginx server for Sentinel-2 raster tiles and the local osm2streets PMTiles archive. | `http://static.jlh_maps.localhost/raster/sen2/tilejson.json` | Requires `${SAT_RASTER_TILE_JSON_DIR}` for Sentinel-2 raster tiles and `${OSM2STREETS_PMTILES_PATH}` for the PMTiles file. See `crates/sat_ingest` for populating raster tiles from satellite imagery. |
| `static_frontend` | Static nginx server for the built Vite frontend. | `http://localhost` or `http://${ROOT_DOMAIN}` | Requires `${JLH_MAPS_DIST_DIR}` to point at the built `packages/jlh_maps/dist` directory. |
| `core_service` | Rust API for looking up imported OSM element metadata from `postgres_osm`. | `http://api.jlh_maps.localhost` | Requires the `unitable` table produced by the OSM import job of `postgres_osm`.                                                                                                               |
| `valhalla` | Valhalla routing service backed by a prebuilt routing graph. | `http://valhalla.jlh_maps.localhost` | Requires generated Valhalla files under `valhalla/custom_files`.                                                                                                                              |

### `compose.jobs.yaml`

| Service/job | Purpose | Inputs                                                                     | Output                                                                                       |
| --- | --- |----------------------------------------------------------------------------|----------------------------------------------------------------------------------------------|
| `gtfs_ingest_seed_sources` | One-shot GTFS feed-source seed job. It upserts configured sources into `postgres_gtfs`. | `config/gtfs_ingest_seed_sources.yaml` | Populates `gtfs_meta.feed_sources` in `postgres_gtfs`. |
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
required and must point to the generated PMTiles file.

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
