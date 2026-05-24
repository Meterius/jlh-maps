import type {
  ExpressionSpecification,
  Map as MapLibreMap,
  MapGeoJSONFeature,
} from 'maplibre-gl'
import { toValue, type WatchSource } from 'vue'
import { createSharedComposable } from '@vueuse/core'
import type { FeatureCollection } from 'geojson'
import { useGeoJsonSource, type UseLayerOptions } from '@/composables/maplibre'
import {
  type MarkerLayerSpecification,
  useMarkerImageSourceProvider,
  useMarkerLayer,
} from '@/maplibre-layers/marker-layer.ts'

const SELECTED_MARKER_SOURCE_ID = 'selected-marker'
const SELECTED_MARKER_LAYER_ID = 'selected-marker'
const SELECTED_MARKER_ICON_NAME = 'selected-marker'

const SELECTED_MARKER_LABEL_PROPERTY = 'name'

const useSelectedMarkerImageProvider = createSharedComposable(() =>
  useMarkerImageSourceProvider(async () => '', [SELECTED_MARKER_ICON_NAME]),
)

const makeSelectedFeatureCollection = (
  features: MapGeoJSONFeature[],
): FeatureCollection => ({
  type: 'FeatureCollection',
  features: features.map((feature) => ({
    type: 'Feature',
    id: feature.id,
    properties: feature.properties,
    geometry: feature.geometry,
  })),
})

const makeSelectedMarkerLayer = (): MarkerLayerSpecification => ({
  id: SELECTED_MARKER_LAYER_ID,
  type: 'symbol',
  source: SELECTED_MARKER_SOURCE_ID,
  markerOptions: {
    color: '#dc2626',
    iconColor: '#dc2626',
  },
  marker: {
    scale: 1.42,
    textSize: 16,
    headIconName: ['literal', SELECTED_MARKER_ICON_NAME] as ExpressionSpecification,
  },
  layout: {
    'icon-allow-overlap': true,
    'icon-ignore-placement': false,
    'text-allow-overlap': true,
    'text-ignore-placement': false,
    'text-field': ['get', SELECTED_MARKER_LABEL_PROPERTY],
    'symbol-sort-key': -100000.0,
  },
  paint: {
    'icon-opacity-transition': { duration: 0, delay: 0 },
    'text-opacity-transition': { duration: 0, delay: 0 },
    'text-color': '#991b1b',
    'text-halo-color': '#ffffff',
    'text-halo-width': 1.5,
  },
})

export const useSelectedMarkerLayer = (
  map: MapLibreMap,
  features: WatchSource<MapGeoJSONFeature[]>,
  options?: UseLayerOptions,
) => {
  useGeoJsonSource(map, SELECTED_MARKER_SOURCE_ID, () =>
    makeSelectedFeatureCollection(toValue(features)),
  )

  useMarkerLayer(map, makeSelectedMarkerLayer(), useSelectedMarkerImageProvider(), options)

  return {
    layerId: SELECTED_MARKER_LAYER_ID,
    sourceId: SELECTED_MARKER_SOURCE_ID,
  }
}
