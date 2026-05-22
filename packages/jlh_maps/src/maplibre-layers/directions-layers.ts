import { computed, effectScope, shallowRef, toValue, watch, type WatchSource } from 'vue'
import type { GeoLocation } from '@/components/types.ts'
import { decodePolylineToPositions, type Trip } from 'valhalla_client'
import type { FeatureCollection, LineString, Point, Position } from 'geojson'
import { distance } from '@turf/turf'
import { point as turfPoint } from '@turf/helpers'
import { svgToImage } from '@/utils/svg-to-image.ts'
import mapPinIconSvg from 'lucide-static/icons/map-pin.svg?raw'
import type { Map as MapLibreMap } from 'maplibre-gl'
import { useGeoJsonSource, useImage, useLayer } from '@/composables/maplibre.ts'
import { onWatcherCleanupLifo } from '@/composables/helper.ts'

const DIRECTION_TRIP_PRIMARY_SOURCE_ID = 'direction-trip-primary'
const DIRECTION_TRIP_PRIMARY_LAYER_ID = 'direction-trip-primary'
const DIRECTION_TRIP_PRIMARY_CONNECTOR_SOURCE_ID = 'direction-trip-primary-connector'
const DIRECTION_TRIP_PRIMARY_CONNECTOR_LAYER_ID = 'direction-trip-primary-connector'
const DIRECTION_TRIP_ENDPOINT_CONNECTOR_THRESHOLD_METERS = 20

const DIRECTION_STOPS_SOURCE_ID = 'direction-stops'
const DIRECTION_STOPS_SHADOW_LAYER_ID = 'direction-stops-shadow'
export const DIRECTION_STOPS_LAYER_ID = 'direction-stops'
const DIRECTION_STOP_ICON_ID = 'lucide:map-pin'

