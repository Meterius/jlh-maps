import type { MapLibreMap, StyleOptions, StyleSwapOptions } from 'maplibre-gl'
import { effectScope, watch, type EffectScope, type WatchSource } from 'vue'
import { useMap } from '@indoorequal/vue-maplibre-gl'
import { onWatcherCleanupLifo, watchDefinedOnce } from '@/composables/helper.ts'

export interface MapStyleLifecycleConfig {
  source: string
  options?: StyleSwapOptions & StyleOptions
  // callback when map is loaded with new style
  instantiate: (map: MapLibreMap) => void
}

// Manages style-scoped reactive resources. instantiate() runs inside a fresh effect scope,
// so nested composables can use scope disposal like child components use unmount cleanup.
export function useMapStyleLifecycle(
  mapKey: string | symbol | undefined,
  style: WatchSource<MapStyleLifecycleConfig>,
) {
  const mapInstance = useMap(mapKey)

  watchDefinedOnce(
    () => mapInstance.map,
    (map) => {
      let styleRequestId = 0
      let styleScope: EffectScope | null = null

      const cleanupStyle = () => {
        const scope = styleScope

        styleScope = null
        scope?.stop()
      }

      const stopStyleWatch = watch(
        style,
        async (selectedStyle) => {
          const currentStyleRequestId = ++styleRequestId

          cleanupStyle()

          const styleLoadedPromise = map.once('style.load')

          if (!(styleLoadedPromise instanceof Promise)) throw new Error('Expected a promise')

          map.setStyle(selectedStyle.source, selectedStyle.options)

          await styleLoadedPromise

          // prevent instantiation if style change occurred in between
          if (currentStyleRequestId !== styleRequestId) return

          styleScope = effectScope()
          try {
            styleScope.run(() => selectedStyle.instantiate(map))
          } catch (error) {
            cleanupStyle()
            throw error
          }
        },
        { immediate: true },
      )

      onWatcherCleanupLifo(() => {
        styleRequestId++
        cleanupStyle()
        stopStyleWatch()
      })
    },
  )
}
