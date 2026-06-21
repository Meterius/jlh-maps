<template>
  <div v-if="cursor" class="pointer-events-none absolute inset-0 z-20">
    <div
      class="feature-inspector-crosshair"
      :style="{ left: `${cursor.point.x}px`, top: `${cursor.point.y}px` }"
    >
      <span class="feature-inspector-crosshair-line feature-inspector-crosshair-line-x" />
      <span class="feature-inspector-crosshair-line feature-inspector-crosshair-line-y" />
    </div>

    <UCard
      variant="subtle"
      :class="[
        'absolute max-h-80 w-80 overflow-hidden text-xs shadow-xl backdrop-blur',
        cursor.pinned ? 'pointer-events-auto' : 'pointer-events-none',
      ]"
      :style="cardStyle"
      :ui="{ body: '!p-0 sm:!p-0' }"
      @click.stop
    >
      <div class="grid gap-1 border-b border-default px-3 py-2">
        <div class="flex min-w-0 items-center justify-between gap-3">
          <span class="min-w-0 truncate font-semibold text-highlighted">Feature Inspector</span>
          <div class="flex shrink-0 items-center gap-1">
            <UBadge
              :label="cursor.pinned ? 'Pinned' : 'Cursor'"
              :color="cursor.pinned ? 'primary' : 'neutral'"
              variant="soft"
              size="sm"
            />
            <UBadge :label="cursor.features.length.toString()" color="neutral" variant="soft" />
            <UButton
              color="neutral"
              variant="ghost"
              size="xs"
              icon="lucide:x"
              title="Disable feature inspector"
              aria-label="Disable feature inspector"
              class="pointer-events-auto cursor-pointer"
              @pointerdown.stop
              @click.stop="dismissFeatureInspector"
            />
          </div>
        </div>
        <div class="grid grid-cols-2 gap-2 font-mono text-[11px] text-muted">
          <span>lat {{ cursor.lngLat.lat.toFixed(6) }}</span>
          <span>lon {{ cursor.lngLat.lng.toFixed(6) }}</span>
        </div>
      </div>

      <div class="max-h-64 overflow-auto px-2 py-2">
        <UAlert
          v-if="cursor.features.length === 0"
          color="neutral"
          variant="subtle"
          title="No rendered features"
          icon="lucide:scan-search"
          :ui="{ root: 'py-2', title: 'text-xs font-normal' }"
        />

        <ol v-else class="grid gap-2">
          <li
            v-for="(feature, idx) in cursor.features"
            :key="`${idx}:${feature.layerId}:${feature.id ?? ''}`"
            class="grid gap-1 rounded-md border border-default/70 bg-elevated/70 px-2 py-1.5"
          >
            <div class="flex min-w-0 items-center justify-between gap-2">
              <span class="min-w-0 truncate font-medium">{{ feature.layerId }}</span>
              <UBadge
                :label="feature.geometryType"
                color="neutral"
                variant="outline"
                size="sm"
                class="shrink-0 font-mono text-[10px]"
              />
            </div>

            <div class="grid gap-0.5 text-[11px] text-muted">
              <div class="min-w-0 truncate">
                <span>{{ feature.source }}</span>
                <span v-if="feature.sourceLayer"> / {{ feature.sourceLayer }}</span>
              </div>
              <div v-if="feature.id !== undefined" class="min-w-0 truncate font-mono">
                id {{ feature.id }}
              </div>
              <div v-if="feature.title" class="min-w-0 truncate text-default">
                {{ feature.title }}
              </div>
            </div>

            <USeparator v-if="feature.properties.length" class="my-0.5" />

            <dl v-if="feature.properties.length" class="mt-1 grid gap-0.5 font-mono text-[10px]">
              <div
                v-for="property in feature.properties"
                :key="property.key"
                class="grid grid-cols-[minmax(0,0.45fr)_minmax(0,0.55fr)] gap-2"
              >
                <dt class="truncate text-muted">{{ property.key }}</dt>
                <dd class="truncate text-default">{{ property.value }}</dd>
              </div>
            </dl>
          </li>
        </ol>

        <UBadge
          v-if="cursor.hiddenFeatureCount > 0"
          :label="`+${cursor.hiddenFeatureCount} more`"
          color="neutral"
          variant="soft"
          size="sm"
          class="mt-2"
        />
      </div>
    </UCard>
  </div>
