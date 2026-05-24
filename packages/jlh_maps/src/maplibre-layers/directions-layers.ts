import { computed, toValue, type WatchSource } from 'vue'
import type { GeoLocation } from '@/components/types.ts'
import { decodePolylineToPositions, type Trip } from 'valhalla_client'
import type { FeatureCollection, LineString, Point, Position } from 'geojson'
import { distance } from '@turf/turf'
import { point as turfPoint } from '@turf/helpers'
import mapPinIconSvg from 'lucide-static/icons/map-pin.svg?raw'
import { createSharedComposable } from '@vueuse/core'
import type { Map as MapLibreMap } from 'maplibre-gl'
import type { ExpressionSpecification } from 'maplibre-gl'
import { useGeoJsonSource, useLayer } from '@/composables/maplibre'
import {
  type MarkerLayerSpecification,
  useMarkerImageSourceProvider,
  useMarkerLayer,
} from '@/maplibre-layers/marker-layer.ts'
import { SORT_KEY_DIRECTION_STOP_MARKER } from '@/maplibre-layers/constants.ts'

const DIRECTION_TRIP_PRIMARY_SOURCE_ID = 'direction-trip-primary'
const DIRECTION_TRIP_PRIMARY_LAYER_ID = 'direction-trip-primary'
const DIRECTION_TRIP_PRIMARY_CONNECTOR_SOURCE_ID = 'direction-trip-primary-connector'
const DIRECTION_TRIP_PRIMARY_CONNECTOR_LAYER_ID = 'direction-trip-primary-connector'
const DIRECTION_TRIP_ENDPOINT_CONNECTOR_THRESHOLD_METERS = 20

const DIRECTION_STOPS_SOURCE_ID = 'direction-stops'
export const DIRECTION_STOPS_LAYER_ID = 'direction-stops'
const DIRECTION_STOP_ICON_NAME = 'map-pin'
const DIRECTION_STOP_ICON_COLOR = '#2563eb'

const useDirectionStopMarkerImageProvider = createSharedComposable(() =>
  useMarkerImageSourceProvider(async () => mapPinIconSvg, [DIRECTION_STOP_ICON_NAME]),
)

const makeDirectionStopMarkerLayer = (): MarkerLayerSpecification => ({
  id: DIRECTION_STOPS_LAYER_ID,
  type: 'symbol',
  source: DIRECTION_STOPS_SOURCE_ID,
  markerOptions: {
    color: DIRECTION_STOP_ICON_COLOR,
    iconColor: DIRECTION_STOP_ICON_COLOR,
  },
  marker: {
    scale: 1,
    textSize: 16,
    headIconName: ['literal', DIRECTION_STOP_ICON_NAME] as ExpressionSpecification,
  },
  layout: {
    'icon-allow-overlap': false,
    'icon-ignore-placement': false,
    'text-allow-overlap': false,
    'text-ignore-placement': false,
    'text-field': ['get', 'label'],
    'symbol-sort-key': SORT_KEY_DIRECTION_STOP_MARKER,
  },
  paint: {
    'text-color': DIRECTION_STOP_ICON_COLOR,
    'text-halo-color': '#ffffff',
    'text-halo-width': 2,
  },
})

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
                label: isStart ? 'Start' : isEnd ? 'End' : String(idx),
              },
            },
          ]
        }),
      }
    },
  )

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
        'line-color': DIRECTION_STOP_ICON_COLOR,
        'line-opacity': 0.85,
        'line-width': 5,
      },
    },
    {
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
        'line-color': DIRECTION_STOP_ICON_COLOR,
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

  useMarkerLayer(map, makeDirectionStopMarkerLayer(), useDirectionStopMarkerImageProvider(), {
    visible,
  })
}
