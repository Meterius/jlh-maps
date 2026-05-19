<template>
  <div class="grid grid-rows-[auto_1fr] w-full overflow-auto overflow-x-hidden">
    <div v-if="props.osm_id" class="row">
      <div class="p-4">
        <h5 class="pb-2">OSM Tags</h5>
        <UTable
          sticky
          :data="tagTableData"
          :loading="loadingOsmData"
          :ui="{
            td: 'py-2',
            root: 'relative overflow-auto',
            base: 'overflow-clip',
            tbody: 'isolate',
          }"
          class="max-h-[400px] w-full min-w-0 flex-1 rounded-md border border-default"
        ></UTable>
      </div>
      <USeparator />
    </div>
    <div>
      <div class="p-4">
        <h5 class="pb-2">Feature Properties</h5>
        <UTable
          sticky
          :data="tableData"
          :ui="{
            td: 'py-2',
            root: 'relative overflow-auto',
            base: 'overflow-clip',
            tbody: 'isolate',
          }"
          class="max-h-[400px] w-full min-w-0 flex-1 rounded-md border border-default"
        ></UTable>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watchEffect } from 'vue'
import type { GeoJSONFeature } from 'maplibre-gl'
import { computedAsync } from '@vueuse/core'
import { getOsmData } from '@/external/endpoints.ts'
import type { OsmId } from '@/utils/osm.js'

const props = defineProps<{
  osm_id?: OsmId
  feature?: GeoJSONFeature
}>()

const loadingOsmData = ref(false)

const osmData = computedAsync(
  async () => (props.osm_id ? getOsmData(props.osm_id) : null),
  null,
  loadingOsmData,
)

watchEffect(() => {
  console.log('Fetched osm data: {}', osmData.value)
})

const tableData = computed(() => {
  return Object.entries(props.feature?.properties ?? {}).map(([key, value]) => ({ key, value }))
})

const tagTableData = computed(() => {
  return Object.entries(osmData.value?.tags ?? {}).map(([key, value]) => ({ key, value }))
})
</script>
