<template>
  <div class="grid w-full auto-rows-max content-start overflow-y-auto overflow-x-hidden">
    <ContentDetailsBodyOsm
      v-if="osmSelection"
      :selection="osmSelection"
      :osm-data="osmData"
      :loading-osm-data="loadingOsmData"
    />

    <ContentDetailsBodyGtfsStop
      v-else-if="gtfsStopSelection"
      :selection="gtfsStopSelection"
      :gtfs-stop="gtfsStop"
      :gtfs-routes="gtfsRoutes"
      :loading-gtfs-stop="loadingGtfsStop"
      :loading-gtfs-routes="loadingGtfsRoutes"
    />

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
                :data="featureTableData"
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
import { computed, ref, watch } from 'vue'
import { computedAsync } from '@vueuse/core'
import {
  getGtfsAggregatedStop,
  getGtfsRoute,
  getOsmData,
  type GtfsAggregatedStop,
  type GtfsRoute,
} from '@/external/endpoints.ts'
import {
  isOsmAmenityValue,
  OSM_AMENITY_METADATA,
  type PoiDisplayMetadata,
} from '@/constants/osm-mapping.ts'
import { isOmtPoiSubclass, OMT_POI_SUBCLASS_METADATA } from '@/constants/omt-mapping.ts'
import {
  GTFS_ROUTE_TYPE_ICON_MAP,
  GtfsRouteIconName,
  GtfsRouteType,
} from '@/maplibre-layers/gtfs-layer.ts'
import {
  SelectionItemKind,
  type GtfsStopSelectionItem,
  type OsmSelectionItem,
  type SelectionItem,
} from '@/views/map-view/map-selection.ts'
import ContentDetailsBodyGtfsStop from '@/views/map-view/map-slideover/details/ContentDetailsBodyGtfsStop.vue'
import ContentDetailsBodyOsm from '@/views/map-view/map-slideover/details/ContentDetailsBodyOsm.vue'

const props = defineProps<{
  selection?: SelectionItem
}>()

const emit = defineEmits<{
  'update:badge': [value: PoiDisplayMetadata | null]
}>()

const loadingOsmData = ref(false)
const loadingGtfsStop = ref(false)
const loadingGtfsRoutes = ref(false)
const featurePropertiesOpen = ref(false)

const tableUi = {
  td: 'py-2 align-top whitespace-pre-wrap',
  root: 'relative block min-w-0 max-w-full overflow-auto',
  base: 'w-max min-w-full',
  tbody: 'isolate',
}

const osmSelection = computed<OsmSelectionItem | undefined>(() =>
  props.selection?.kind === SelectionItemKind.Osm ? props.selection : undefined,
)

const gtfsStopSelection = computed<GtfsStopSelectionItem | undefined>(() =>
  props.selection?.kind === SelectionItemKind.GtfsStop ? props.selection : undefined,
)

const feature = computed(() => props.selection?.feature)

const featureTableData = computed(() => {
  return makeRawTableData(feature.value?.properties)
})

const osmData = computedAsync(
  async () => {
    const selectedOsmId = osmSelection.value?.osmId
    return selectedOsmId ? getOsmData(selectedOsmId) : null
  },
  null,
  loadingOsmData,
)

const gtfsStop = computedAsync(
  async () => {
    const selectedStop = gtfsStopSelection.value?.stopRef
    return selectedStop ? getGtfsAggregatedStop(selectedStop.versionId, selectedStop.stopId) : null
  },
  null,
  loadingGtfsStop,
)

const gtfsRoutes = computedAsync(
  async () => {
    const selectedStop = gtfsStopSelection.value?.stopRef
    const stop = gtfsStop.value

    if (
      !selectedStop ||
      !stop ||
      stop.version_id !== selectedStop.versionId ||
      stop.stop_id !== selectedStop.stopId
    ) {
      return []
    }

    const routes = await Promise.all(
      collectGtfsRouteIds(stop).map((routeId) => getGtfsRoute(stop.version_id, routeId)),
    )

    return routes.filter((route): route is GtfsRoute => route !== null)
  },
  [] as GtfsRoute[],
  loadingGtfsRoutes,
)

