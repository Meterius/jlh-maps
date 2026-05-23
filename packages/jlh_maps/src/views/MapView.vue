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
      />

      <div class="pointer-events-none absolute right-2 top-2 z-10 flex flex-col gap-2">
        <UButton
          color="neutral"
          active-color="primary"
          variant="outline-solid"
          :active="globeActive"
          :disabled="globeDisabled"
          size="xl"
          class="pointer-events-auto cursor-pointer"
          icon="lucide:globe"
          :title="globeTitle"
          :aria-label="globeTitle"
          :aria-pressed="globeAriaPressed"
          @click="triggerGlobe"
        />

        <UButton
          color="neutral"
          active-color="primary"
          variant="outline-solid"
          :active="geolocateActive"
          :disabled="geolocateDisabled"
          :loading="geolocateLoading"
          size="xl"
          class="pointer-events-auto cursor-pointer"
          :icon="geolocateIcon"
          :title="geolocateTitle"
          :aria-label="geolocateTitle"
          :aria-pressed="geolocateAriaPressed"
          @click="triggerGeolocate"
        />

        <UFieldGroup orientation="vertical" class="pointer-events-auto">
          <UButton
            color="neutral"
            variant="outline-solid"
            :disabled="zoomInDisabled"
            size="xl"
            class="cursor-pointer"
            icon="lucide:plus"
            :title="zoomInTitle"
            :aria-label="zoomInTitle"
            @click="zoomIn"
          />

          <UButton
            color="neutral"
            variant="outline-solid"
            :disabled="zoomOutDisabled"
            size="xl"
            class="cursor-pointer"
            icon="lucide:minus"
            :title="zoomOutTitle"
            :aria-label="zoomOutTitle"
            @click="zoomOut"
          />

          <UButton
            color="neutral"
            variant="outline-solid"
            :disabled="compassDisabled"
            size="xl"
            class="cursor-pointer"
            icon="mingcute:compass-3-fill"
            :title="compassTitle"
            :aria-label="compassTitle"
            :style="compassIconStyle"
            :ui="{ leadingIcon: '[transform:var(--compass-icon-transform)]' }"
            @pointerdown="startCompassDrag"
            @click="resetBearing"
          />
        </UFieldGroup>

        <UButton
          color="neutral"
          active-color="primary"
          variant="outline-solid"
          :active="slideoverOpen === SlideoverTab.Settings"
          size="xl"
          class="pointer-events-auto cursor-pointer"
          icon="lucide:settings"
          title="Map settings"
          aria-label="Map settings"
          @click="slideoverOpen = SlideoverTab.Settings"
        />

        <UButton
          color="neutral"
          active-color="primary"
          variant="outline-solid"
          :active="slideoverOpen === SlideoverTab.Directions"
          size="xl"
          class="pointer-events-auto cursor-pointer"
          icon="lucide:signpost"
          title="Navigation"
          aria-label="Navigation"
          @click="slideoverOpen = SlideoverTab.Directions"
        />

        <UButton
          color="neutral"
          active-color="primary"
          variant="outline-solid"
          :active="showBevyCanvas"
          size="xl"
          class="pointer-events-auto cursor-pointer"
          icon="lucide:bug"
          title="Show bevy"
          aria-label="Show bevy"
          :aria-pressed="showBevyCanvas"
          @click="showBevyCanvas = !showBevyCanvas"
        />
      </div>

      <div
        class="pointer-events-none absolute z-10 flex gap-2 p-2 transition-[bottom,left] duration-200 ease-out"
        :style="layersControlStyle"
      >
        <UPopover modal>
          <template #default="{ open }">
            <UButton
              color="neutral"
              active-color="primary"
              variant="outline-solid"
              :active="open"
              size="xl"
              class="pointer-events-auto cursor-pointer"
              icon="lucide:layers"
              title="Layers"
              aria-label="Layers"
            />
          </template>

          <template #content>
            <UCard :ui="{ body: '!p-2 grid w-72 max-w-[calc(100vw-1rem)] gap-2' }">
              <div class="grid grid-cols-2 gap-1">
                <UButton
                  label="Shadows"
                  color="neutral"
                  active-color="primary"
                  variant="outline-solid"
                  :active="mapViewSettings.enable_shadows"
                  size="md"
                  class="cursor-pointer"
                  icon="lucide:sunset"
                  @click="mapViewSettings.enable_shadows = !mapViewSettings.enable_shadows"
                />

                <UButton
                  label="3D Buildings"
                  color="neutral"
                  active-color="primary"
                  variant="outline-solid"
                  :active="mapViewSettings.enable_buildings"
                  size="md"
                  class="cursor-pointer"
                  icon="lucide:building"
                  @click="mapViewSettings.enable_buildings = !mapViewSettings.enable_buildings"
                />

                <UButton
                  label="Terrain"
                  color="neutral"
                  active-color="primary"
                  variant="outline-solid"
                  :active="terrainEnabled"
                  size="md"
                  class="cursor-pointer"
                  icon="lucide:mountain"
                  @click="terrainEnabled = !terrainEnabled"
                />

                <UButton
                  label="Dark Theme"
                  color="neutral"
                  active-color="primary"
                  variant="outline-solid"
                  :active="darkThemeEnabled"
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
                    color="neutral"
                    active-color="primary"
                    variant="outline-solid"
                    :active="rainfallEnabled"
                    size="md"
                    class="min-w-0 cursor-pointer justify-start"
                    icon="lucide:cloud-rain"
                    @click="rainfallEnabled = !rainfallEnabled"
                  />

                  <UButton
                    :disabled="!rainfallEnabled"
                    color="neutral"
                    variant="outline-solid"
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
          <template #default="{ open }">
            <UButton
              color="neutral"
              active-color="primary"
              variant="outline-solid"
              :active="open"
              size="xl"
              class="pointer-events-auto cursor-pointer"
              icon="lucide:sun"
              title="Sun"
              aria-label="Sun"
            />
          </template>

          <template #content>
            <UCard :ui="{ body: '!p-3 grid min-w-72 gap-3' }">
              <label class="grid gap-2">
                <span class="flex justify-between gap-4 text-sm">
                  <span>Azimuth</span>
                  <output class="text-muted tabular-nums">{{ sunAzimuthLabel }}</output>
                </span>
                <USlider
                  v-model.number="mapViewSettings.sun_azimuth_degrees"
                  :min="0"
                  :max="360"
                  :step="1"
                />
              </label>

              <label class="grid gap-2">
                <span class="flex justify-between gap-4 text-sm">
                  <span>Elevation</span>
                  <output class="text-muted tabular-nums">{{ sunElevationLabel }}</output>
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
        ref="bevyDebugCanvas"
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
      :details-osm-id="selection[0]?.osm_id"
      :details-feature="selection[0]?.feature"
      :map="mapInstance.map"
      :bevy-instance-id="instanceId"
      @update:drawer-direction="slideoverDirection = $event"
      @update:drawer-size="slideoverSize = $event"
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
import { computed, ref, shallowRef, watch, watchEffect } from 'vue'
import { onLongPress, useDark } from '@vueuse/core'
import type { MapMouseEvent } from 'maplibre-gl'
import {
  TILESERVER_OMT_DEFAULT_STYLE_TILEJSON_URL,
  TILESERVER_RASTER_SEN2_TILE_URL_PATTERN,
} from '@/external/endpoints.ts'
import {
  makeUniqueMapKey,
  useLayer,
  useMapExtended,
  useMapSelection,
  useRasterTilesBasedSource,
  useSource,
} from '@/composables/maplibre'
import {
  useGeolocateControl,
  useGlobeControl,
  useNavigationControl,
} from '@/composables/maplibre/controls'
import { onScopeDisposeLifo, watchDefinedOnce } from '@/composables/helper.ts'
import { useMaplibreIntegration } from '@/bevy/maplibre-integration'
import { mountBevy, useBevy } from '@/bevy'
import { BevyLayer } from '../maplibre-layers/bevy-layer.ts'
import MapSlideover, { type MapSlideoverTab } from '@/views/map-view/MapSlideover.vue'
import { GeoLocationType, type GeoLocation } from '@/components/types.ts'
import type { ContextMenuItem } from '@nuxt/ui'
import {
  DIRECTION_STOPS_LAYER_ID,
  useDirectionsLayers,
} from '@/maplibre-layers/directions-layers.ts'
import { useHighlightLayer } from '@/maplibre-layers/highlight-layer.ts'
import { usePoiLayers } from '@/maplibre-layers/poi-layer.ts'
import { useRainfallRasterLayer } from '@/maplibre-layers/rainfall-raster-layer.ts'
import { useRainfallRasterProvider } from '@/composables/rainfall-raster-provider.ts'
import type { ModeSelectorOption } from '@/components/ModeSelector.vue'
import {
  type MapStyleLifecycleConfig,
  useMapStyleLifecycle,
} from '@/composables/maplibre/style-lifecycle'
import { usePanProfiles } from '@/composables/maplibre/pan-profiles'
import { provideMapDirectionStops } from '@/views/map-view/map-direction-stops.ts'
import { provideMapCameraController } from '@/views/map-view/map-camera-controller.ts'

