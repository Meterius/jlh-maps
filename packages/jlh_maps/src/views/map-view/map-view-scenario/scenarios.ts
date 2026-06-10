import type { MapLibreMap } from 'maplibre-gl'
import type { Ref } from 'vue'
import {
  type MapViewBaseStyleLayerSettingsStore,
  type MapViewStore,
} from '@/views/map-view/map-view-store.ts'
import { MapViewBaseStyleType } from '@/views/map-view/map-view-types.ts'
import { delay } from '@/utils/helper.ts'

const FORWARD_MOVEMENT_DURATION_MS = 6_000
const FORWARD_MOVEMENT_METERS = 3000

const SETUP_DELAY_MS = 10_000
const STATIC_RUN_DURATION_MS = 5_000

export enum MapViewScenarioName {
  Static = 'static',
  Movement = 'movement',
}

export enum MapViewScenarioRuntimeStatus {
  Initializing = 'initializing',
  Ready = 'ready',
  Running = 'running',
  Finished = 'finished',
  Error = 'error',
  Disposed = 'disposed',
}

export type MapViewScenarioStoreContext = {
  mapViewStore: Ref<MapViewStore>
  baseStyleLayerSettingsStore: Ref<MapViewBaseStyleLayerSettingsStore>
}

export type MapViewScenarioHookContext = MapViewScenarioStoreContext & {
  map: MapLibreMap
  scenario: MapViewScenario
}

export type MapViewScenario = {
  name: MapViewScenarioName
  setup?: (context: MapViewScenarioStoreContext) => Promise<void> | void
  run?: (context: MapViewScenarioHookContext) => Promise<void> | void
}

export const MAP_VIEW_SCENARIOS = {
  [MapViewScenarioName.Static]: {
    name: MapViewScenarioName.Static,
    setup: applyBaseScenarioSettings,
    run: () => delay(STATIC_RUN_DURATION_MS),
  },
  [MapViewScenarioName.Movement]: {
    name: MapViewScenarioName.Movement,
    setup: applyBaseScenarioSettings,
    run: ({ map }) => moveNorth(map, FORWARD_MOVEMENT_METERS, FORWARD_MOVEMENT_DURATION_MS),
  },
} as const satisfies Record<MapViewScenarioName, MapViewScenario>

export function getMapViewScenario(name: string): MapViewScenario | null {
  return isMapViewScenarioName(name) ? MAP_VIEW_SCENARIOS[name] : null
}

export function isMapViewScenarioName(name: string): name is MapViewScenarioName {
  return Object.values(MapViewScenarioName).includes(name as MapViewScenarioName)
}

async function applyBaseScenarioSettings({
  mapViewStore,
  baseStyleLayerSettingsStore,
}: MapViewScenarioStoreContext) {
  mapViewStore.value = createScenarioMapViewStore()
  baseStyleLayerSettingsStore.value = createScenarioBaseStyleLayerSettingsStore()
  await delay(SETUP_DELAY_MS)
}

function createScenarioMapViewStore(): MapViewStore {
  return {
    view: {
      center: [14.33091736043184, 41.0793005898268],
      zoom: 14.969598425949876,
      pitch: 70,
      bearing: -33.07185377494426,
    },
    lighting: {
      automatic: false,
      disableHue: false,
      sun: {
        azimuthDegrees: 11.31,
        elevationDegrees: 32.52,
      },
      moon: {
        azimuthDegrees: 191.31,
        elevationDegrees: -32.52,
      },
    },
    lod: {
      maxZoomLevelsOnScreen: 4.25,
      tileCountMaxMinRatio: 8,
    },
    rainfallEnabled: false,
    baseStyleType: MapViewBaseStyleType.Normal,
    projection: { type: 'mercator' },
    bevyCanvasEnabled: false,
    frameStatisticsEnabled: false,
    advancedRoadsEnabled: false,
  }
}

function createScenarioBaseStyleLayerSettingsStore(): MapViewBaseStyleLayerSettingsStore {
  return {
    [MapViewBaseStyleType.Normal]: {
      bevyEnabled: true,
      shadowsEnabled: true,
      buildingsEnabled: true,
      treesEnabled: true,
      cinematicEnabled: true,
      terrainEnabled: false,
      featureVisibilityDistance: 12,
    },
    [MapViewBaseStyleType.Satellite]: {
      bevyEnabled: true,
      shadowsEnabled: true,
      buildingsEnabled: true,
      treesEnabled: true,
      cinematicEnabled: false,
      terrainEnabled: false,
      featureVisibilityDistance: 10,
    },
  }
}

async function moveNorth(map: MapLibreMap, metersNorth: number, durationMs: number) {
  const center = map.getCenter()
  const destination: [number, number] = [
    center.lng,
    center.lat + metersToLatitudeDegrees(metersNorth),
  ]

  map.easeTo({
    center: destination,
    duration: durationMs,
    easing: (time) => time,
    essential: true,
  })

  await delay(durationMs)
}

function metersToLatitudeDegrees(meters: number) {
  return meters / 111_320
}
