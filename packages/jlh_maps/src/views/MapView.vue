<template>
  <div style="position: absolute; left: 0; right: 0; top: 0; bottom: 0">
    <div
      :style="`position: absolute; width: 100%; height: ${showBevyCanvas ? '50%' : '100%'}; top: 0`"
    >
      <mgl-map
        :map-key="mapKey"
        :center="[13.35203105083487, 52.499757263332086]"
        :zoom="14"
        :canvas-context-attributes="{ antialias: true }"
        @map:contextmenu="onMapContextMenu"
      >
        <mgl-custom-control position="top-right">
          <button
            class="map-custom-control"
            type="button"
            title="Map settings"
            aria-label="Map settings"
            @click="slideoverOpen = SlideoverTab.Settings"
          >
            <UIcon
              name="material-symbols:settings-outline-rounded"
              class="size-6"
              style="margin: auto"
            />
          </button>
        </mgl-custom-control>

        <mgl-custom-control position="top-right">
          <button
            class="map-custom-control"
            type="button"
            title="Navigation"
            aria-label="Navigation"
            @click="slideoverOpen = SlideoverTab.Directions"
          >
            <UIcon
              name="material-symbols:signpost-outline-rounded"
              class="size-6"
              style="margin: auto"
            />
          </button>
        </mgl-custom-control>

        <mgl-custom-control position="top-right">
          <button
            class="map-custom-control"
            type="button"
            title="Show bevy"
            aria-label="Show bevy"
            :aria-pressed="showBevyCanvas"
            @click="showBevyCanvas = !showBevyCanvas"
          >
            <UIcon
              name="material-symbols:bug-report-outline-rounded"
              :class="['size-6', ...[showBevyCanvas ? ['text-secondary'] : []]]"
              style="margin: auto"
            />
          </button>
        </mgl-custom-control>
      </mgl-map>

      <div
        class="pointer-events-none absolute z-10 flex gap-2 p-2 transition-[bottom,left] duration-200 ease-out"
        :style="layersControlStyle"
      >
        <UPopover modal>
          <UButton
            color="neutral"
            variant="outline"
            size="xl"
            class="pointer-events-auto cursor-pointer"
            icon="lucide:layers"
            title="Layers"
            aria-label="Layers"
          />

          <template #content>
            <UCard :ui="{ body: '!p-2 grid w-72 max-w-[calc(100vw-1rem)] gap-2' }">
              <div class="grid grid-cols-2 gap-1">
                <UButton
                  label="Shadows"
                  :color="mapViewSettings.enable_shadows ? 'primary' : 'neutral'"
                  variant="outline"
                  size="md"
                  class="cursor-pointer"
                  icon="lucide:sunset"
                  @click="mapViewSettings.enable_shadows = !mapViewSettings.enable_shadows"
                />

                <UButton
                  label="3D Buildings"
                  :color="mapViewSettings.enable_buildings ? 'primary' : 'neutral'"
                  variant="outline"
                  size="md"
                  class="cursor-pointer"
                  icon="lucide:building"
                  @click="mapViewSettings.enable_buildings = !mapViewSettings.enable_buildings"
                />

                <UButton
                  label="Terrain"
                  :color="terrainEnabled ? 'primary' : 'neutral'"
                  variant="outline"
                  size="md"
                  class="cursor-pointer"
                  icon="lucide:mountain"
                  @click="terrainEnabled = !terrainEnabled"
                />

                <UButton
                  label="Dark Theme"
                  :color="darkThemeEnabled ? 'primary' : 'neutral'"
                  variant="outline"
                  size="md"
                  class="cursor-pointer"
                  :icon="darkThemeEnabled ? 'lucide:moon' : 'lucide:sun'"
                  @click="toggleDarkTheme"
                />
              </div>

              <USeparator />

              <div class="grid min-w-0 gap-1">
                <div class="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] gap-1">
                  <UButton
                    label="Rainfall"
                    :color="rainfallEnabled ? 'primary' : 'neutral'"
                    variant="outline"
                    size="md"
                    class="min-w-0 cursor-pointer justify-start"
                    icon="lucide:cloud-rain"
                    @click="rainfallEnabled = !rainfallEnabled"
                  />

                  <UButton
                    :disabled="!rainfallEnabled"
                    color="neutral"
                    variant="outline"
                    size="md"
                    class="shrink-0 cursor-pointer"
                    icon="lucide:refresh-cw"
                    title="Refresh rainfall data"
                    aria-label="Refresh rainfall data"
                    :loading="rainfallRasterLoading"
                    @click.stop="refreshRainfallRasterData"
                  />
                </div>

                <p v-if="rainfallEnabled" class="min-w-0 truncate px-1 text-xs text-muted">
                  {{ rainfallRasterDataTimeLabel }}
                </p>
              </div>

              <USeparator />

              <ModeSelector
                :options="baseStyleTypeOptions"
                :ui="{ root: 'min-w-0', button: 'py-2' }"
                v-model="baseStyleType"
              />
            </UCard>
          </template>
        </UPopover>

        <UPopover modal>
          <UButton
            color="neutral"
            variant="outline"
            size="xl"
            class="pointer-events-auto cursor-pointer"
            icon="lucide:sun"
            title="Sun"
            aria-label="Sun"
          />

          <template #content>
            <UCard :ui="{ body: '!p-3 grid min-w-72 gap-3' }">
              <label class="sun-control-field">
                <span class="sun-control-label">
                  <span>Azimuth</span>
                  <output>{{ sunAzimuthLabel }}</output>
                </span>
                <USlider
                  v-model.number="mapViewSettings.sun_azimuth_degrees"
                  :min="0"
                  :max="360"
                  :step="1"
                />
              </label>

              <label class="sun-control-field">
                <span class="sun-control-label">
                  <span>Elevation</span>
                  <output>{{ sunElevationLabel }}</output>
                </span>
                <USlider
                  v-model.number="mapViewSettings.sun_elevation_degrees"
                  :min="0"
                  :max="85"
                  :step="1"
                />
              </label>
            </UCard>
          </template>
        </UPopover>
      </div>
    </div>

    <div
      v-show="showBevyCanvas"
      :style="`position: absolute; width: ${showBevyCanvas ? '100%' : '10px'}; height: ${showBevyCanvas ? '50%' : '1px'}; bottom: 0`"
    >
      <canvas
        :id="bevyCanvasId"
        style="position: absolute; inset: 0; height: 100%; width: 100%"
      ></canvas>
    </div>

    <UContextMenu :items="contextMenuItems" :modal="false">
      <div
        ref="mapContextMenuTarget"
        class="h-full w-full absolute"
        style="pointer-events: none"
        @contextmenu="console.log"
      ></div>
    </UContextMenu>

    <MapSlideover
      ref="mapSlideover"
      :open="slideoverOpen !== null"
      :active="slideoverOpen"
      :direction-stops="directionStops"
      :details-osm-id="selection[0]?.osm_id"
      :details-feature="selection[0]?.feature"
      :map="mapInstance.map"
      :bevy-settings="mapViewSettings"
      :bevy-camera-settings="mapViewCameraSettings"
      @update:direction-stops="directionStops = $event"
      @update:trip-primary="directionsTripPrimary = $event"
      @update:trip-alternates="directionsTripAlternates = $event"
      @update:drawer-direction="slideoverDirection = $event"
      @update:drawer-size="slideoverSize = $event"
      @focus-trip="focusTrip"
      @update:open="
        (value: boolean) => {
          if (!value) onSlideoverClose()
        }
      "
    />
  </div>
