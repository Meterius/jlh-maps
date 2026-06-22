<template>
  <MapPopoverControl icon="lucide:layers" title="Layers">
    <UCard :ui="{ body: '!p-2 grid w-72 max-w-[calc(100vw-1rem)] gap-2' }">
      <div v-if="currentBaseStyleLayerSettings.bevyEnabled" class="grid grid-cols-2 gap-1">
        <UButton
          label="Shadows"
          color="neutral"
          active-color="primary"
          variant="outline-solid"
          :active="currentBaseStyleLayerSettings.shadowsEnabled"
          size="md"
          class="cursor-pointer"
          icon="lucide:sunset"
          @click="
            currentBaseStyleLayerSettings.shadowsEnabled =
              !currentBaseStyleLayerSettings.shadowsEnabled
          "
        />

        <UButton
          label="3D Buildings"
          color="neutral"
          active-color="primary"
          variant="outline-solid"
          :active="currentBaseStyleLayerSettings.buildingsEnabled"
          size="md"
          class="cursor-pointer"
          icon="lucide:building"
          @click="
            currentBaseStyleLayerSettings.buildingsEnabled =
              !currentBaseStyleLayerSettings.buildingsEnabled
          "
        />

        <UButton
          label="Trees"
          color="neutral"
          active-color="primary"
          variant="outline-solid"
          :active="currentBaseStyleLayerSettings.treesEnabled"
          size="md"
          class="cursor-pointer"
          icon="lucide:tree-pine"
          @click="
            currentBaseStyleLayerSettings.treesEnabled = !currentBaseStyleLayerSettings.treesEnabled
          "
        />

        <UButton
          label="Cinematic"
          color="neutral"
          active-color="primary"
          variant="outline-solid"
          :active="currentBaseStyleLayerSettings.cinematicEnabled"
          size="md"
          class="cursor-pointer"
          icon="lucide:film"
          @click="
            currentBaseStyleLayerSettings.cinematicEnabled =
              !currentBaseStyleLayerSettings.cinematicEnabled
          "
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
            label="Fancy"
            color="neutral"
            active-color="primary"
            variant="outline-solid"
            :active="currentBaseStyleLayerSettings.bevyEnabled"
            size="md"
            class="cursor-pointer"
            icon="lucide:star"
            @click="
              currentBaseStyleLayerSettings.bevyEnabled = !currentBaseStyleLayerSettings.bevyEnabled
            "
          />

          <UButton
            label="Terrain"
            color="neutral"
            active-color="primary"
            variant="outline-solid"
            :active="currentBaseStyleLayerSettings.terrainEnabled"
            size="md"
            class="cursor-pointer"
            icon="lucide:mountain"
            @click="
              currentBaseStyleLayerSettings.terrainEnabled =
                !currentBaseStyleLayerSettings.terrainEnabled
            "
          />

          <UButton
            label="Advanced Roads"
            color="neutral"
            active-color="primary"
            variant="outline-solid"
            :active="mapViewStore.advancedRoadsEnabled"
            size="md"
            class="cursor-pointer"
            icon="lucide:route"
            @click="mapViewStore.advancedRoadsEnabled = !mapViewStore.advancedRoadsEnabled"
          />

          <UButton
            label="GTFS"
            color="neutral"
            active-color="primary"
            variant="outline-solid"
            :active="mapViewStore.gtfsEnabled"
            size="md"
            class="cursor-pointer"
            icon="lucide:train-front"
            @click="mapViewStore.gtfsEnabled = !mapViewStore.gtfsEnabled"
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
</script>
