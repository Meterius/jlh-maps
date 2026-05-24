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
  sun: {
    azimuthDegrees: number
    elevationDegrees: number
  }
  rainfallEnabled: boolean
  baseStyleType: MapViewBaseStyleType
  projection?: ProjectionSpecification
  bevyCanvasEnabled: boolean
}

export type MapViewBaseStyleLayerSettings = {
  shadowsEnabled: boolean
  buildingsEnabled: boolean
  terrainEnabled: boolean
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
    sun: {
      azimuthDegrees: 11.31,
      elevationDegrees: 32.52,
    },
    rainfallEnabled: false,
    baseStyleType: MapViewBaseStyleType.Normal,
    projection: { type: 'mercator' },
    bevyCanvasEnabled: false,
  }
}

function createDefaultMapViewBaseStyleLayerSettingsStore(): MapViewBaseStyleLayerSettingsStore {
  return {
    [MapViewBaseStyleType.Normal]: createDefaultMapViewBaseStyleLayerSettings(),
    [MapViewBaseStyleType.Satellite]: createDefaultMapViewBaseStyleLayerSettings(),
  }
}

function createDefaultMapViewBaseStyleLayerSettings(): MapViewBaseStyleLayerSettings {
  return {
    shadowsEnabled: true,
    buildingsEnabled: true,
    terrainEnabled: false,
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
    sun: {
      ...defaults.sun,
      ...storageValue.sun,
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