const mapKey = makeUniqueMapKey()

const bevyCanvasId = `bevy-canvas-${mapKey}`
const bevyDebugCanvas = ref<HTMLCanvasElement | null>(null)

const { mapInstance, loaded, zoom } = useMapExtended(mapKey)

const { instanceId } = mountBevy(
  () => bevyDebugCanvas.value ?? null,
  () => mapInstance.map?.getCanvas() ?? null,
)
const { tick, textureOffscreenCanvas, mapViewSettings } = useBevy(instanceId)

const {
  active: globeActive,
  ariaPressed: globeAriaPressed,
  disabled: globeDisabled,
  title: globeTitle,
  trigger: triggerGlobe,
} = useGlobeControl(mapKey)

const {
  active: geolocateActive,
  ariaPressed: geolocateAriaPressed,
  disabled: geolocateDisabled,
  icon: geolocateIcon,
  loading: geolocateLoading,
  title: geolocateTitle,
  trigger: triggerGeolocate,
} = useGeolocateControl(mapKey, {
  trackUserLocation: true,
})

const {
  compassDisabled,
  compassIconStyle,
  compassTitle,
  resetBearing,
  startCompassDrag,
  zoomIn,
  zoomInDisabled,
  zoomInTitle,
  zoomOut,
  zoomOutDisabled,
  zoomOutTitle,
} = useNavigationControl(mapKey, { northRotationOffset: 135 })

