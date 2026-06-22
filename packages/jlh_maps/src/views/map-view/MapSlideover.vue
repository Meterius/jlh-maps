<template>
  <UDrawer
    :active-snap-point="drawerRequestedSnapPoint"
    :direction="drawerDirection"
    :handle="false"
    :handle-only="isMobileDrawer"
    :snap-points="drawerSnapPoints"
    :modal="false"
    :overlay="false"
    :dismissible="isMobileDrawer"
    :content="drawerContentProps"
    :open="props.open"
    :ui="{
      content: drawerContentClass,
    }"
    @update:open="emit('update:open', $event)"
    @update:active-snap-point="drawerRequestedSnapPoint = $event"
  >
    <template #content>
      <div
        ref="drawerContent"
        :class="['relative flex min-h-0 w-full flex-col select-text', drawerPanelClass]"
        :style="drawerPanelStyle"
      >
        <div class="relative z-20">
          <!-- Custom handle that spans header to make dragging more reliable -->
          <div
            :class="[
              'relative grid min-h-14 min-w-0 items-center p-4',
              isMobileDrawer ? 'cursor-grab touch-none select-none active:cursor-grabbing' : '',
            ]"
          >
            <DrawerHandle
              v-if="isMobileDrawer"
              class="!absolute inset-0 z-10 !m-0 !h-auto !w-auto !rounded-none !bg-transparent !opacity-100 !touch-none"
              prevent-cycle
            />
            <div
              v-if="isMobileDrawer"
              class="pointer-events-none absolute left-1/2 top-2 z-20 h-1.5 w-10 -translate-x-1/2 rounded-full bg-neutral-300 dark:bg-neutral-600"
            />

            <div
              :class="[
                'relative z-20 grid min-w-0 items-center gap-2',
                headerGridClass,
                isMobileDrawer ? 'pointer-events-none pt-2' : '',
              ]"
            >
              <h1 class="truncate font-semibold">{{ title }}</h1>

              <UBadge
                v-if="headerBadge"
                class="min-w-0 max-w-full justify-self-end"
                :icon="headerBadge.iconName"
                color="info"
                variant="outline"
                :label="headerBadge.label"
              />

              <UButton
                v-if="!isMobileDrawer"
                class="z-30 rounded-full cursor-pointer justify-self-end"
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

          <USeparator />
        </div>

        <div
          class="grid min-h-0 min-w-0 w-full max-w-full flex-1 grid-rows-[minmax(0,1fr)] overflow-hidden"
        >
          <ContentDirections v-show="props.active === 'directions'" class="h-full" />

          <ContentDetails
            v-show="props.active === 'details'"
            class="h-full"
            :selection="props.detailsSelection"
            @update:badge="detailsBadge = $event"
          />

          <ContentSettings
            v-if="props.map"
            v-show="props.active === 'settings'"
            class="h-full"
            :map="props.map"
            :bevy-instance-id="props.bevyInstanceId"
          />
        </div>
      </div>
    </template>
  </UDrawer>
</template>

<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { useMediaQuery, useResizeObserver } from '@vueuse/core'
import { DrawerHandle } from 'vaul-vue'
import type { Map as MapLibreMap } from 'maplibre-gl'
import type { PoiDisplayMetadata } from '@/constants/osm-mapping.ts'
import ContentDetails from '@/views/map-view/map-slideover/ContentDetails.vue'
import ContentDirections from '@/views/map-view/map-slideover/ContentDirections.vue'
import ContentSettings from '@/views/map-view/map-slideover/ContentSettings.vue'
import type { SelectionItem } from '@/views/map-view/map-selection.ts'

export type MapSlideoverTab = 'details' | 'directions' | 'settings'

const props = defineProps<{
  open: boolean
  active: MapSlideoverTab | null
  detailsSelection?: SelectionItem
  map?: MapLibreMap
  bevyInstanceId?: string
}>()

const emit = defineEmits<{
  'update:open': [value: boolean]
  'update:drawer-size': [value: { width: number; height: number }]
  'update:drawer-direction': [value: 'left' | 'right' | 'top' | 'bottom']
}>()

const MOBILE_DRAWER_INITIAL_SNAP = 0.4
const MOBILE_DRAWER_SNAP_POINTS = [MOBILE_DRAWER_INITIAL_SNAP, 0.9]

const drawerContent = ref<HTMLElement | null>(null)
const detailsBadge = ref<PoiDisplayMetadata | null>(null)

const isMobileDrawer = useMediaQuery(
  '(max-width: 767px), (pointer: coarse) and (max-height: 600px)',
)

const drawerDirection = computed(() => (isMobileDrawer.value ? 'bottom' : 'left'))

watch(drawerDirection, (value) => emit('update:drawer-direction', value), {
  immediate: true,
})

const drawerSnapPoints = computed(() =>
  isMobileDrawer.value ? MOBILE_DRAWER_SNAP_POINTS : undefined,
)

const drawerRequestedSnapPoint = ref<number | string>(MOBILE_DRAWER_INITIAL_SNAP)

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

// prevent outside clicks from dismissing drawer, while allow dismissins drawer on
// downwards drag
const drawerContentProps = {
  onEscapeKeyDown: (event: Event) => {
    event.preventDefault()
  },
  onInteractOutside: (event: Event) => {
    event.preventDefault()
  },
  onPointerDownOutside: (event: Event) => {
    event.preventDefault()
  },
}

const title = computed(() => {
  switch (props.active) {
    case 'details':
      return getDetailsTitle()
    case 'settings':
      return 'Map Settings'
    case 'directions':
    default:
      return 'Directions'
  }
})

const getDetailsTitle = () => {
  const selection = props.detailsSelection
  const name = selection?.feature.properties?.name

  if (typeof name === 'string' && name) {
    return name
  }

  return selection?.label || 'Location Details'
}

const headerBadge = computed(() => (props.active === 'details' ? detailsBadge.value : null))
const headerGridClass = computed(() => {
  if (isMobileDrawer.value) {
    return headerBadge.value ? 'grid-cols-[minmax(0,1fr)_auto]' : 'grid-cols-[minmax(0,1fr)]'
  }

  return headerBadge.value
    ? 'grid-cols-[minmax(0,1fr)_auto_auto]'
    : 'grid-cols-[minmax(0,1fr)_auto]'
})

//

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

const getDrawerRootRect = () => {
  if (!props.open) return undefined

  return drawerContent.value?.getBoundingClientRect()
}

const getCoveredWidth = () => {
  if (drawerDirection.value !== 'left') return 0
  if (props.active !== 'directions') return 0

  const rect = getDrawerRootRect()
  return rect ? Math.max(0, rect.right) : 0
}

const getCoveredHeight = () => {
  if (drawerDirection.value !== 'bottom') return 0
  if (props.active !== 'directions') return 0

  const rect = getDrawerRootRect()
  return rect ? Math.max(0, window.innerHeight - rect.top) : 0
}

const emitSlideoverSize = () => {
  const rect = getDrawerRootRect()

  emit('update:drawer-size', {
    width: rect?.width ?? 0,
    height: rect?.height ?? 0,
  })
}

useResizeObserver(drawerContent, emitSlideoverSize)

watch(
  [() => props.open, drawerDirection, drawerRequestedSnapPoint],
  () => {
    nextTick(emitSlideoverSize)
  },
  { immediate: true },
)

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
