<template>
  <div
    class="grid w-full auto-rows-max content-start overflow-y-auto overflow-x-hidden"
  >
    <div v-if="badge" class="px-2 pt-2">
      <UBadge :icon="badge.icon" color="info" variant="outline" :label="badge.label" />
    </div>

    <InterpretedFeatureProperties
      :feature="props.feature"
      :osm-data="osmData"
      :loading="loadingOsmData"
    />

    <USeparator />

    <div v-if="props.osm_id" class="min-w-0 w-full">
      <div class="w-full p-4">
        <UCollapsible v-model:open="osmTagsOpen">
          <UButton
            class="px-0 cursor-pointer"
            block
            color="neutral"
            variant="link"
            label="OSM Tags"
            trailing-icon="lucide:chevron-down"
          />

          <template #content>
            <div class="pt-2">
              <UTable
                sticky
                :data="tagTableData"
                :loading="loadingOsmData"
                :ui="tableUi"
                  class="max-h-[400px] w-full rounded-md border border-default"
              ></UTable>
            </div>
          </template>
        </UCollapsible>
      </div>

      <USeparator />
    </div>

    <div class="min-w-0 max-w-full">
      <div class="w-full p-4">
        <UCollapsible v-model:open="featurePropertiesOpen">
          <UButton
            class="px-0 cursor-pointer"
            block
            color="neutral"
            variant="link"
            label="Feature Properties"
            trailing-icon="lucide:chevron-down"
          />

          <template #content>
            <div class="pt-2">
              <UTable
                sticky
                :data="tableData"
                :ui="tableUi"
                class="max-h-[400px] w-full rounded-md border border-default"
              ></UTable>
            </div>
          </template>
        </UCollapsible>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import type { GeoJSONFeature } from 'maplibre-gl'
import { computedAsync } from '@vueuse/core'
import { getOsmData } from '@/external/endpoints.ts'
import type { OsmId } from '@/utils/osm.js'
import InterpretedFeatureProperties from '@/components/map-slideover/InterpretedFeatureProperties.vue'
import { isOsmAmenityValue, OSM_AMENITY_METADATA } from '@/constants/osm-mapping.ts'
import { isOmtPoiSubclass, OMT_POI_SUBCLASS_METADATA } from '@/constants/omt-mapping.ts'

const props = defineProps<{
  osm_id?: OsmId
  feature?: GeoJSONFeature
}>()

const loadingOsmData = ref(false)
const osmTagsOpen = ref(false)
const featurePropertiesOpen = ref(false)

const tableUi = {
  td: 'py-2',
  root: 'relative block min-w-0 max-w-full overflow-auto',
  base: 'w-max min-w-full',
  tbody: 'isolate',
}

const osmData = computedAsync(
  async () => (props.osm_id ? getOsmData(props.osm_id) : null),
  null,
  loadingOsmData,
)

const tableData = computed(() => {
  return Object.entries(props.feature?.properties ?? {}).map(([key, value]) => ({ key, value }))
})

const tagTableData = computed(() => {
  return loadingOsmData.value
    ? undefined
    : Object.entries(osmData.value?.tags ?? {}).map(([key, value]) => ({ key, value }))
})

const badge = computed(() => {
  const osmAmenityTag = osmData.value?.tags['amenity']
  if (!loadingOsmData.value && osmAmenityTag && isOsmAmenityValue(osmAmenityTag)) {
    return OSM_AMENITY_METADATA[osmAmenityTag]
  }

  const featureSubclassTag = props.feature?.properties['subclass']
  if (featureSubclassTag && isOmtPoiSubclass(featureSubclassTag)) {
    return OMT_POI_SUBCLASS_METADATA[featureSubclassTag]
  }

  return null
})
</script>
