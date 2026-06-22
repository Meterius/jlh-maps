<template>
  <InterpretedFeatureProperties
    :feature="selection.feature"
    :osm-data="osmData"
    :loading="loadingOsmData"
  />

  <USeparator />

  <div v-if="selection.osmId" class="min-w-0 w-full">
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
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import type { OsmData } from '@/external/endpoints.ts'
import type { OsmSelectionItem } from '@/views/map-view/map-selection.ts'
import InterpretedFeatureProperties from '@/views/map-view/map-slideover/details/InterpretedFeatureProperties.vue'

const props = defineProps<{
  selection: OsmSelectionItem
  osmData: OsmData | null
  loadingOsmData: boolean
}>()

const osmTagsOpen = ref(false)

const tableUi = {
  td: 'py-2 align-top whitespace-pre-wrap',
  root: 'relative block min-w-0 max-w-full overflow-auto',
  base: 'w-max min-w-full',
  tbody: 'isolate',
}

const tagTableData = computed(() => {
  return props.loadingOsmData ? undefined : makeRawTableData(props.osmData?.tags)
})

function makeRawTableData(data: object | null | undefined) {
  return Object.entries(data ?? {}).map(([key, value]) => ({
    key,
    value: formatRawValue(value),
  }))
}

function formatRawValue(value: unknown): string {
  if (value === null || value === undefined) {
    return ''
  }

  if (typeof value === 'object') {
    return JSON.stringify(value, null, 2)
  }

  return String(value)
}
</script>
