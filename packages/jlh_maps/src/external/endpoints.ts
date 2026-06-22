import { type OsmId, OsmType } from '../utils/osm.ts'

const APP_BASE_URL = new URL(import.meta.env.BASE_URL, window.location.href)

function makeAppUrl(value: string): URL {
  return new URL(value, APP_BASE_URL)
}

export const TILESERVER_URL = makeAppUrl(import.meta.env.VITE_TILESERVER_OMT_URL)
export const STATIC_TILESERVER_URL = makeAppUrl(import.meta.env.VITE_TILESERVER_STATIC_URL)
export const API_URL = makeAppUrl(import.meta.env.VITE_API_URL)
export const VALHALLA_URL = makeAppUrl(import.meta.env.VITE_VALHALLA_URL)

export const TILESERVER_ROADS_PMTILES_URL = new URL('roads/tiles.pmtiles', STATIC_TILESERVER_URL)
export const TILESERVER_GTFS_PMTILES_URL = new URL('gtfs/tiles.pmtiles', STATIC_TILESERVER_URL)

export const TILESERVER_OMT_DEFAULT_STYLE_TILEJSON_URL = import.meta.env
  .VITE_TILESERVER_OMT_STYLE_URL
  ? makeAppUrl(import.meta.env.VITE_TILESERVER_OMT_STYLE_URL)
  : new URL('styles/omt_default/style.json', TILESERVER_URL)

export const TILESERVER_RASTER_SEN2_TILE_URL_PATTERN = `${STATIC_TILESERVER_URL.toString().replace(/\/$/, '')}/raster/sen2/{z}/{x}/{y}.png`

export interface OsmData {
  tags: Record<string, string>
  attrs: Record<string, string | number>
}

export interface GtfsAggregatedStop {
  version_id: number
  stop_id: string
  stop_code: string | null
  stop_name: string | null
  stop_desc: string | null
  stop_lat: number | null
  stop_lon: number | null
  zone_id: string | null
  stop_url: string | null
  location_type: number | null
  parent_station: string | null
  wheelchair_boarding: number | null
  platform_code: string | null
  route_ids: string[]
  children: GtfsAggregatedStop[]
}

export interface GtfsRoute {
  version_id: number
  route_id: string
  agency_id: string | null
  route_short_name: string | null
  route_long_name: string | null
  route_desc: string | null
  route_type: number | null
  route_url: string | null
  route_color: string | null
  route_text_color: string | null
}

export async function getOsmData(osm_id: OsmId): Promise<OsmData | null> {
  const type = {
    [OsmType.Node]: 'node',
    [OsmType.Way]: 'way',
    [OsmType.Relation]: 'relation',
  }[osm_id.type]

  const res = await fetch(new URL(`osm/element/${type}/${osm_id.key}`, API_URL))

  if (res.ok) {
    return res.json()
  } else if (res.status === 404) {
    return null
  }

  throw new Error(
    `Failed to fetch OSM data for ${osm_id.type}/${osm_id.key}: ${res.status} ${res.statusText}`,
  )
}

export async function getGtfsAggregatedStop(
  versionId: number,
  stopId: string,
): Promise<GtfsAggregatedStop | null> {
  const res = await fetch(
    new URL(`gtfs/version/${versionId}/aggregated-stop/${encodeURIComponent(stopId)}`, API_URL),
  )

  if (res.ok) {
    return res.json()
  } else if (res.status === 404) {
    return null
  }

  throw new Error(
    `Failed to fetch GTFS stop ${versionId}/${stopId}: ${res.status} ${res.statusText}`,
  )
}

export async function getGtfsRoute(versionId: number, routeId: string): Promise<GtfsRoute | null> {
  const res = await fetch(
    new URL(`gtfs/version/${versionId}/route/${encodeURIComponent(routeId)}`, API_URL),
  )

  if (res.ok) {
    return res.json()
  } else if (res.status === 404) {
    return null
  }

  throw new Error(
    `Failed to fetch GTFS route ${versionId}/${routeId}: ${res.status} ${res.statusText}`,
  )
}
