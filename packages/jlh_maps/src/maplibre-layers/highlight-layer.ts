import { toValue, type WatchSource } from 'vue'
import type { Map as MapLibreMap } from 'maplibre-gl'
import type { Geometry } from 'geojson'
import { useGeoJsonSource, useLayer } from '@/composables/maplibre'
import { center } from '@turf/turf'

const HIGHLIGHT_SOURCE_ID = 'highlight'
const HIGHLIGHT_LAYER_ID = 'highlight'

export function useHighlightLayer(map: MapLibreMap, items: WatchSource<Geometry[]>) {
  useGeoJsonSource(map, HIGHLIGHT_SOURCE_ID, () => ({
    type: 'FeatureCollection',
    features: toValue<Geometry[]>(items).map((item) => center(item)),
  }))

  useLayer(map, {
    id: HIGHLIGHT_LAYER_ID,
    source: HIGHLIGHT_SOURCE_ID,
    type: 'circle',
    paint: {
      'circle-radius': 25,
      'circle-color': 'transparent',
      'circle-stroke-color': '#1d87bf',
      'circle-stroke-opacity': 0.75,
      'circle-stroke-width': 3,
    },
  })
}
