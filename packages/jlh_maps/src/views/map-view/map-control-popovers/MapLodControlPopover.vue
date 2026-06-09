<template>
  <MapPopoverControl icon="lucide:sliders-horizontal" title="Tile LOD">
    <UCard :ui="{ body: '!p-3 grid min-w-72 gap-3' }">
      <label class="grid gap-2">
        <span class="flex justify-between gap-4 text-sm">
          <span>Max zoom levels</span>
          <output class="text-muted tabular-nums">
            {{ lodMaxZoomLevelsOnScreenLabel }}
          </output>
        </span>
        <USlider
          v-model.number="mapViewStore.lod.maxZoomLevelsOnScreen"
          :min="1"
          :max="12"
          :step="0.25"
        />
      </label>

      <label class="grid gap-2">
        <span class="flex justify-between gap-4 text-sm">
          <span>Tile count ratio</span>
          <output class="text-muted tabular-nums">
            {{ lodTileCountMaxMinRatioLabel }}
          </output>
        </span>
        <USlider
          v-model.number="mapViewStore.lod.tileCountMaxMinRatio"
          :min="1"
          :max="8"
          :step="0.25"
        />
      </label>

      <UButton
        label="Reset"
        color="neutral"
        variant="outline-solid"
        size="md"
        class="cursor-pointer"
        icon="lucide:rotate-ccw"
        @click="resetLod"
      />
    </UCard>
  </MapPopoverControl>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import MapPopoverControl from '@/views/map-view/MapPopoverControl.vue'
import {
  DEFAULT_MAPLIBRE_LOD_SETTINGS,
  useMapViewStoreOrThrow,
} from '@/views/map-view/map-view-store.ts'

const { mapViewStore } = useMapViewStoreOrThrow()

const lodMaxZoomLevelsOnScreenLabel = computed(() =>
  mapViewStore.value.lod.maxZoomLevelsOnScreen.toFixed(2),
)
const lodTileCountMaxMinRatioLabel = computed(() =>
  mapViewStore.value.lod.tileCountMaxMinRatio.toFixed(2),
)

const resetLod = () => {
  mapViewStore.value.lod = { ...DEFAULT_MAPLIBRE_LOD_SETTINGS }
}
</script>
