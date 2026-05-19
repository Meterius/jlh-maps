<template>
  <UDrawer
    :active-snap-point="drawerActiveSnapPoint"
    :direction="drawerDirection"
    :handle="isMobileDrawer"
    :handle-only="isMobileDrawer"
    :snap-points="drawerSnapPoints"
    :modal="false"
    :overlay="false"
    :dismissible="isMobileDrawer"
    :open="props.open"
    :ui="{
      content: drawerContentClass,
    }"
    @drag="handleDrawerDrag"
    @release="handleDrawerRelease"
    @update:open="handleDrawerOpenUpdate"
    @update:active-snap-point="drawerRequestedSnapPoint = $event"
  >
    <template #content>
      <div
        ref="drawerContent"
        :class="['relative flex min-h-0 w-full flex-col', drawerPanelClass]"
        :style="drawerPanelStyle"
      >
        <div class="grid min-h-14 items-center p-4 pe-14">
          <div>
            <h1 class="truncate font-semibold">{{ title }}</h1>
          </div>
          <div>
            <UButton
              v-if="!isMobileDrawer"
              class="absolute right-3 top-3 z-30 rounded-full cursor-pointer"
              icon="material-symbols:close-rounded"
              title="Close"
              aria-label="Close"
              variant="ghost"
              color="neutral"
              size="md"
              square
              :ui="{ leadingIcon: 'size-6' }"
              @click="emit('update:open', false)"
            />
          </div>
        </div>

        <USeparator class="absolute inset-x-0 top-14 z-20" />

        <div class="grid min-h-0 flex-1 grid-rows-[minmax(0,1fr)] overflow-hidden">
          <ContentDirections
            v-show="props.active === 'directions'"
            class="h-full"
            :stops="props.directionStops"
            @update:stops="emit('update:direction-stops', $event)"
            @update:trip-primary="emit('update:trip-primary', $event)"
            @update:trip-alternates="emit('update:trip-alternates', $event)"
            @focus-trip="emit('focus-trip', $event)"
          />

          <ContentDetails
            v-show="props.active === 'details'"
            class="h-full"
            :osm_id="props.detailsOsmId"
            :feature="props.detailsFeature"
          />

          <ContentSettings
            v-if="props.map"
            v-show="props.active === 'settings'"
            class="h-full"
            :map="props.map"
            :bevy-settings="props.bevySettings"
            :bevy-camera-settings="props.bevyCameraSettings"
          />
        </div>
      </div>
    </template>
  </UDrawer>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useMediaQuery } from '@vueuse/core'
import type { GeoJSONFeature, Map as MapLibreMap } from 'maplibre-gl'
import type { Trip } from 'valhalla_client'
import type {
  MapViewCameraSettings as MapViewCameraSettingsBevy,
  MapViewSettings as MapViewSettingsBevy,
} from 'jlh_maps_app'
import type { GeoLocation } from '@/components/types.ts'
import type { OsmId } from '@/utils/osm.ts'
import ContentDetails from '@/components/map-slideover/ContentDetails.vue'
import ContentDirections from '@/components/map-slideover/ContentDirections.vue'
import ContentSettings from '@/components/map-slideover/ContentSettings.vue'

export type MapSlideoverTab = 'details' | 'directions' | 'settings'

const props = defineProps<{
  open: boolean
  active: MapSlideoverTab | null
  directionStops: (GeoLocation | null)[]
  detailsOsmId?: OsmId
  detailsFeature?: GeoJSONFeature
  map?: MapLibreMap
  bevySettings: MapViewSettingsBevy
  bevyCameraSettings: MapViewCameraSettingsBevy
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
  'update:direction-stops': [value: (GeoLocation | null)[]]
  'update:trip-primary': [value: Trip | null]
  'update:trip-alternates': [value: Trip[]]
  'focus-trip': [value: Trip]
}>()

const MOBILE_DRAWER_INITIAL_SNAP = 0.35
const MOBILE_DRAWER_SNAP_POINTS = [MOBILE_DRAWER_INITIAL_SNAP, 0.7]

