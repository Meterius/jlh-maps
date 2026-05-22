import type { MapLibreMap, StyleOptions, StyleSwapOptions } from 'maplibre-gl'
import { onWatcherCleanup, watch, type WatchSource } from 'vue'
import { useMap } from '@indoorequal/vue-maplibre-gl'
import { watchDefinedOnce } from '@/composables/helper.ts'

export interface MapStyleLifecycleConfig {
  source: string
  options?: StyleSwapOptions & StyleOptions,
  // callback when map is loaded with new style
  instantiate: (map: MapLibreMap) => StyleInstance,
}

export interface StyleInstance {
  onRemove?: () => void
}

// Manages style lifecycle, allows for changes styles that require mutations on the map and
// require cleanup, expects that map style is not altered by other actors
export function useMapStyleLifecycle(
  mapKey: string | symbol | undefined,
  style: WatchSource<MapStyleLifecycleConfig>,
) {
  const mapInstance = useMap(mapKey)

  watchDefinedOnce(
    () => mapInstance.map,
    (map) => {
      const onWatcherCleanupCallbacks: (() => void)[] = []
      let styleRequestId = 0

      const onInstanceCleanupCallbacks: (() => void)[] = []

      const cleanupStyle = () => {
        onInstanceCleanupCallbacks.splice(0).forEach((callback) => callback())
      }

      onWatcherCleanupCallbacks.push(
        watch(
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

            const inst = selectedStyle.instantiate(map)
            onInstanceCleanupCallbacks.push(() => inst.onRemove?.())
          },
          { immediate: true },
        ).stop,
      )

      onWatcherCleanup(() => {
        styleRequestId++
        cleanupStyle()
        onWatcherCleanupCallbacks.forEach((callback) => callback())
      })
    },
  )
}