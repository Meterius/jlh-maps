import { createInjectionState } from '@vueuse/core'
import { computed, shallowRef } from 'vue'
import { createInjectOrThrow } from '@/composables/helper.ts'
import type { MapGeoJSONFeature } from 'maplibre-gl'
import type { MapFeatureId } from '@/composables/maplibre'
import { extractOsmIdFromOmtFeatureId, type OsmId } from '@/utils/osm.ts'

export type SelectionItem = {
  feature: MapGeoJSONFeature
  featureId: MapFeatureId
  osmId?: OsmId
}

const [provideMapSelection, useMapSelection] = createInjectionState(() => {
  const selectedMap = shallowRef<Record<MapFeatureId, SelectionItem>>({})

  return {
    selected: computed(() => Object.values(selectedMap.value)),
    selectFeature: (feature: MapGeoJSONFeature) => {
      selectedMap.value =
        feature.id !== undefined
          ? {
              [feature.id]: {
                feature,
                featureId: feature.id,
                osmId:
                  typeof feature.id === 'number'
                    ? (extractOsmIdFromOmtFeatureId(feature.id) ?? undefined)
                    : undefined,
              },
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