const drawerContent = ref<HTMLElement | null>(null)
const isMobileDrawer = useMediaQuery(
  '(max-width: 767px), (pointer: coarse) and (max-height: 600px)',
)
const drawerDirection = computed(() => (isMobileDrawer.value ? 'bottom' : 'left'))
const drawerSnapPoints = computed(() =>
  isMobileDrawer.value ? MOBILE_DRAWER_SNAP_POINTS : undefined,
)
const drawerRequestedSnapPoint = ref<number | string>(MOBILE_DRAWER_INITIAL_SNAP)
const drawerWasDragged = ref(false)
const drawerActiveSnapPoint = computed(() =>
  isMobileDrawer.value ? drawerRequestedSnapPoint.value : undefined,
)
const drawerContentClass = computed(() =>
  isMobileDrawer.value ? 'max-h-[100dvh]' : '!left-0 w-125 max-w-[100vw]',
)
const drawerPanelClass = computed(() => (isMobileDrawer.value ? '' : 'h-full'))
const drawerPanelStyle = computed(() =>
  isMobileDrawer.value
    ? {
        height: `calc(${getSnapPointViewportHeight(drawerRequestedSnapPoint.value)}dvh - 1.5rem)`,
      }
    : undefined,
)

const title = computed(() => {
  switch (props.active) {
    case 'details':
      return props.detailsFeature?.properties?.name ?? 'Location Details'
    case 'settings':
      return 'Map Settings'
    case 'directions':
    default:
      return 'Directions'
  }
})

watch(
  () => props.open,
  (open) => {
    if (open && isMobileDrawer.value) {
      drawerRequestedSnapPoint.value = MOBILE_DRAWER_INITIAL_SNAP
    }
  },
)

watch(isMobileDrawer, (mobile) => {
  if (mobile) {
    drawerRequestedSnapPoint.value = MOBILE_DRAWER_INITIAL_SNAP
  }
})

const getSnapPointViewportHeight = (snapPoint: number | string) => {
  if (typeof snapPoint === 'number' && Number.isFinite(snapPoint)) {
    return Math.max(0, Math.min(100, snapPoint * 100))
  }

  const snapPointText = String(snapPoint)
  const parsedSnapPoint = Number.parseFloat(snapPointText)
  if (!Number.isFinite(parsedSnapPoint)) return MOBILE_DRAWER_INITIAL_SNAP * 100

  return Math.max(
    0,
    Math.min(100, snapPointText.endsWith('%') ? parsedSnapPoint : parsedSnapPoint * 100),
  )
}

const handleDrawerDrag = () => {
  if (isMobileDrawer.value) {
    drawerWasDragged.value = true
  }
}

const handleDrawerRelease = () => {
  window.setTimeout(() => {
    drawerWasDragged.value = false
  })
}

const handleDrawerOpenUpdate = (open: boolean) => {
  if (open) {
    emit('update:open', true)
    return
  }

  if (isMobileDrawer.value && drawerWasDragged.value) {
    emit('update:open', false)
  }
}

const getCoveredWidth = () => {
  if (drawerDirection.value !== 'left') return 0
  if (props.active !== 'directions') return 0

  const rect = drawerContent.value?.getBoundingClientRect()
  return rect ? Math.max(0, rect.right) : 0
}

const getCoveredHeight = () => {
  if (drawerDirection.value !== 'bottom') return 0
  if (props.active !== 'directions') return 0

  const rect = drawerContent.value?.getBoundingClientRect()
  return rect ? Math.max(0, window.innerHeight - rect.top) : 0
}

const getRouteFitPadding = () => {
  const basePadding = 80
  const slideoverWidth = getCoveredWidth()
  const slideoverHeight = getCoveredHeight()

  return {
    top: basePadding,
    right: basePadding,
    bottom: slideoverHeight > 0 ? Math.ceil(slideoverHeight + 32) : basePadding,
    left: slideoverWidth > 0 ? Math.ceil(slideoverWidth + 32) : basePadding,
  }
}

defineExpose({
  getRouteFitPadding,
})
</script>