export function useDirectionsLayers(
  map: MapLibreMap,
  {
    stops,
    tripPrimary,
    visible,
  }: {
    stops: WatchSource<(GeoLocation | null)[]>
    tripPrimary: WatchSource<Trip | null>
    visible?: WatchSource<boolean>
  },
  beforeLayerId?: string,
) {
  const getTripLineCoordinates = (trip: Trip | null): Position[] =>
    trip?.legs.flatMap((leg) => {
      const shape = leg.shape ?? leg.encoded_shape
      return shape ? decodePolylineToPositions(shape) : []
    }) ?? []

  const tripLocationToPosition = (trip: Trip | null, idx: number): Position | null => {
    const location = trip?.locations.at(idx)
    return location ? [location.lon, location.lat] : null
  }

  const distanceMeters = (left: Position, right: Position) =>
    distance(turfPoint(left), turfPoint(right), { units: 'kilometers' }) * 1000

  const makeEndpointConnector = (from: Position | null, to: Position | undefined) => {
    if (!from || !to) return []
    if (distanceMeters(from, to) <= DIRECTION_TRIP_ENDPOINT_CONNECTOR_THRESHOLD_METERS) return []

    return [
      {
        type: 'Feature' as const,
        geometry: {
          type: 'LineString' as const,
          coordinates: [from, to],
        },
        properties: {},
      },
    ]
  }

  // Direction Trip Source

  const directionsTripPrimaryGeoJsonData = computed((): FeatureCollection<LineString> => {
    const coordinates = getTripLineCoordinates(toValue(tripPrimary))

    return {
      type: 'FeatureCollection',
      features:
        coordinates.length >= 2
          ? [
              {
                type: 'Feature',
                geometry: {
                  type: 'LineString',
                  coordinates,
                },
                properties: {},
              },
            ]
          : [],
    }
  })

  const directionsTripPrimaryConnectorGeoJsonData = computed((): FeatureCollection<LineString> => {
    const trip = toValue(tripPrimary)
    const coordinates = getTripLineCoordinates(trip)
    const startLocation = tripLocationToPosition(trip, 0)
    const endLocation = tripLocationToPosition(trip, -1)

    return {
      type: 'FeatureCollection',
      features: [
        ...makeEndpointConnector(startLocation, coordinates[0]),
        ...makeEndpointConnector(coordinates.at(-1) ?? null, endLocation ?? undefined),
      ],
    }
  })

  // Direction Stops Source

  type DirectionStopProperties = {
    label: string
    sortKey: number
  }

  const directionStopsGeoJsonData = computed(
    (): FeatureCollection<Point, DirectionStopProperties> => {
      const stopsValue = toValue<(GeoLocation | null)[]>(stops)
      const lastIdx = stopsValue.length - 1

      return {
        type: 'FeatureCollection',
        features: stopsValue.flatMap((stop, idx) => {
          if (!stop) return []

          const isStart = idx === 0
          const isEnd = idx === lastIdx

          return [
            {
              type: 'Feature',
              geometry: {
                type: 'Point',
                coordinates: [stop.coords.lng, stop.coords.lat],
              },
              properties: {
                label: isStart ? 'S' : isEnd ? 'E' : String(idx),
                sortKey: idx,
              },
            },
          ]
        }),
      }
    },
  )

  const image = shallowRef<ImageData | null>(null)

  watch(
    image,
    (value) => {
      if (value === null) return

      const scope = effectScope()
      scope.run(() => {
        useImage(map, DIRECTION_STOP_ICON_ID, value, {
          options: { pixelRatio: 2 },
        })
      })
      onWatcherCleanupLifo(() => scope.stop())
    },
  )

  svgToImage(mapPinIconSvg, {
    width: 24,
    pixelRatio: 2,
    color: '#2563eb',
  }).then((res) => {
    image.value = res.image
  }, console.error)

  useGeoJsonSource(map, DIRECTION_TRIP_PRIMARY_SOURCE_ID, directionsTripPrimaryGeoJsonData)

  useLayer(
    map,
    {
      id: DIRECTION_TRIP_PRIMARY_LAYER_ID,
      source: DIRECTION_TRIP_PRIMARY_SOURCE_ID,
      type: 'line',
      layout: {
        'line-cap': 'round',
        'line-join': 'round',
      },
      paint: {
        'line-color': '#2563eb',
        'line-opacity': 0.85,
        'line-width': 5,
      },
    },
    {
      beforeId: beforeLayerId,
      visible,
    },
  )

  useGeoJsonSource(
    map,
    DIRECTION_TRIP_PRIMARY_CONNECTOR_SOURCE_ID,
    directionsTripPrimaryConnectorGeoJsonData,
  )

  useLayer(
    map,
    {
      id: DIRECTION_TRIP_PRIMARY_CONNECTOR_LAYER_ID,
      source: DIRECTION_TRIP_PRIMARY_CONNECTOR_SOURCE_ID,
      type: 'line',
      layout: {
        'line-cap': 'round',
        'line-join': 'round',
      },
      paint: {
        'line-color': '#2563eb',
        'line-dasharray': [0.5, 2.0],
        'line-opacity': 0.75,
        'line-width': 3,
      },
    },
    {
      beforeId: DIRECTION_TRIP_PRIMARY_LAYER_ID,
      visible,
    },
  )

  useGeoJsonSource(map, DIRECTION_STOPS_SOURCE_ID, directionStopsGeoJsonData)

  useLayer(
    map,
    {
      id: DIRECTION_STOPS_SHADOW_LAYER_ID,
      source: DIRECTION_STOPS_SOURCE_ID,
      type: 'circle',
      paint: {
        'circle-radius': 16,
        'circle-blur': 0.4,
        'circle-color': '#000000',
        'circle-opacity': 0.3,
        'circle-translate': [0, 0],
        'circle-translate-anchor': 'viewport',
        'circle-pitch-alignment': 'map',
      },
    },
    { visible },
  )

  useLayer(
    map,
    {
      id: DIRECTION_STOPS_LAYER_ID,
      source: DIRECTION_STOPS_SOURCE_ID,
      type: 'symbol',
      layout: {
        'icon-image': DIRECTION_STOP_ICON_ID,
        'icon-size': 1.5,
        'icon-anchor': 'bottom',
        'icon-allow-overlap': true,
        'icon-ignore-placement': true,
        'text-field': ['get', 'label'],
        'text-anchor': 'bottom',
        'text-offset': [0, -2.25],
        'text-size': 16,
        'text-allow-overlap': true,
        'text-ignore-placement': true,
        'symbol-sort-key': ['get', 'sortKey'],
      },
      paint: {
        'text-color': '#111827',
        'text-halo-color': '#ffffff',
        'text-halo-width': 2,
      },
    },
    { visible },
  )
}
