<template>
  <MapPopoverControl icon="lucide:layers" title="Layers">
    <UCard :ui="{ body: '!p-2 grid w-72 max-w-[calc(100vw-1rem)] gap-2' }">
      <div v-if="currentBaseStyleLayerSettings.bevyEnabled" class="grid grid-cols-2 gap-1">
        <UButton
          v-for="(button, index) in bevyToggleButtons"
          :key="index"
          :label="button.label"
          color="neutral"
          active-color="primary"
          variant="outline-solid"
          :active="button.active"
          size="md"
          class="cursor-pointer"
          :icon="button.icon"
          @click="button.click"
        />

        <label class="col-span-2 grid gap-2 px-1 pt-2">
          <span class="flex justify-between gap-4 text-sm">
            <span>Feature distance</span>
            <output class="text-muted tabular-nums">
              {{ featureVisibilityDistanceLabel }}
            </output>
          </span>
          <USlider
            v-model.number="currentBaseStyleLayerSettings.featureVisibilityDistance"
            :min="0"
            :max="40"
            :step="1"
          />
        </label>
      </div>

      <USeparator v-if="currentBaseStyleLayerSettings.bevyEnabled" />

      <div class="grid min-w-0 gap-1">
        <div class="grid grid-cols-2 gap-1">
          <UButton
            v-for="(button, index) in genericToggleButtons"
            :key="index"
            :label="button.label"
            color="neutral"
            active-color="primary"
            variant="outline-solid"
            :active="button.active"
            size="md"
            class="cursor-pointer"
            :icon="button.icon"
            @click="button.click"
          />
        </div>

        <div class="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] gap-1">
          <UButton
            label="Rainfall"
            color="neutral"
            active-color="primary"
            variant="outline-solid"
            :active="mapViewStore.rainfallEnabled"
            size="md"
            class="min-w-0 cursor-pointer justify-start"
            icon="lucide:cloud-rain"
            @click="mapViewStore.rainfallEnabled = !mapViewStore.rainfallEnabled"
          />

          <UButton
            :disabled="!mapViewStore.rainfallEnabled"
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

        <p v-if="mapViewStore.rainfallEnabled" class="min-w-0 truncate px-1 text-xs text-muted">
          {{ rainfallRasterDataTimeLabel }}
        </p>
      </div>

      <USeparator />

      <ModeSelector
        v-model="mapViewStore.baseStyleType"
        :options="baseStyleTypeOptions"
        :ui="{ root: 'min-w-0', button: 'py-2' }"
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
    </UCard>
  </MapPopoverControl>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useDark } from '@vueuse/core'
import ModeSelector, { type ModeSelectorOption } from '@/components/ModeSelector.vue'
import MapPopoverControl from '@/views/map-view/MapPopoverControl.vue'
import { useMapViewStoreOrThrow } from '@/views/map-view/map-view-store.ts'
import { MapViewBaseStyleType } from '@/views/map-view/map-view-types.ts'
import type { RainfallRasterSourceProviderRet } from '@/composables/rainfall-raster-provider.ts'

const props = defineProps<{
  rainfallRasterSourceProvider: RainfallRasterSourceProviderRet
}>()

const { mapViewStore, currentBaseStyleLayerSettings } = useMapViewStoreOrThrow()

const rainfallRasterLoading = props.rainfallRasterSourceProvider.loading

const rainfallRasterDataTimeFormatter = new Intl.DateTimeFormat(undefined, {
  month: 'short',
  day: 'numeric',
  hour: '2-digit',
  minute: '2-digit',
})

const rainfallRasterDataTimeLabel = computed(() => {
  const dataTime = props.rainfallRasterSourceProvider.rasterDataTime.value

  if (!dataTime) return 'Radar frame not loaded'

  return `At ${rainfallRasterDataTimeFormatter.format(dataTime)}`
})

const refreshRainfallRasterData = () => {
  props.rainfallRasterSourceProvider.refreshData().catch(() => {
    // The provider reports load failures through onLoadError.
  })
}

const featureVisibilityDistanceLabel = computed(
  () => `${Math.round(currentBaseStyleLayerSettings.value.featureVisibilityDistance)}`,
)

const baseStyleTypeOptions: ModeSelectorOption<MapViewBaseStyleType>[] = [
  {
    label: 'Normal',
    value: MapViewBaseStyleType.Normal,
  },
  {
    label: 'Satellite',
    value: MapViewBaseStyleType.Satellite,
  },
]

const darkThemeEnabled = useDark()

const toggleDarkTheme = () => {
  darkThemeEnabled.value = !darkThemeEnabled.value
}

type ToggleButtonProps = {
  label: string
  icon: string
  active: boolean
  click: () => void
}

const genericToggleButtons = computed<ToggleButtonProps[]>(() => [
  {
    label: 'Fancy',
    icon: 'lucide:star',
    active: currentBaseStyleLayerSettings.value.bevyEnabled,
    click: () =>
      (currentBaseStyleLayerSettings.value.bevyEnabled =
        !currentBaseStyleLayerSettings.value.bevyEnabled),
  },
  {
    label: 'Terrain',
    icon: 'lucide:mountain',
    active: currentBaseStyleLayerSettings.value.terrainEnabled,
    click: () =>
      (currentBaseStyleLayerSettings.value.terrainEnabled =
        !currentBaseStyleLayerSettings.value.terrainEnabled),
  },
  {
    label: 'Advanced Roads',
    icon: 'lucide:route',
    active: currentBaseStyleLayerSettings.value.advancedRoadsEnabled,
    click: () =>
      (currentBaseStyleLayerSettings.value.advancedRoadsEnabled =
        !currentBaseStyleLayerSettings.value.advancedRoadsEnabled),
  },
  {
    label: 'GTFS',
    icon: 'lucide:train-front',
    active: currentBaseStyleLayerSettings.value.gtfsEnabled,
    click: () =>
      (currentBaseStyleLayerSettings.value.gtfsEnabled =
        !currentBaseStyleLayerSettings.value.gtfsEnabled),
  },
  {
    label: 'Cinematic',
    icon: 'lucide:film',
    active: currentBaseStyleLayerSettings.value.cinematicEnabled,
    click: () =>
      (currentBaseStyleLayerSettings.value.cinematicEnabled =
        !currentBaseStyleLayerSettings.value.cinematicEnabled),
  },
])

const bevyToggleButtons = computed<ToggleButtonProps[]>(() => [
  {
    label: 'Shadows',
    icon: 'lucide:sunset',
    active: currentBaseStyleLayerSettings.value.shadowsEnabled,
    click: () =>
      (currentBaseStyleLayerSettings.value.shadowsEnabled =
        !currentBaseStyleLayerSettings.value.shadowsEnabled),
  },
  {
    label: '3D Buildings',
    icon: 'lucide:building',
    active: currentBaseStyleLayerSettings.value.buildingsEnabled,
    click: () =>
      (currentBaseStyleLayerSettings.value.buildingsEnabled =
        !currentBaseStyleLayerSettings.value.buildingsEnabled),
  },
  {
    label: 'Trees',
    icon: 'lucide:tree-pine',
    active: currentBaseStyleLayerSettings.value.treesEnabled,
    click: () =>
      (currentBaseStyleLayerSettings.value.treesEnabled =
        !currentBaseStyleLayerSettings.value.treesEnabled),
  },
])
</script>