</template>

<script setup lang="ts">
import { MglMap } from '@indoorequal/vue-maplibre-gl'
import { computed, onWatcherCleanup, ref, shallowRef, watch, watchEffect } from 'vue'
import { onLongPress, useDark } from '@vueuse/core'
import {
  GeolocateControl,
  GlobeControl,
  NavigationControl,
  type DragPanOptions,
  type Map as MapLibreMap,
  type MapMouseEvent,
  type StyleSwapOptions,
  type StyleOptions,
} from 'maplibre-gl'
import { center } from '@turf/turf'
import type { FeatureCollection } from 'geojson'
import {
  TILESERVER_OMT_DEFAULT_STYLE_TILEJSON_URL,
  TILESERVER_RASTER_SEN2_TILEJSON_URL,
} from '@/external/endpoints.ts'
import { makeUniqueMapKey, useMapExtended, useMapSelection } from '@/composables/maplibre.ts'
import { watchDefinedOnce } from '@/composables/helper.ts'
import { useMaplibreGlJsIntegration } from '@/composables/bevy-maplibre-integration.ts'
import { useBevy } from '@/composables/bevy.ts'
import { BevyLayer } from '../maplibre-layers/bevy-layer.ts'
import MapSlideover, { type MapSlideoverTab } from '@/components/map-slideover/MapSlideover.vue'
import { GeoLocationType, type GeoLocation } from '@/components/types.ts'
import type { ContextMenuItem } from '@nuxt/ui'
import type { Trip } from 'valhalla_client'
import {
  DIRECTION_STOPS_LAYER_ID,
  useDirectionsLayers,
} from '@/maplibre-layers/directions-layers.ts'
import { useHighlightLayer } from '@/maplibre-layers/highlight-layer.ts'
import { useRainfallRasterLayer } from '@/maplibre-layers/rainfall-raster-layer.ts'
import { getTripBounds } from '@/utils/valhalla.ts'
import type { ModeSelectorOption } from '@/components/ModeSelector.vue'