const GTFS_STATION_METADATA: Record<GtfsRouteIconName, PoiDisplayMetadata> = {
  [GtfsRouteIconName.Air]: {
    label: 'Air Station',
    iconName: 'lucide:plane',
  },
  [GtfsRouteIconName.Bus]: {
    label: 'Bus Station',
    iconName: 'lucide:bus-front',
  },
  [GtfsRouteIconName.Cable]: {
    label: 'Cable Station',
    iconName: 'lucide:cable-car',
  },
  [GtfsRouteIconName.Ferry]: {
    label: 'Ferry Station',
    iconName: 'lucide:ship',
  },
  [GtfsRouteIconName.Funicular]: {
    label: 'Funicular Station',
    iconName: 'lucide:mountain',
  },
  [GtfsRouteIconName.Generic]: {
    label: 'Transport Station',
    iconName: 'lucide:route',
  },
  [GtfsRouteIconName.Rail]: {
    label: 'Train Station',
    iconName: 'lucide:train-front',
  },
  [GtfsRouteIconName.Taxi]: {
    label: 'Taxi Station',
    iconName: 'lucide:car-taxi-front',
  },
  [GtfsRouteIconName.Tram]: {
    label: 'Tram Station',
    iconName: 'lucide:tram-front',
  },
}

const badge = computed<PoiDisplayMetadata | null>(() => {
  switch (props.selection?.kind) {
    case SelectionItemKind.Osm: {
      const osmAmenityTag = osmData.value?.tags['amenity']
      if (!loadingOsmData.value && osmAmenityTag && isOsmAmenityValue(osmAmenityTag)) {
        return OSM_AMENITY_METADATA[osmAmenityTag]
      }

      const featureSubclassTag = feature.value?.properties?.['subclass']
      if (typeof featureSubclassTag === 'string' && isOmtPoiSubclass(featureSubclassTag)) {
        return OMT_POI_SUBCLASS_METADATA[featureSubclassTag]
      }

      return null
    }

    case SelectionItemKind.GtfsStop:
      return getGtfsStationBadge()
  }

  return null
})

function collectGtfsRouteIds(stop: GtfsAggregatedStop): string[] {
  const routeIds = new Set<string>()
  const stack = [stop]

  while (stack.length > 0) {
    const current = stack.pop()
    if (!current) continue

    current.route_ids.forEach((routeId) => routeIds.add(routeId))
    stack.push(...current.children)
  }

  return [...routeIds].sort((a, b) => a.localeCompare(b))
}

function getGtfsStationBadge(): PoiDisplayMetadata {
  return (
    getGtfsRouteTypeBadge(getFeatureGtfsRouteTypes()) ??
    getGtfsRouteTypeBadge(
      gtfsRoutes.value
        .map((route) => normalizeGtfsRouteType(route.route_type))
        .filter((routeType): routeType is GtfsRouteType => routeType !== null),
    ) ??
    GTFS_STATION_METADATA[GtfsRouteIconName.Generic]
  )
}

function getFeatureGtfsRouteTypes(): GtfsRouteType[] {
  const routeTypes = feature.value?.properties?.['route_types']

  if (typeof routeTypes === 'string') {
    return routeTypes
      .split(',')
      .map((routeType) => normalizeGtfsRouteType(routeType.trim()))
      .filter((routeType): routeType is GtfsRouteType => routeType !== null)
  }

  const routeType = normalizeGtfsRouteType(routeTypes)

  return routeType ? [routeType] : []
}

function getGtfsRouteTypeBadge(routeTypes: GtfsRouteType[]): PoiDisplayMetadata | null {
  const iconNames = [...new Set(routeTypes.map((routeType) => GTFS_ROUTE_TYPE_ICON_MAP[routeType]))]
  const iconName = iconNames[0]

  return iconNames.length === 1 && iconName ? GTFS_STATION_METADATA[iconName] : null
}

function normalizeGtfsRouteType(value: unknown): GtfsRouteType | null {
  const normalized = typeof value === 'number' ? String(value) : value

  return typeof normalized === 'string' &&
    Object.values(GtfsRouteType).includes(normalized as GtfsRouteType)
    ? (normalized as GtfsRouteType)
    : null
}

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

watch(badge, (value) => emit('update:badge', value), { immediate: true })
</script>
