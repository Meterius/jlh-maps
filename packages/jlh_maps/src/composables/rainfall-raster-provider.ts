import { computed, ref, shallowRef } from 'vue'
import { onScopeDisposeLifo } from '@/composables/helper.ts'

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

export type RainfallRasterSourceProviderOptions = {
  onLoadError?: (error: unknown) => void
}

export type RainfallRasterSourceProviderRet = ReturnType<typeof useRainfallRasterProvider>

const RAIN_VIEWER_MANIFEST_URL = 'https://api.rainviewer.com/public/weather-maps.json'

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
    tileUrlTemplate: `${manifest.host}${latestFrame.path}/512/{z}/{x}/{y}/2/1_1.png`,
  }
}

export function useRainfallRasterProvider({
  onLoadError,
}: RainfallRasterSourceProviderOptions = {}) {
  const activeFrame = shallowRef<RainViewerFrame | null>(null)
  const activeTileUrlTemplate = shallowRef<string | null>(null)
  const loading = ref(false)
  const error = shallowRef<unknown | null>(null)
  let requestId = 0

  const rasterDataTime = computed(() =>
    activeFrame.value ? new Date(activeFrame.value.time * 1000) : null,
  )
  const tiles = computed(() => (activeTileUrlTemplate.value ? [activeTileUrlTemplate.value] : null))

  const refreshData = async () => {
    const currentRequestId = ++requestId
    loading.value = true
    error.value = null

    try {
      const latest = await getLatestRainViewerFrame()

      if (currentRequestId !== requestId) return

      activeFrame.value = latest.frame
      activeTileUrlTemplate.value = latest.tileUrlTemplate
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

  onScopeDisposeLifo(() => {
    requestId++
  })

  return {
    error,
    loading,
    rasterDataTime,
    refreshData,
    tileUrlTemplate: activeTileUrlTemplate,
    tiles,
  }
}