const mapKey = makeUniqueMapKey()

const bevyCanvasId = `bevy-canvas-${mapKey}`

const { instanceId, mapViewSettings, mapViewCameraSettings, tick, mapTextureOffscreenCanvas } =
  useBevy(`#${bevyCanvasId}`, '.maplibregl-canvas')

const { mapInstance, loaded, zoom } = useMapExtended(mapKey)

const { syncOnRender } = useMaplibreGlJsIntegration(() => instanceId, mapKey, {
  featureSourceLayers: [
    { sourceId: 'openmaptiles', sourceLayer: 'building' },
    { sourceId: 'openmaptiles', sourceLayer: 'water' },
  ],
})

const tilejsonUrl = TILESERVER_OMT_DEFAULT_STYLE_TILEJSON_URL.toString()
console.debug('Using TileJson URL: ', tilejsonUrl)

// Base Style

enum BaseStyleDefinitionType {
  Normal = 'normal',
  Satellite = 'satellite',
}

const baseStyleTypeOptions: ModeSelectorOption<BaseStyleDefinitionType>[] = [
  {
    label: 'Normal',
    value: BaseStyleDefinitionType.Normal,
  },
  {
    label: 'Satellite',
    value: BaseStyleDefinitionType.Satellite,
  },
]

const baseStyleType = shallowRef(BaseStyleDefinitionType.Normal)

//

const darkThemeEnabled = useDark()

const toggleDarkTheme = () => {
  darkThemeEnabled.value = !darkThemeEnabled.value
}

// Context Menu

const mapContextMenuTarget = ref<HTMLElement | null>(null)
const contextMenuLocation = shallowRef<GeoLocation | null>(null)

type MglMapMouseEvent = {
  type: string
  event: MapMouseEvent
}

const openContextMenu = ({
  lat,
  lng,
  clientX,
  clientY,
}: {
  lat: number
  lng: number
  clientX: number
  clientY: number
}) => {
  contextMenuLocation.value = {
    type: GeoLocationType.Coords,
    coords: {
      lat,
      lng,
    },
  }

  mapContextMenuTarget.value?.dispatchEvent(
    new MouseEvent('contextmenu', {
      bubbles: true,
      cancelable: true,
      clientX,
      clientY,
    }),
  )
}

const onMapContextMenu = ({ event }: MglMapMouseEvent) => {
  event.preventDefault()
  event.originalEvent.preventDefault()

  openContextMenu({
    lat: event.lngLat.lat,
    lng: event.lngLat.lng,
    clientX: event.originalEvent.clientX,
    clientY: event.originalEvent.clientY,
  })
}

const setDirectionStop = (idx: number) => {
  if (!contextMenuLocation.value) return

  if (slideoverOpen.value !== SlideoverTab.Directions) {
    directionStops.value =
      idx === 0 ? [contextMenuLocation.value, null] : [null, contextMenuLocation.value]
    slideoverOpen.value = SlideoverTab.Directions
    return
  }

  const stops = [...directionStops.value]
  stops[idx] = contextMenuLocation.value

  directionStops.value = stops
  slideoverOpen.value = SlideoverTab.Directions
}

const contextMenuCoordinateLabel = computed(() => {
  const location = contextMenuLocation.value
  if (!location) return 'No location selected'

  return `${location.coords.lat.toFixed(6)}, ${location.coords.lng.toFixed(6)}`
})

