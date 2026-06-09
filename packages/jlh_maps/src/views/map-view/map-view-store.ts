import { createInjectionState, useLocalStorage } from '@vueuse/core'
import { computed } from 'vue'
import { createInjectOrThrow } from '@/composables/helper.ts'
import { MapViewBaseStyleType } from '@/views/map-view/map-view-types.ts'
import type { ProjectionSpecification } from 'maplibre-gl'

export type MapViewStore = {
  view: {
    center: [number, number]
    zoom: number
    pitch: number
    bearing: number
  }
  lighting: MapViewLightingSettings
  lod: MapLibreLodSettings
  rainfallEnabled: boolean
  baseStyleType: MapViewBaseStyleType
  projection?: ProjectionSpecification
  bevyCanvasEnabled: boolean
  frameStatisticsEnabled: boolean
  advancedRoadsEnabled: boolean
}

export type MapCelestialLightingSettings = {
  azimuthDegrees: number
  elevationDegrees: number
}

export type MapViewLightingSettings = {
  automatic: boolean
  time?: string
  sun: MapCelestialLightingSettings
  moon: MapCelestialLightingSettings
}

export type MapLibreLodSettings = {
  maxZoomLevelsOnScreen: number
  tileCountMaxMinRatio: number
}

export const DEFAULT_MAPLIBRE_LOD_SETTINGS: MapLibreLodSettings = {
  maxZoomLevelsOnScreen: 9.314,
  tileCountMaxMinRatio: 3,
}

export type MapViewBaseStyleLayerSettings = {
  bevyEnabled: boolean
  shadowsEnabled: boolean
  buildingsEnabled: boolean
  treesEnabled: boolean
  cinematicEnabled: boolean
  terrainEnabled: boolean
  featureVisibilityDistance: number
}

export type MapViewBaseStyleLayerSettingsStore = Record<
  MapViewBaseStyleType,
  MapViewBaseStyleLayerSettings
>

const [provideMapViewStore, useMapViewStore] = createInjectionState(() => {
  const mapViewStore = useLocalStorage<MapViewStore>(
    'jlh-maps:map-view',
    createDefaultMapViewStore(),
    {
      mergeDefaults: mergeMapViewStoreDefaults,
    },
  )

  const baseStyleLayerSettingsStore = useLocalStorage<MapViewBaseStyleLayerSettingsStore>(
    'jlh-maps:map-view:base-style-layer-settings',
    createDefaultMapViewBaseStyleLayerSettingsStore(),
    {
      mergeDefaults: mergeMapViewBaseStyleLayerSettingsStoreDefaults,
    },
  )

  const currentBaseStyleLayerSettings = computed(
    () => baseStyleLayerSettingsStore.value[mapViewStore.value.baseStyleType],
  )

  return {
    mapViewStore,
    baseStyleLayerSettingsStore,
    currentBaseStyleLayerSettings,
  }
})

export { provideMapViewStore }

export const useMapViewStoreOrThrow = createInjectOrThrow(
  useMapViewStore,
  'Map view store was not provided',
)

function createDefaultMapViewStore(): MapViewStore {
  return {
    view: {
      center: [13.35203105083487, 52.499757263332086],
      zoom: 14,
      pitch: 0,
      bearing: 0,
    },
    lighting: {
      automatic: false,
      sun: {
        azimuthDegrees: 11.31,
        elevationDegrees: 32.52,
      },
      moon: {
        azimuthDegrees: 191.31,
        elevationDegrees: -32.52,
      },
    },
    lod: { ...DEFAULT_MAPLIBRE_LOD_SETTINGS },
    rainfallEnabled: false,
    baseStyleType: MapViewBaseStyleType.Normal,
    projection: { type: 'mercator' },
    bevyCanvasEnabled: false,
    frameStatisticsEnabled: false,
    advancedRoadsEnabled: false,
  }
}

function createDefaultMapViewBaseStyleLayerSettingsStore(): MapViewBaseStyleLayerSettingsStore {
  return {
    [MapViewBaseStyleType.Normal]: createDefaultMapViewBaseStyleLayerSettings(),
    [MapViewBaseStyleType.Satellite]: {
      ...createDefaultMapViewBaseStyleLayerSettings(),
      bevyEnabled: false,
    },
  }
}

function createDefaultMapViewBaseStyleLayerSettings(): MapViewBaseStyleLayerSettings {
  return {
    bevyEnabled: true,
    shadowsEnabled: true,
    buildingsEnabled: true,
    treesEnabled: true,
    cinematicEnabled: false,
    terrainEnabled: false,
    featureVisibilityDistance: 10,
  }
}

function mergeMapViewStoreDefaults(
  storageValue: MapViewStore,
  defaults: MapViewStore,
): MapViewStore {
  return {
    ...defaults,
    ...storageValue,
    view: {
      ...defaults.view,
      ...storageValue.view,
    },
    lighting: {
      ...defaults.lighting,
      ...storageValue.lighting,
      sun: {
        ...defaults.lighting.sun,
        ...storageValue.lighting?.sun,
      },
      moon: {
        ...defaults.lighting.moon,
        ...storageValue.lighting?.moon,
      },
    },
    lod: {
      ...defaults.lod,
      ...storageValue.lod,
    },
  }
}

function mergeMapViewBaseStyleLayerSettingsStoreDefaults(
  storageValue: MapViewBaseStyleLayerSettingsStore,
  defaults: MapViewBaseStyleLayerSettingsStore,
): MapViewBaseStyleLayerSettingsStore {
  return {
    [MapViewBaseStyleType.Normal]: {
      ...defaults[MapViewBaseStyleType.Normal],
      ...storageValue[MapViewBaseStyleType.Normal],
    },
    [MapViewBaseStyleType.Satellite]: {
      ...defaults[MapViewBaseStyleType.Satellite],
      ...storageValue[MapViewBaseStyleType.Satellite],
    },
  }
}