</template>

<script setup lang="ts">
import type { Map as MapLibreMap, MapGeoJSONFeature, MapMouseEvent } from 'maplibre-gl'
import { computed, onScopeDispose, shallowRef } from 'vue'
import { onMapEvent } from '@/composables/maplibre'
import { useMapViewStoreOrThrow } from '@/views/map-view/map-view-store.ts'

// TODO: Refactor/double check implementation

const props = defineProps<{
  map: MapLibreMap
}>()

const { mapViewStore } = useMapViewStoreOrThrow()

const MAX_VISIBLE_FEATURES = 16
const MAX_VISIBLE_PROPERTIES = 6
const CARD_WIDTH_PX = 320
const CARD_MAX_HEIGHT_PX = 320
const CARD_OFFSET_PX = 14
const TITLE_PROPERTY_KEYS = [
  'name',
  'stop_name',
  'route_short_name',
  'route_long_name',
  'class',
  'subclass',
  'type',
  'kind',
]

type InspectorCursor = {
  pinned: boolean
  point: {
    x: number
    y: number
  }
  viewport: {
    width: number
    height: number
  }
  lngLat: {
    lng: number
    lat: number
  }
  features: InspectedFeature[]
  hiddenFeatureCount: number
}

type InspectedFeature = {
  layerId: string
  source: string
  sourceLayer?: string
  geometryType: string
  id?: string | number
  title?: string
  properties: InspectedFeatureProperty[]
}

type InspectedFeatureProperty = {
  key: string
  value: string
}

const cursor = shallowRef<InspectorCursor | null>(null)
let pinnedLocation: { lng: number; lat: number } | null = null

const cardStyle = computed(() => {
  const currentCursor = cursor.value
  if (!currentCursor) return undefined

  const placeRight =
    currentCursor.point.x + CARD_WIDTH_PX + CARD_OFFSET_PX <= currentCursor.viewport.width
  const placeBelow =
    currentCursor.point.y + CARD_MAX_HEIGHT_PX + CARD_OFFSET_PX <= currentCursor.viewport.height

  return {
    left: `${currentCursor.point.x + (placeRight ? CARD_OFFSET_PX : -CARD_OFFSET_PX)}px`,
    top: `${currentCursor.point.y + (placeBelow ? CARD_OFFSET_PX : -CARD_OFFSET_PX)}px`,
    transform: `translate(${placeRight ? '0' : '-100%'}, ${placeBelow ? '0' : '-100%'})`,
  }
})

let animationFrameId: number | undefined
let pendingCursorFactory: (() => InspectorCursor | null) | undefined

onMapEvent(props.map, 'mousemove', (event) => {
  if (pinnedLocation) return

  scheduleCursorUpdate(() => makeInspectorCursorFromEvent(props.map, event, false))
})

onMapEvent(props.map, 'click', (event) => {
  if (pinnedLocation) {
    pinnedLocation = null
    clearScheduledCursorUpdate()
    cursor.value = null
    return
  }

  pinnedLocation = {
    lng: event.lngLat.lng,
    lat: event.lngLat.lat,
  }

  schedulePinnedCursorUpdate(props.map)
})

onMapEvent(props.map, 'move', () => {
  if (!pinnedLocation) return

  schedulePinnedCursorUpdate(props.map)
})

onMapEvent(props.map, 'mouseout', () => {
  if (pinnedLocation) return

  clearScheduledCursorUpdate()
  cursor.value = null
})

onScopeDispose(() => {
  pinnedLocation = null
  clearScheduledCursorUpdate()
  cursor.value = null
})

function dismissFeatureInspector() {
  pinnedLocation = null
  clearScheduledCursorUpdate()
  cursor.value = null
  mapViewStore.value.featureInspectorEnabled = false
}

function schedulePinnedCursorUpdate(map: MapLibreMap) {
  const location = pinnedLocation
  if (!location) return

  scheduleCursorUpdate(() => makeInspectorCursorFromLngLat(map, location, true))
}