const contextMenuItems = computed((): ContextMenuItem[] => [
  {
    label: contextMenuCoordinateLabel.value,
    type: 'label',
    icon: 'material-symbols:location-on-outline-rounded',
  },
  {
    type: 'separator',
  },
  {
    label: 'Directions From Here',
    icon: 'material-symbols:line-end-circle-outline-rounded',
    ui: {
      itemLeadingIcon: '-rotate-90',
    } as unknown as ContextMenuItem['ui'],
    onSelect: () => setDirectionStop(0),
    disabled: directionStops.value.length < 1,
  },
  {
    label: 'Directions To Here',
    icon: 'material-symbols:line-end-circle-outline-rounded',
    ui: {
      itemLeadingIcon: 'rotate-90',
    } as unknown as ContextMenuItem['ui'],
    onSelect: () => setDirectionStop(directionStops.value.length - 1),
    disabled: directionStops.value.length < 2,
  },
])

const registerTouchContextMenu = (map: NonNullable<typeof mapInstance.map>) => {
  const canvas = map.getCanvas()

  const cleanupLongPress = onLongPress(
    canvas,
    (event) => {
      if (event.pointerType !== 'touch' || !event.isPrimary) return

      event.preventDefault()

      // prevent pointer release after long press from immediately closing the context menu,
      // which must handle both the touch and pointer events fired on release,
      // note: not great, works fine for now, but has unhandled edge cases which for pointerup is fine
      // as it only relates to the original pointerId but the touch events cannot be properly identified and may
      // subsequently incorrectly block input (with a fallback solution of a 5s timer to automatically release the listeners)

      const pointerId = event.pointerId
      const onPointerUp = (event: PointerEvent) => {
        if (event.pointerId === pointerId) {
          event.preventDefault()
          document.removeEventListener('pointerup', onPointerUp, true)
        }
      }

      const onTouchEnd = (event: TouchEvent) => {
        event.preventDefault()
        document.removeEventListener('touchend', onTouchEnd, true)
      }

      document.addEventListener('pointerup', onPointerUp, true)
      document.addEventListener('touchend', onTouchEnd, true)
      setTimeout(() => {
        document.removeEventListener('pointerup', onPointerUp, true)
        document.removeEventListener('touchend', onTouchEnd, true)
      }, 5000)

      //

      const canvasBounds = canvas.getBoundingClientRect()
      const lngLat = map.unproject([
        event.clientX - canvasBounds.left,
        event.clientY - canvasBounds.top,
      ])

      openContextMenu({
        lat: lngLat.lat,
        lng: lngLat.lng,
        clientX: event.clientX,
        clientY: event.clientY,
      })
    },
    {
      delay: 700,
      distanceThreshold: 12,
    },
  )

  return () => {
    cleanupLongPress()
  }
}

// Slideover

const SlideoverTab = {
  Details: 'details',
  Settings: 'settings',
  Directions: 'directions',
} as const satisfies Record<string, MapSlideoverTab>

const slideoverOpen = ref<MapSlideoverTab | null>(null)
const mapSlideover = ref<InstanceType<typeof MapSlideover> | null>(null)

const slideoverSize = shallowRef({ width: 0, height: 0 })
const slideoverDirection = shallowRef<'left' | 'right' | 'top' | 'bottom'>('left')

const layersControlStyle = computed(() => ({
  bottom: '0px',
  left: slideoverDirection.value === 'left' ? `${slideoverSize.value.width}px` : '0px',
}))

const sunAzimuthLabel = computed(() => `${Math.round(mapViewSettings.sun_azimuth_degrees)} deg`)
const sunElevationLabel = computed(() => `${Math.round(mapViewSettings.sun_elevation_degrees)} deg`)

const onSlideoverClose = () => {
  switch (slideoverOpen.value) {
    case SlideoverTab.Details:
      selection.value.splice(0)
      break
  }

  slideoverOpen.value = null
}

// Directions

const directionStops = shallowRef<(GeoLocation | null)[]>([null, null])

const directionsTripPrimary = shallowRef<Trip | null>(null)
const directionsTripAlternates = shallowRef<Trip[]>([])

const directionsLayers = useDirectionsLayers({
  stops: directionStops,
  tripPrimary: directionsTripPrimary,
  visible: computed(() => slideoverOpen.value === SlideoverTab.Directions),
})