const { syncOnRender } = useMaplibreIntegration(instanceId, mapKey, {
  featureSourceLayers: [
    { sourceId: 'openmaptiles', sourceLayer: 'building' },
    { sourceId: 'openmaptiles', sourceLayer: 'water' },
  ],
})

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

const contextMenuCoordinateLabel = computed(() => {
  const location = contextMenuLocation.value
  if (!location) return 'No location selected'

  return `${location.coords.lat.toFixed(6)}, ${location.coords.lng.toFixed(6)}`
})

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

const sunAzimuthLabel = computed(
  () => `${Math.round(mapViewSettings.value.sun_azimuth_degrees)} deg`,
)
const sunElevationLabel = computed(
  () => `${Math.round(mapViewSettings.value.sun_elevation_degrees)} deg`,
)

const onSlideoverClose = () => {
  switch (slideoverOpen.value) {
    case SlideoverTab.Details:
      selection.value.splice(0)
      break
  }

  slideoverOpen.value = null
}

// Camera

provideMapCameraController(mapKey, {
  viewPadding: () => mapSlideover.value?.getRouteFitPadding() ?? 80,
})

// Directions

const mapDirectionStops = provideMapDirectionStops()

const { directionStops, directionsTripPrimary } = mapDirectionStops

const setDirectionStop = (idx: number) => {
  if (idx < 0) return

  const location = contextMenuLocation.value
  if (!location) return

  if (slideoverOpen.value !== SlideoverTab.Directions) {
    directionStops.value = idx === 0 ? [location, null] : [null, location]
    slideoverOpen.value = SlideoverTab.Directions
    return
  }

  const stops = [...directionStops.value]
  stops[idx] = location

  directionStops.value = stops
  slideoverOpen.value = SlideoverTab.Directions
}

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

const showBevyCanvas = ref(false)

watch(
  showBevyCanvas,
  (value) => {
    mapViewSettings.value.enable_window_cameras = value
  },
  { immediate: true },
)

const terrainEnabled = ref(false)
const rainfallEnabled = ref(false)

const rainfallRasterSourceProvider = useRainfallRasterProvider({
  onLoadError: (error) => {
    console.warn('Failed to load RainViewer rainfall layer', error)
    rainfallEnabled.value = false
  },
})

const rainfallRasterDataTime = rainfallRasterSourceProvider.rasterDataTime
const rainfallRasterLoading = rainfallRasterSourceProvider.loading

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
  rainfallRasterSourceProvider.refreshData().catch(() => {
    // The provider reports load failures through onLoadError.
  })
}

watch(rainfallEnabled, (value) => {
  if (value) {
    refreshRainfallRasterData()
  }
})

// Controls

watchDefinedOnce(
  () => (loaded.value ? mapInstance.map : undefined),
  (map) => {
    map.setMaxPitch(85)
  },
)

usePanProfiles(mapKey)

// Base Styles

