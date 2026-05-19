# Infrastructure

Docker Compose stack for the map data services used by `jlh_maps`.
The base `compose.yaml` contains shared service definitions. Environment-specific
overlays provide routing, published ports, and data volumes.

Run commands from this `infra` directory unless otherwise noted.

## Services

### Compose Files

| File | Purpose |
| --- | --- |
| `compose.yaml` | Shared service definitions and common named volumes. Deployment-specific data mounts are intentionally omitted here. |
| `compose.local.yaml` | Local Docker Desktop override with `.localhost` and `ROOT_DOMAIN` Traefik routing, local DNS, local host ports, and host bind-mounted data paths. |
| `compose.prod.mono.yaml` | Single-server production override with HTTPS Traefik routing under fixed service subdomains and local Docker volumes for service data. |
| `compose.jobs.yaml` | One-shot import jobs, such as loading OSM data into PostGIS. |

### Services

| Service | Purpose | Local endpoint | Data/setup dependency                                                                                                                                                                         |
| --- | --- | --- |-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `dnsmasq` | Local DNS forwarder. Resolves `${ROOT_DOMAIN}` and its subdomains to `${LOCAL_SERVER_IP}` for LAN testing. | DNS on `${LOCAL_SERVER_IP}:53` | Requires devices under test to use `${LOCAL_SERVER_IP}` as their DNS server.                                                                                                                  |
| `traefik` | Reverse proxy for the HTTP services in this stack. | `http://localhost:80`; dashboard on `http://localhost:8081` |                                                                                                                                                                                               |
| `postgres_osm` | PostGIS PostgreSQL database. Stores OSM data imported by the `osm2pgsql_osm_import` job. | PostgreSQL on `localhost:5433` | Automatically initialized from `postgres_osm/init/init.sql`; persisted in the `postgres_osm_data` Docker volume.                                                                              |
| `omt_tileserver_gl` | Serves OpenMapTiles vector tiles and styles through TileServer GL. | `http://tiles.jlh_maps.localhost` | Requires `${OPENMAPTILES_DIR}/data`, `${OPENMAPTILES_DIR}/style`, and `${OPENMAPTILES_DIR}/build`; Populated by output of https://github.com/Meterius/jlh-sys-design-playground-openmaptiles. |
| `raster_tile_json_server` | Static nginx server for Sentinel-2 raster TileJSON and XYZ PNG tiles. | `http://raster.jlh_maps.localhost/raster/sen2/tilejson.json` | Requires `${SAT_RASTER_TILE_JSON_DIR}` to contain `tilejson.json` plus the generated `{z}/{x}/{y}.png` tile tree. See `crates/sat_ingest` for populating raster tiles from satellite imagery. |
| `core_service` | Rust API for looking up imported OSM element metadata from `postgres_osm`. | `http://api.jlh_maps.localhost` | Requires the `unitable` table produced by the OSM import job of `postgres_osm`.                                                                                                               |
| `valhalla` | Valhalla routing service backed by a prebuilt routing graph. | `http://valhalla.jlh_maps.localhost` | Requires generated Valhalla files under `valhalla/custom_files`.                                                                                                                              |

### `compose.jobs.yaml`

| Service/job | Purpose | Inputs                                                                     | Output                                                                                       |
| --- | --- |----------------------------------------------------------------------------|----------------------------------------------------------------------------------------------|
| `osm2pgsql_osm_import` | One-shot OSM import job. It runs `osm2pgsql` in flex mode and loads OSM data into `postgres_osm`. | `postgres_osm/osm2pgsql/style.lua`; `https://download.geofabrik.de/europe` | Populates the `unitable` table for `postgres_osm`. |

Run the import job with the main stack file included so the `postgres_osm`
dependency is available:

```powershell
docker compose --env-file .env -f compose.yaml -f compose.jobs.yaml run --rm osm2pgsql_osm_import
```

## External Data And Setup

### Environment

The environment files are split by scope:

| File | Used by | Purpose |
| --- | --- | --- |
| `.env` | All Compose commands | Shared database defaults used by the base stack and jobs. |
| `.local.env` | `compose.local.yaml` | Local host paths, local root domain, and Docker host LAN IP. |
| `.prod.mono.env` | `compose.prod.mono.yaml` only | Root domain, TLS email, production database credentials, and external Docker volume names. |