const focusTrip = (trip: Trip) => {
  const map = mapInstance.map
  if (!map) return

  const bounds = getTripBounds(trip)
  if (!bounds) return

  map.fitBounds(bounds, {
    padding: mapSlideover.value?.getRouteFitPadding() ?? 80,
    maxZoom: 17,
    duration: 700,
  })
}

// Selection

const selectableLayers = ref<string[]>([])

const { selection } = useMapSelection({
  key: mapKey,
  targetLayers: selectableLayers,
})

watchEffect(() => {
  if (selection.value.length === 1) {
    slideoverOpen.value = SlideoverTab.Details
  } else if (selection.value.length !== 1 && slideoverOpen.value === SlideoverTab.Details) {
    slideoverOpen.value = null
  }
})

const highlightGeoJsonData = computed(
  (): FeatureCollection => ({
    type: 'FeatureCollection',
    features: selection.value.map((item) => center(item.feature.geometry)),
  }),
)

const highlightLayer = useHighlightLayer({
  data: highlightGeoJsonData,
  visible: computed(() => true),
})

const showBevyCanvas = ref(false)

watch(
  showBevyCanvas,
  (value) => {
    mapViewSettings.enable_window_cameras = value
  },
  { immediate: true },
)

const terrainEnabled = ref(false)
const rainfallEnabled = ref(false)

const rainfallLayer = useRainfallRasterLayer({
  visible: rainfallEnabled,
  onLoadError: (error) => {
    console.warn('Failed to load RainViewer rainfall layer', error)
    rainfallEnabled.value = false
  },
})

const rainfallRasterDataTime = rainfallLayer.rasterDataTime
const rainfallRasterLoading = rainfallLayer.loading

const rainfallRasterDataTimeFormatter = new Intl.DateTimeFormat(undefined, {
  month: 'short',
  day: 'numeric',
  hour: '2-digit',
  minute: '2-digit',
})

const rainfallRasterDataTimeLabel = computed(() => {
  const dataTime = rainfallRasterDataTime.value

  if (!dataTime) return 'Radar frame not loaded'

  return `At ${rainfallRasterDataTimeFormatter.format(dataTime)}`
})

const refreshRainfallRasterData = () => {
  rainfallLayer.refreshData().catch(() => {
    // The layer composable reports load failures through onLoadError.
  })
}

watch(rainfallEnabled, (value) => {
  if (value) {
    refreshRainfallRasterData()
  }
})

const DESKTOP_DRAG_PAN_OPTIONS: DragPanOptions = {
  linearity: 0.35,
  maxSpeed: 3200,
  deceleration: 6000,
  easing: lateBrakeDragPanEasing,
}

const MOBILE_DRAG_PAN_OPTIONS: DragPanOptions = {
  linearity: 0.7,
  maxSpeed: 4800,
  deceleration: 2000,
  easing: gradualFadeDragPanEasing,
}

function lateBrakeDragPanEasing(t: number) {
  const x = clampUnit(t)

  return (4 * x - x ** 4) / 3
}

function gradualFadeDragPanEasing(t: number) {
  const x = clampUnit(t)

  return 1 - (1 - x) ** 2
}

function clampUnit(value: number) {
  return Math.max(0, Math.min(1, value))
}

function registerDragPanInertiaProfiles(map: MapLibreMap) {
  const canvasContainer = map.getCanvasContainer()
  const useDesktopProfile = () => map.dragPan.enable(DESKTOP_DRAG_PAN_OPTIONS)
  const useMobileProfile = () => map.dragPan.enable(MOBILE_DRAG_PAN_OPTIONS)
  const usePointerProfile = (event: PointerEvent) => {
    if (event.pointerType === 'touch' || event.pointerType === 'pen') {
      useMobileProfile()
      return
    }

    useDesktopProfile()
  }

  if (window.matchMedia('(pointer: coarse)').matches) {
    useMobileProfile()
  } else {
    useDesktopProfile()
  }

  canvasContainer.addEventListener('pointerdown', usePointerProfile, { capture: true })
  canvasContainer.addEventListener('mousedown', useDesktopProfile, { capture: true })
  canvasContainer.addEventListener('touchstart', useMobileProfile, {
    capture: true,
    passive: true,
  })

  return () => {
    canvasContainer.removeEventListener('pointerdown', usePointerProfile, { capture: true })
    canvasContainer.removeEventListener('mousedown', useDesktopProfile, { capture: true })
    canvasContainer.removeEventListener('touchstart', useMobileProfile, { capture: true })
  }
}

