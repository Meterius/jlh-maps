<template>
  <MapPopoverControl icon="lucide:sun" title="Sun">
    <UCard :ui="{ body: '!p-3 grid min-w-80 gap-3' }">
      <div class="grid grid-cols-2 gap-1">
        <UButton
          label="Automatic"
          color="neutral"
          active-color="primary"
          variant="outline-solid"
          :active="mapViewStore.sun.automatic"
          size="md"
          class="cursor-pointer"
          icon="lucide:refresh-cw"
          @click="mapViewStore.sun.automatic = true"
        />

        <UButton
          label="Manual"
          color="neutral"
          active-color="primary"
          variant="outline-solid"
          :active="!mapViewStore.sun.automatic"
          size="md"
          class="cursor-pointer"
          icon="lucide:sliders-horizontal"
          @click="mapViewStore.sun.automatic = false"
        />
      </div>

      <div v-if="mapViewStore.sun.automatic" class="grid gap-2">
        <span class="flex justify-between gap-4 text-sm">
          <span>Time</span>
          <output class="text-muted tabular-nums">
            {{ sunTimeOverridden ? 'Manual' : 'Synced' }} {{ centerLocalUtcOffsetLabel }}
          </output>
        </span>

        <div class="grid grid-cols-2 gap-1">
          <UButton
            label="Sync"
            color="neutral"
            active-color="primary"
            variant="outline-solid"
            :active="!sunTimeOverridden"
            size="md"
            class="cursor-pointer"
            icon="lucide:map-pin"
            @click="setSunTimeSynced"
          />

          <UButton
            label="Override"
            color="neutral"
            active-color="primary"
            variant="outline-solid"
            :active="sunTimeOverridden"
            size="md"
            class="cursor-pointer"
            icon="lucide:clock"
            @click="setSunTimeOverridden"
          />
        </div>

        <UInput
          v-model="sunTimeInputValue"
          type="datetime-local"
          :disabled="!sunTimeOverridden"
          size="md"
        />
      </div>

      <USeparator />

      <label class="grid gap-2">
        <span class="flex justify-between gap-4 text-sm">
          <span>Azimuth</span>
          <output class="text-muted tabular-nums">{{ sunAzimuthLabel }}</output>
        </span>
        <USlider
          v-if="!mapViewStore.sun.automatic"
          v-model.number="mapViewStore.sun.azimuthDegrees"
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
          v-if="!mapViewStore.sun.automatic"
          v-model.number="mapViewStore.sun.elevationDegrees"
          :min="0"
          :max="85"
          :step="1"
        />
      </label>
    </UCard>
  </MapPopoverControl>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useNow } from '@vueuse/core'
import MapPopoverControl from '@/views/map-view/MapPopoverControl.vue'
import { useMapViewStoreOrThrow } from '@/views/map-view/map-view-store.ts'
import {
  formatCenterLocalDateTimeInputValue,
  getCenterLocalUtcOffsetMinutes,
} from '@/views/map-view/map-sun-controller.ts'

const { mapViewStore } = useMapViewStoreOrThrow()
const now = useNow({ interval: 60_000 })

const sunAzimuthLabel = computed(() => `${Math.round(mapViewStore.value.sun.azimuthDegrees)} deg`)

const sunElevationLabel = computed(() => {
  return `${Math.round(mapViewStore.value.sun.elevationDegrees)} deg`
})

const inferredSunTimeInputValue = computed(() =>
  formatCenterLocalDateTimeInputValue(now.value, mapViewStore.value.view.center),
)

const sunTimeOverridden = computed(() => mapViewStore.value.sun.time !== undefined)

const sunTimeInputValue = computed({
  get: () => mapViewStore.value.sun.time ?? inferredSunTimeInputValue.value,
  set: (value) => {
    mapViewStore.value.sun.time = value || undefined
  },
})

const centerLocalUtcOffsetLabel = computed(() =>
  formatUtcOffsetLabel(getCenterLocalUtcOffsetMinutes(mapViewStore.value.view.center)),
)

const setSunTimeSynced = () => {
  mapViewStore.value.sun.time = undefined
}

const setSunTimeOverridden = () => {
  mapViewStore.value.sun.time ??= inferredSunTimeInputValue.value
}

const formatUtcOffsetLabel = (offsetMinutes: number) => {
  const sign = offsetMinutes >= 0 ? '+' : '-'
  const absoluteOffsetMinutes = Math.abs(offsetMinutes)
  const hours = Math.floor(absoluteOffsetMinutes / 60)
  const minutes = absoluteOffsetMinutes % 60

  return `UTC${sign}${hours.toString().padStart(2, '0')}:${minutes.toString().padStart(2, '0')}`
}
</script>