function scheduleCursorUpdate(cursorFactory: () => InspectorCursor | null) {
  pendingCursorFactory = cursorFactory

  if (animationFrameId !== undefined) return

  animationFrameId = window.requestAnimationFrame(() => {
    animationFrameId = undefined
    const factory = pendingCursorFactory
    pendingCursorFactory = undefined

    if (!factory) return

    cursor.value = factory()
  })
}

function clearScheduledCursorUpdate() {
  pendingCursorFactory = undefined

  if (animationFrameId !== undefined) {
    window.cancelAnimationFrame(animationFrameId)
    animationFrameId = undefined
  }
}

function makeInspectorCursorFromEvent(
  map: MapLibreMap,
  event: MapMouseEvent,
  pinned: boolean,
): InspectorCursor {
  return makeInspectorCursor(map, event.point, event.lngLat, pinned)
}

function makeInspectorCursorFromLngLat(
  map: MapLibreMap,
  lngLat: { lng: number; lat: number },
  pinned: boolean,
): InspectorCursor {
  return makeInspectorCursor(map, map.project([lngLat.lng, lngLat.lat]), lngLat, pinned)
}

function makeInspectorCursor(
  map: MapLibreMap,
  point: MapMouseEvent['point'],
  lngLat: { lng: number; lat: number },
  pinned: boolean,
): InspectorCursor {
  const canvas = map.getCanvas()
  const features = queryRenderedFeatures(map, point)
  const visibleFeatures = features.slice(0, MAX_VISIBLE_FEATURES).map(makeInspectedFeature)

  return {
    pinned,
    point: {
      x: point.x,
      y: point.y,
    },
    viewport: {
      width: canvas.clientWidth,
      height: canvas.clientHeight,
    },
    lngLat: {
      lng: lngLat.lng,
      lat: lngLat.lat,
    },
    features: visibleFeatures,
    hiddenFeatureCount: Math.max(0, features.length - visibleFeatures.length),
  }
}

function queryRenderedFeatures(
  map: MapLibreMap,
  point: MapMouseEvent['point'],
): MapGeoJSONFeature[] {
  try {
    return map.queryRenderedFeatures(point)
  } catch (error) {
    console.warn('Failed to query rendered features for inspector', error)
    return []
  }
}

function makeInspectedFeature(feature: MapGeoJSONFeature): InspectedFeature {
  return {
    layerId: feature.layer.id,
    source: feature.source,
    sourceLayer: feature.sourceLayer,
    geometryType: feature.geometry.type,
    id: feature.id,
    title: getFeatureTitle(feature),
    properties: getFeatureProperties(feature.properties),
  }
}

function getFeatureTitle(feature: MapGeoJSONFeature): string | undefined {
  for (const key of TITLE_PROPERTY_KEYS) {
    const value = feature.properties[key]
    if (value !== undefined && value !== null && value !== '') {
      return formatPropertyValue(value)
    }
  }

  return undefined
}

function getFeatureProperties(
  properties: MapGeoJSONFeature['properties'],
): InspectedFeatureProperty[] {
  return Object.entries(properties)
    .filter(([, value]) => value !== undefined && value !== null && value !== '')
    .slice(0, MAX_VISIBLE_PROPERTIES)
    .map(([key, value]) => ({
      key,
      value: formatPropertyValue(value),
    }))
}

function formatPropertyValue(value: unknown): string {
  switch (typeof value) {
    case 'string':
      return value
    case 'number':
    case 'boolean':
    case 'bigint':
      return value.toString()
    default:
      return JSON.stringify(value) ?? String(value)
  }
}
</script>

<style scoped>
.feature-inspector-crosshair {
  position: absolute;
  width: 20px;
  height: 20px;
  transform: translate(-50%, -50%);
  filter: drop-shadow(0 1px 2px rgb(0 0 0 / 0.35));
}

.feature-inspector-crosshair-line {
  position: absolute;
  background: rgb(20 184 166);
}

.feature-inspector-crosshair-line-x {
  left: 0;
  right: 0;
  top: 50%;
  height: 1px;
}

.feature-inspector-crosshair-line-y {
  left: 50%;
  top: 0;
  bottom: 0;
  width: 1px;
}
</style>