// Controls

watchDefinedOnce(
  () => (loaded.value ? mapInstance.map : undefined),
  (map) => {
    map.addControl(new GlobeControl())
    map.addControl(new NavigationControl())
    map.addControl(new GeolocateControl({}))

    map.setMaxPitch(85)
  },
)

// Base Styles

type BaseStyle = {
  source: string
  options?: StyleSwapOptions & StyleOptions
  onLoaded: (map: MapLibreMap) => {
    onCleanup: (() => void)[]
  }
}

const makeBasicStyle = (useRaster: boolean): BaseStyle => ({
  source: TILESERVER_OMT_DEFAULT_STYLE_TILEJSON_URL.toString(),
  options: { diff: false },
  onLoaded: (map) => {
    const onCleanup: (() => void)[] = []

    onCleanup.push(registerDragPanInertiaProfiles(map))
    onCleanup.push(registerTouchContextMenu(map))
    rainfallLayer.register(map, 'Water labels')
    onCleanup.push(rainfallLayer.unregister)

    if (useRaster) {
      map.addSource('raster-sen2', {
        type: 'raster',
        url: TILESERVER_RASTER_SEN2_TILEJSON_URL.toString(),
      })

      map.addLayer(
        {
          id: 'raster-sen2-layer',
          type: 'raster',
          source: 'raster-sen2',
          paint: {
            'raster-brightness-min': 0.1,
            'raster-contrast': 0.2,
          },
        },
        'Residential',
      )

      map.getLayersOrder().forEach((layerId) => {
        if (layerId === 'raster-sen2-layer') return

        const layer = map.getLayer(layerId)!

        switch (layer.type) {
          case 'symbol':
            map.setPaintProperty(layerId, 'icon-opacity', [
              'interpolate',
              ['linear'],
              ['zoom'],
              13,
              0.0,
              16,
              0.75,
            ])
            map.setPaintProperty(layerId, 'text-opacity', [
              'interpolate',
              ['linear'],
              ['zoom'],
              13,
              0.0,
              16,
              0.75,
            ])
            break

          case 'fill':
            map.setPaintProperty(
              layerId,
              'fill-outline-color',
              layer.getPaintProperty('fill-color'),
            )
            map.setPaintProperty(layerId, 'fill-color', 'transparent')
            map.setPaintProperty(layerId, 'fill-opacity', [
              'interpolate',
              ['linear'],
              ['zoom'],
              14,
              0,
              16,
              0.25,
            ])
            break

          case 'line':
            map.setPaintProperty(layerId, 'line-opacity', 0.05)
            break

          default:
            map.setLayoutProperty(layerId, 'visibility', 'none')
            break
        }
      })
    }

    // Sky / Terrain / Hillshade

    map.addSource('terrain', {
      type: 'raster-dem',
      url: 'https://tiles.mapterhorn.com/tilejson.json',
      maxzoom: 16,
    })

    map.addSource('hillshade', {
      type: 'raster-dem',
      url: 'https://tiles.mapterhorn.com/tilejson.json',
      maxzoom: 16,
    })

    map.setSky({
      'sky-color': '#199EF3',
      'sky-horizon-blend': 0.7,
      'horizon-color': 'rgb(236 248 251)',
      'horizon-fog-blend': 0.9,
      'fog-color': 'rgb(165 209 223 / 0.5)',
      'fog-ground-blend': 0.8,
      'atmosphere-blend': ['interpolate', ['linear'], ['zoom'], 0, 0.45, 7, 0.25, 10, 0],
    })

    onCleanup.push(
      watch(
        zoom,
        (value) => {
          if (value < 10) {
            map.setLight({
              anchor: 'map',
              position: [1.5, 90, 80],
              intensity: 0.25,
            })
          } else {
            map.setLight({
              anchor: 'viewport',
              position: [1.15, 210, 30],
              intensity: 0.5,
            })
          }
        },
        { immediate: true },
      ).stop,
    )

    onCleanup.push(
      watch(
        terrainEnabled,
        (enabled) => {
          if (enabled) {
            map.setTerrain({
              source: 'terrain',
              exaggeration: 1.0,
            })
          } else {
            map.setTerrain(null)
          }
        },
        { immediate: true },
      ).stop,
    )

    map.addLayer({
      id: 'hills',
      type: 'hillshade',
      source: 'hillshade',
      paint: {
        'hillshade-exaggeration': useRaster ? 0.4 : 0.5,
        'hillshade-shadow-color': useRaster ? 'rgb(0 0 0 / 0.8)' : 'rgb(71 59 36 / 0.84)',
        'hillshade-highlight-color': useRaster
          ? 'rgb(255 255 255 / 0.29)'
          : 'rgb(255 255 255 / 0.84)',
        'hillshade-method': useRaster ? 'igor' : 'combined',
      },
    })

    onCleanup.push(
      watch(
        terrainEnabled,
        (enabled) => {
          if (enabled) {
            map.setLayoutProperty('hills', 'visibility', 'visible')
          } else {
            map.setLayoutProperty('hills', 'visibility', 'none')
          }
        },
        { immediate: true },
      ).stop,
    )

    // Bevy

    map.addLayer(
      new BevyLayer(mapTextureOffscreenCanvas, {
        id: 'bevy-texture',
        tick: () => {
          syncOnRender()
          tick()
        },
      }),
      'Water labels',
    )
    ;['Oneway path', 'Oneway', 'Oneway opposite'].forEach((layerId) => {
      const layer = map.getStyle().layers.find((l) => l.id === layerId)
      if (!layer) return

      map.removeLayer(layerId)
      map.addLayer(layer, 'bevy-texture')
    })

    // Highlight

    highlightLayer.register(map)

    // Directions

    directionsLayers.register(map, 'Other border')

    //

    selectableLayers.value = map
      .getLayersOrder()
      .filter(
        (layer) => map.getLayer(layer)?.type === 'symbol' && layer !== DIRECTION_STOPS_LAYER_ID,
      )

    // Clean-Up

    return { onCleanup }
  },
})

