<template>
  <div v-if="selection.stopRef" class="min-w-0 w-full">
    <div class="w-full p-4">
      <UCollapsible v-model:open="gtfsStopOpen">
        <UButton
          class="px-0 cursor-pointer"
          block
          color="neutral"
          variant="link"
          label="GTFS Stop"
          trailing-icon="lucide:chevron-down"
        />

        <template #content>
          <div class="grid gap-4 pt-2">
            <UTable
              v-if="loadingGtfsStop"
              sticky
              :data="undefined"
              :loading="loadingGtfsStop"
              :ui="tableUi"
              class="max-h-[400px] w-full rounded-md border border-default"
            ></UTable>

            <div v-for="table in gtfsStopTables" v-else :key="table.key" class="min-w-0">
              <h2 class="truncate px-1 pb-2 text-sm font-semibold text-highlighted">
                {{ table.title }}
              </h2>

              <UTable
                sticky
                :data="table.data"
                :ui="tableUi"
                class="max-h-[400px] w-full rounded-md border border-default"
              ></UTable>
            </div>
          </div>
        </template>
      </UCollapsible>
    </div>

    <USeparator />
  </div>

  <div v-if="selection.stopRef" class="min-w-0 w-full">
    <div class="w-full p-4">
      <UCollapsible v-model:open="gtfsRoutesOpen">
        <UButton
          class="px-0 cursor-pointer"
          block
          color="neutral"
          variant="link"
          label="GTFS Routes"
          trailing-icon="lucide:chevron-down"
        />

        <template #content>
          <div class="grid gap-4 pt-2">
            <UTable
              v-if="loadingGtfsStop || loadingGtfsRoutes"
              sticky
              :data="undefined"
              :loading="loadingGtfsStop || loadingGtfsRoutes"
              :ui="tableUi"
              class="max-h-[400px] w-full rounded-md border border-default"
            ></UTable>

            <div v-for="table in gtfsRouteTables" v-else :key="table.key" class="min-w-0">
              <h2 class="truncate px-1 pb-2 text-sm font-semibold text-highlighted">
                {{ table.title }}
              </h2>

              <UTable
                sticky
                :data="table.data"
                :ui="tableUi"
                class="max-h-[400px] w-full rounded-md border border-default"
              ></UTable>
            </div>
          </div>
        </template>
      </UCollapsible>
    </div>

    <USeparator />
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import type { GtfsAggregatedStop, GtfsRoute } from '@/external/endpoints.ts'
import type { GtfsStopSelectionItem } from '@/views/map-view/map-selection.ts'

const props = defineProps<{
  selection: GtfsStopSelectionItem
  gtfsStop: GtfsAggregatedStop | null
  gtfsRoutes: GtfsRoute[]
  loadingGtfsStop: boolean
  loadingGtfsRoutes: boolean
}>()

const gtfsStopOpen = ref(false)
const gtfsRoutesOpen = ref(false)

const tableUi = {
  td: 'py-2 align-top whitespace-pre-wrap',
  root: 'relative block min-w-0 max-w-full overflow-auto',
  base: 'w-max min-w-full',
  tbody: 'isolate',
}

type RawTableRow = {
  key: string
  value: string
}

type RawDataTable = {
  key: string
  title: string
  data: RawTableRow[]
}

const gtfsStopTables = computed(() => {
  if (props.loadingGtfsStop || !props.gtfsStop) {
    return []
  }

  return makeGtfsStopTables(props.gtfsStop)
})

const gtfsRouteTables = computed(() => {
  if (props.loadingGtfsStop || props.loadingGtfsRoutes) {
    return []
  }

  return props.gtfsRoutes.map((route) => ({
    key: `route:${route.version_id}:${route.route_id}`,
    title: getGtfsRouteTitle(route),
    data: makeRawTableData(route),
  }))
})

function makeGtfsStopTables(stop: GtfsAggregatedStop): RawDataTable[] {
  const tables: RawDataTable[] = []
  const stack: GtfsAggregatedStop[] = [stop]

  while (stack.length > 0) {
    const current = stack.shift()
    if (!current) continue

    tables.push({
      key: `stop:${current.version_id}:${current.stop_id}`,
      title: getGtfsStopTitle(current),
      data: makeRawTableData(current, ['children']),
    })

    stack.push(...current.children)
  }

  return tables
}

function getGtfsStopTitle(stop: GtfsAggregatedStop): string {
  return stop.stop_name || stop.platform_code || stop.stop_code || stop.stop_id || 'GTFS Stop'
}

function getGtfsRouteTitle(route: GtfsRoute): string {
  const title = [route.route_short_name, route.route_long_name].filter(Boolean).join(' - ')

  return title || route.route_id || 'GTFS Route'
}

function makeRawTableData(data: object | null | undefined, excludedKeys: string[] = []) {
  const excludedKeySet = new Set(excludedKeys)

  return Object.entries(data ?? {})
    .filter(([key]) => !excludedKeySet.has(key))
    .map(([key, value]) => ({
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