Review `.env` and the target overlay env file before starting a stack.
These files are intentionally gitignored because they can contain local paths
and production secrets.

Shared `.env`:

```dotenv
POSTGRES_OSM_USER=...
POSTGRES_OSM_PASSWORD=...
POSTGRES_OSM_DB=...
```

Local `.local.env`:

```dotenv
OPENMAPTILES_DIR=...
SAT_RASTER_TILE_JSON_DIR=...
VALHALLA_CUSTOM_FILES_DIR=...
ROOT_DOMAIN=...
LOCAL_SERVER_IP=...
```

`OPENMAPTILES_DIR`, `SAT_RASTER_TILE_JSON_DIR`, and
`VALHALLA_CUSTOM_FILES_DIR` point to data prepared outside this repository.
For local Docker Desktop use, they must be paths Docker can mount.

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
| `tiles.jlh-maps.test` | `omt_tileserver_gl` |
| `raster.jlh-maps.test` | `raster_tile_json_server` |
| `api.jlh-maps.test` | `core_service` |
| `valhalla.jlh-maps.test` | `valhalla` |

Point a device's DNS server at `${LOCAL_SERVER_IP}` to use those names from the
LAN. If port 53 is already occupied on the Docker host, stop the conflicting DNS
service or move this stack to a host where DNS port 53 can be published.

Production `.prod.mono.env` is only loaded by the prod mono recipe and should
be edited on the target server. It includes `ROOT_DOMAIN`, TLS email, and the
names of pre-provisioned Docker volumes.

Example production domain settings:

```dotenv
ROOT_DOMAIN=example.com
TRAEFIK_ACME_EMAIL=ops@example.com
```

With `ROOT_DOMAIN=example.com`, `compose.prod.mono.yaml` routes these fixed
service subdomains through Traefik:

| Host | Service |
| --- | --- |
| `tiles.example.com` | `omt_tileserver_gl` |
| `raster.example.com` | `raster_tile_json_server` |
| `api.example.com` | `core_service` |
| `valhalla.example.com` | `valhalla` |

Create explicit `A`/`AAAA` records for those hosts, or a wildcard record such
as `*.example.com`, pointing at the production server. Traefik then selects the
target service from the request `Host` header; no nginx routing layer is needed
for this subdomain layout.

Valhalla CORS is configured for `https://${ROOT_DOMAIN}` and
`https://maps.${ROOT_DOMAIN}`. If the frontend is hosted somewhere else, adjust
the `valhalla_cors` labels in `compose.prod.mono.yaml`.

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

For `compose.prod.mono.yaml`, the same three directories are supplied by these
external Docker volumes:

- `${PROD_MONO_OPENMAPTILES_DATA_VOLUME}` mounted at `/data`
- `${PROD_MONO_OPENMAPTILES_STYLE_VOLUME}` mounted at `/style`
- `${PROD_MONO_OPENMAPTILES_BUILD_VOLUME}` mounted at `/build`

### Sentinel-2 Raster TileJSON

`raster_tile_json_server` only serves files. Generate the raster tiles before
starting the service. See `crates/sat_ingest` for an example of converting satellite imagery to the expected raster tile format.

For `compose.prod.mono.yaml`, `${PROD_MONO_SAT_RASTER_TILE_JSON_VOLUME}` must
contain `tilejson.json` and the generated `{z}/{x}/{y}.png` tree.

### OSM Data For PostGIS

The `osm2pgsql_osm_import` job downloads data from Geofabrik and
imports it into `postgres_osm`. Edit the URL in `compose.jobs.yaml` if a
different extract is needed.

The job uses `postgres_osm/osm2pgsql/style.lua`, which writes all OSM object
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

For `compose.prod.mono.yaml`, `${PROD_MONO_VALHALLA_CUSTOM_FILES_VOLUME}` must
contain these files at the volume root.

## Production Mono Volumes

`compose.prod.mono.yaml` assumes one server running the Docker stack. These
external local Docker volumes must already exist and be populated before
starting the prod mono stack:

```powershell
docker volume create jlh_maps_openmaptiles_data
docker volume create jlh_maps_openmaptiles_style
docker volume create jlh_maps_openmaptiles_build
docker volume create jlh_maps_sat_raster_tile_json
docker volume create jlh_maps_valhalla_custom_files
```

The names above match the defaults in `.prod.mono.env`. Change either the env
file or the created volume names if a different naming convention is used.

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
