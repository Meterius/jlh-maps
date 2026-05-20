import { computed, onBeforeUnmount, ref, shallowRef, toValue, watch, type WatchSource } from 'vue'
import type { Map as MapLibreMap } from 'maplibre-gl'

type RainViewerFrame = {
  time: number
  path: string
}

type RainViewerWeatherMaps = {
  host: string
  radar?: {
    past?: RainViewerFrame[]
  }
}

type RainfallRasterLayerOptions = {
  visible: WatchSource<boolean>
  onLoadError?: (error: unknown) => void
}

const RAIN_VIEWER_MANIFEST_URL = 'https://api.rainviewer.com/public/weather-maps.json'
const RAINFALL_SOURCE_ID = 'rainviewer-rainfall'
const RAINFALL_LAYER_ID = 'rainviewer-rainfall-layer'

async function getLatestRainViewerFrame() {
  const response = await fetch(RAIN_VIEWER_MANIFEST_URL)

  if (!response.ok) {
    throw new Error(`RainViewer manifest request failed: ${response.status} ${response.statusText}`)
  }

  const manifest = (await response.json()) as RainViewerWeatherMaps
  const latestFrame = manifest.radar?.past?.at(-1)

  if (!latestFrame) {
    throw new Error('RainViewer manifest did not include any past radar frames')
  }

  return {
    frame: latestFrame,
    tileUrl: `${manifest.host}${latestFrame.path}/512/{z}/{x}/{y}/2/1_1.png`,
  }
}

function unregisterRainfallRasterLayer(map: MapLibreMap) {
  if (map.getLayer(RAINFALL_LAYER_ID)) {
    map.removeLayer(RAINFALL_LAYER_ID)
  }

  if (map.getSource(RAINFALL_SOURCE_ID)) {
    map.removeSource(RAINFALL_SOURCE_ID)
  }
}

function registerRainfallRasterLayer(map: MapLibreMap, tileUrl: string, beforeLayerId?: string) {
  unregisterRainfallRasterLayer(map)

  map.addSource(RAINFALL_SOURCE_ID, {
    type: 'raster',
    tiles: [tileUrl],
    tileSize: 512,
    maxzoom: 7,
    attribution:
      '<a href="https://www.rainviewer.com/" target="_blank" rel="noopener">RainViewer</a>',
  })

  map.addLayer(
    {
      id: RAINFALL_LAYER_ID,
      type: 'raster',
      source: RAINFALL_SOURCE_ID,
      paint: {
        'raster-opacity': 0.5,
        'raster-fade-duration': 250,
      },
    },
    beforeLayerId && map.getLayer(beforeLayerId) ? beforeLayerId : undefined,
  )
}

export function useRainfallRasterLayer({ visible, onLoadError }: RainfallRasterLayerOptions) {
  const activeFrame = shallowRef<RainViewerFrame | null>(null)
  const activeTileUrl = shallowRef<string | null>(null)
  const loading = ref(false)
  const error = shallowRef<unknown | null>(null)

  const rasterDataTime = computed(() =>
    activeFrame.value ? new Date(activeFrame.value.time * 1000) : null,
  )

  let map: MapLibreMap | null = null
  let beforeLayerId: string | undefined
  let requestId = 0
  let stopVisibleWatch: (() => void) | null = null

  const showActiveLayer = () => {
    if (!map || !toValue(visible) || !activeTileUrl.value) return

    registerRainfallRasterLayer(map, activeTileUrl.value, beforeLayerId)
  }

  const refreshData = async () => {
    const currentRequestId = ++requestId
    loading.value = true
    error.value = null

    try {
      const latest = await getLatestRainViewerFrame()

      if (currentRequestId !== requestId) return

      activeFrame.value = latest.frame
      activeTileUrl.value = latest.tileUrl
      showActiveLayer()
    } catch (caughtError) {
      if (currentRequestId !== requestId) return

      error.value = caughtError
      onLoadError?.(caughtError)
      throw caughtError
    } finally {
      if (currentRequestId === requestId) {
        loading.value = false
      }
    }
  }

  const register = (registeredMap: MapLibreMap, registeredBeforeLayerId?: string) => {
    unregister()

    map = registeredMap
    beforeLayerId = registeredBeforeLayerId

    stopVisibleWatch = watch(
      () => toValue(visible),
      (enabled) => {
        if (!map) return

        if (!enabled) {
          unregisterRainfallRasterLayer(map)
          return
        }

        showActiveLayer()
        refreshData().catch(() => {
          // The caller receives the error through onLoadError; keep this watcher non-fatal.
        })
      },
      { immediate: true },
    )
  }

  const unregister = () => {
    requestId++
    stopVisibleWatch?.()
    stopVisibleWatch = null

    if (map) {
      unregisterRainfallRasterLayer(map)
    }

    map = null
    beforeLayerId = undefined
    loading.value = false
  }

  onBeforeUnmount(unregister)

  return {
    error,
    loading,
    rasterDataTime,
    refreshData,
    register,
    unregister,
  }
}
