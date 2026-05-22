import { effectScope, toValue, watch, type WatchSource } from 'vue'
import type { Map as MapLibreMap } from 'maplibre-gl'
import { useLayer, useRasterTilesBasedSource } from '@/composables/maplibre.ts'
import { onWatcherCleanupLifo } from '@/composables/helper.ts'

type RainfallRasterLayerOptions = {
  tiles: WatchSource<string[] | null>
  visible?: WatchSource<boolean>
}

const RAINFALL_SOURCE_ID = 'rainviewer-rainfall'
const RAINFALL_LAYER_ID = 'rainviewer-rainfall-layer'

export function useRainfallRasterLayer(
  map: MapLibreMap,
  { tiles, visible }: RainfallRasterLayerOptions,
  beforeLayerId?: string,
) {
  watch(
    () => toValue(tiles),
    (tileTemplates) => {
      if (!tileTemplates?.length) return

      const scope = effectScope()
      try {
        scope.run(() => {
          useRasterTilesBasedSource(map, RAINFALL_SOURCE_ID, {
            tiles: tileTemplates,
            tileSize: 512,
            maxzoom: 7,
            attribution:
              '<a href="https://www.rainviewer.com/" target="_blank" rel="noopener">RainViewer</a>',
          })

          useLayer(
            map,
            {
              id: RAINFALL_LAYER_ID,
              type: 'raster',
              source: RAINFALL_SOURCE_ID,
              paint: {
                'raster-opacity': 0.5,
                'raster-fade-duration': 250,
              },
            },
            {
              beforeId: beforeLayerId && map.getLayer(beforeLayerId) ? beforeLayerId : undefined,
              visible,
            },
          )
        })
      } catch (error) {
        scope.stop()
        throw error
      }

      onWatcherCleanupLifo(() => scope.stop())
    },
    { immediate: true },
  )
}
