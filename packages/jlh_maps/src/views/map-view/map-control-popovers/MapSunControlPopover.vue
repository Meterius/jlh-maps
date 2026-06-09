<template>
  <MapPopoverControl icon="lucide:sun" title="Sun">
    <UCard :ui="{ body: '!p-3 grid min-w-72 gap-3' }">
      <label class="grid gap-2">
        <span class="flex justify-between gap-4 text-sm">
          <span>Azimuth</span>
          <output class="text-muted tabular-nums">{{ sunAzimuthLabel }}</output>
        </span>
        <USlider v-model.number="mapViewStore.sun.azimuthDegrees" :min="0" :max="360" :step="1" />
      </label>

      <label class="grid gap-2">
        <span class="flex justify-between gap-4 text-sm">
          <span>Elevation</span>
          <output class="text-muted tabular-nums">{{ sunElevationLabel }}</output>
        </span>
        <USlider v-model.number="mapViewStore.sun.elevationDegrees" :min="0" :max="85" :step="1" />
      </label>
    </UCard>
  </MapPopoverControl>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import MapPopoverControl from '@/views/map-view/MapPopoverControl.vue'
import { useMapViewStoreOrThrow } from '@/views/map-view/map-view-store.ts'

const { mapViewStore } = useMapViewStoreOrThrow()

const sunAzimuthLabel = computed(() => `${Math.round(mapViewStore.value.sun.azimuthDegrees)} deg`)

const sunElevationLabel = computed(() => {
  return `${Math.round(mapViewStore.value.sun.elevationDegrees)} deg`
})
</script>