const makeBasicStyle = (useRaster: boolean): MapStyleLifecycleConfig => ({
  source: TILESERVER_OMT_DEFAULT_STYLE_TILEJSON_URL.toString(),
  options: { diff: false },
  instantiate: (map) => {
    onScopeDisposeLifo(registerTouchContextMenu(map))

    useRainfallRasterLayer(
      map,
      {
        tiles: rainfallRasterSourceProvider.tiles,
        visible: rainfallEnabled,
      },
      'Water labels',
    )
    usePoiLayers(map)

    if (useRaster) {
      useRasterTilesBasedSource(map, 'raster-sen2', {
        tiles: [TILESERVER_RASTER_SEN2_TILE_URL_PATTERN],
        bounds: [-180.0, -81.06141849964385, 180.0, 83.74834535283912],
        scheme: 'xyz',
        minzoom: 0,
        maxzoom: 10,
        tileSize: 2048,
      })

      useLayer(
        map,
        {
          id: 'raster-sen2-layer',
          type: 'raster',
          source: 'raster-sen2',
          paint: {
            'raster-brightness-min': 0.1,
            'raster-contrast': 0.2,
          },
        },
        { beforeId: 'Residential' },
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

    // Bevy

    if (!useRaster) {
      useLayer(
        map,
        new BevyLayer(textureOffscreenCanvas, {
          id: 'bevy-texture',
          tick: () => {
            syncOnRender()
            tick()
          },
        }),
        { beforeId: 'Water labels' },
      )
      ;['Oneway path', 'Oneway', 'Oneway opposite'].forEach((layerId) => {
        if (map.getLayer(layerId)) {
          map.moveLayer(layerId, 'bevy-texture')
        }
      })
    }

    // Sky / Terrain / Hillshade

    useSource(map, 'terrain', {
      type: 'raster-dem',
      url: 'https://tiles.mapterhorn.com/tilejson.json',
      maxzoom: 16,
    })

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
    )

    onScopeDisposeLifo(() => {
      map.setTerrain(null)
    })

    useLayer(
      map,
      {
        id: 'hills',
        type: 'hillshade',
        source: 'terrain',
        paint: {
          'hillshade-exaggeration': useRaster ? 0.4 : 0.5,
          'hillshade-shadow-color': useRaster ? 'rgb(0 0 0 / 0.8)' : 'rgb(71 59 36 / 0.84)',
          'hillshade-highlight-color': useRaster
            ? 'rgb(255 255 255 / 0.29)'
            : 'rgb(255 255 255 / 0.84)',
          'hillshade-method': useRaster ? 'igor' : 'combined',
        },
      },
      {
        visible: terrainEnabled,
        // insert hillshade before bevy as otherwise bevy depth mask will clip hillshade
        beforeId: useRaster ? undefined : 'bevy-texture',
      },
    )

    map.setSky({
      'sky-color': '#199EF3',
      'sky-horizon-blend': 0.7,
      'horizon-color': 'rgb(236 248 251)',
      'horizon-fog-blend': 0.9,
      'fog-color': 'rgb(165 209 223 / 0.5)',
      'fog-ground-blend': 0.8,
      'atmosphere-blend': ['interpolate', ['linear'], ['zoom'], 0, 0.45, 7, 0.25, 10, 0],
    })

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
    )

    // Highlight

    useHighlightLayer(map, () => selection.value.map((item) => item.feature.geometry))

    // Directions

    useDirectionsLayers(
      map,
      {
        stops: directionStops,
        tripPrimary: directionsTripPrimary,
        visible: computed(() => slideoverOpen.value === SlideoverTab.Directions),
      },
      'Other border',
    )

    //

    selectableLayers.value = map
      .getLayersOrder()
      .filter(
        (layer) => map.getLayer(layer)?.type === 'symbol' && layer !== DIRECTION_STOPS_LAYER_ID,
      )

    onScopeDisposeLifo(() => {
      selectableLayers.value = []
    })
    onScopeDisposeLifo(() => {
      selection.value = []
    })

    // Clean-Up
  },
})

const baseStyleDefinitions = {
  [BaseStyleDefinitionType.Normal]: makeBasicStyle(false),
  [BaseStyleDefinitionType.Satellite]: makeBasicStyle(true),
}

const baseStyle = computed(() => baseStyleDefinitions[baseStyleType.value])

useMapStyleLifecycle(mapKey, baseStyle)
</script>

<style lang="css">
@import 'maplibre-gl/dist/maplibre-gl.css';

/* Background for the inserted maplibre canvas */
.maplibregl-canvas {
  background: #131d25;
}
</style>