const baseStyleDefinitions = {
  [BaseStyleDefinitionType.Normal]: makeBasicStyle(false),
  [BaseStyleDefinitionType.Satellite]: makeBasicStyle(true),
}

const baseStyle = computed(() => baseStyleDefinitions[baseStyleType.value])

watchDefinedOnce(
  () => mapInstance.map,
  (map) => {
    const onWatcherCleanupCallbacks: (() => void)[] = []
    let styleRequestId = 0
    let onStyleCleanupCallbacks: (() => void)[] = []

    const cleanupStyle = () => {
      onStyleCleanupCallbacks.splice(0).forEach((callback) => callback())
      selectableLayers.value = []
    }

    onWatcherCleanupCallbacks.push(
      watch(
        baseStyle,
        async (selectedBaseStyle) => {
          const currentStyleRequestId = ++styleRequestId

          cleanupStyle()

          const styleLoaded = map.once('style.load') as Promise<unknown>
          map.setStyle(selectedBaseStyle.source, selectedBaseStyle.options)

          await styleLoaded

          if (currentStyleRequestId !== styleRequestId) return

          onStyleCleanupCallbacks = selectedBaseStyle.onLoaded(map).onCleanup
        },
        { immediate: true },
      ).stop,
    )

    onWatcherCleanup(() => {
      styleRequestId++
      cleanupStyle()
      onWatcherCleanupCallbacks.forEach((callback) => callback())
    })
  },
)
</script>

<style lang="css">
@import 'maplibre-gl/dist/maplibre-gl.css';

.maplibregl-canvas {
  background: #131d25;
}

.map-custom-control {
  width: 29px;
  height: 29px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 0;
  padding: 0;
  background: #fff;
  color: #333;
  cursor: pointer;
}

.map-custom-control:hover {
  background: #f2f2f2;
}

.map-custom-control:focus-visible {
  outline: 2px solid #2563eb;
  outline-offset: -2px;
}

.map-custom-control.is-active {
  background: #e0f2fe;
  color: #0369a1;
  box-shadow: inset 0 0 0 2px #0284c7;
}

.map-custom-control-icon {
  width: 17px;
  height: 17px;
  display: block;
}

.sun-control-field {
  display: grid;
  gap: 0.45rem;
}

.sun-control-label {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  font-size: 0.875rem;
}

.sun-control-label output {
  color: #64748b;
  font-variant-numeric: tabular-nums;
}
</style>
