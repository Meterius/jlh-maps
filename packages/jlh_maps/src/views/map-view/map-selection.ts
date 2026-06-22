import { createInjectionState } from '@vueuse/core'
import { computed, shallowRef } from 'vue'
import { createInjectOrThrow } from '@/composables/helper.ts'
import type { MapGeoJSONFeature } from 'maplibre-gl'
import type { MapFeatureId } from '@/composables/maplibre'
import type { OsmId } from '@/utils/osm.ts'

export enum SelectionItemKind {
  Osm = 'osm',
  GtfsStop = 'gtfs-stop',
}

type BaseSelectionItem = {
  feature: MapGeoJSONFeature
  label: string
  featureId: MapFeatureId
}

export type OsmSelectionItem = BaseSelectionItem & {
  kind: SelectionItemKind.Osm
  osmId?: OsmId
}

export type GtfsStopRef = {
  versionId: number
  stopId: string
}

export type GtfsStopSelectionItem = BaseSelectionItem & {
  kind: SelectionItemKind.GtfsStop
  stopRef?: GtfsStopRef
}

export type SelectionItem = OsmSelectionItem | GtfsStopSelectionItem

export type SelectFeatureInput =
  | Omit<OsmSelectionItem, 'featureId'>
  | Omit<GtfsStopSelectionItem, 'featureId'>

const [provideMapSelection, useMapSelection] = createInjectionState(() => {
  const selectedMap = shallowRef<Record<MapFeatureId, SelectionItem>>({})

  return {
    selected: computed(() => Object.values(selectedMap.value)),
    selectFeature: (item: SelectFeatureInput) => {
      selectedMap.value =
        item.feature.id !== undefined
          ? {
              [item.feature.id]: {
                ...item,
                featureId: item.feature.id,
              } as const,
            }
          : {}
    },
    clearSelection: () => {
      selectedMap.value = {}
    },
  }
})

export { provideMapSelection }

export const useMapSelectionOrThrow = createInjectOrThrow(
  useMapSelection,
  'Map selection was not provided',
)
