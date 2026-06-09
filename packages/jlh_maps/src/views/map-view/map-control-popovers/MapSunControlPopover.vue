<template>
  <MapPopoverControl icon="lucide:sun-moon" title="Lighting">
    <UCard :ui="{ body: '!p-3 grid min-w-80 gap-3' }">
      <div class="grid grid-cols-2 gap-1">
        <UButton
          label="Automatic"
          color="neutral"
          active-color="primary"
          variant="outline-solid"
          :active="mapViewStore.lighting.automatic"
          size="md"
          class="cursor-pointer"
          icon="lucide:refresh-cw"
          @click="mapViewStore.lighting.automatic = true"
        />

        <UButton
          label="Manual"
          color="neutral"
          active-color="primary"
          variant="outline-solid"
          :active="!mapViewStore.lighting.automatic"
          size="md"
          class="cursor-pointer"
          icon="lucide:sliders-horizontal"
          @click="mapViewStore.lighting.automatic = false"
        />
      </div>

      <div v-if="mapViewStore.lighting.automatic" class="grid gap-2">
        <USeparator />

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

        <div class="grid gap-2">
          <label class="grid gap-1">
            <span class="text-muted text-xs">Date</span>
            <UInput
              v-model="sunDateInputValue"
              type="date"
              :disabled="!sunTimeOverridden"
              size="md"
            />
          </label>

          <label class="grid gap-2">
            <span class="flex justify-between gap-4 text-sm">
              <span>Local time</span>
              <output class="text-muted tabular-nums">{{ sunTimeOfDayLabel }}</output>
            </span>
            <USlider
              v-model.number="sunTimeOfDayMinutesInputValue"
              :min="0"
              :max="MINUTES_PER_DAY - 1"
              :step="15"
              :disabled="!sunTimeOverridden"
            />
          </label>
        </div>
      </div>

      <USeparator />

      <div class="grid gap-2">
        <span class="text-muted text-xs font-medium uppercase tracking-wide">Sun</span>

        <label class="grid gap-2">
          <span class="flex justify-between gap-4 text-sm">
            <span>Azimuth</span>
            <output class="text-muted tabular-nums">{{ sunAzimuthLabel }}</output>
          </span>
          <USlider
            v-if="!mapViewStore.lighting.automatic"
            v-model.number="mapViewStore.lighting.sun.azimuthDegrees"
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
            v-if="!mapViewStore.lighting.automatic"
            v-model.number="mapViewStore.lighting.sun.elevationDegrees"
            :min="0"
            :max="85"
            :step="1"
          />
        </label>
      </div>

      <USeparator />

      <div class="grid gap-2">
        <span class="text-muted text-xs font-medium uppercase tracking-wide">Moon</span>

        <label class="grid gap-2">
          <span class="flex justify-between gap-4 text-sm">
            <span>Azimuth</span>
            <output class="text-muted tabular-nums">{{ moonAzimuthLabel }}</output>
          </span>
          <USlider
            v-if="!mapViewStore.lighting.automatic"
            v-model.number="mapViewStore.lighting.moon.azimuthDegrees"
            :min="0"
            :max="360"
            :step="1"
          />
        </label>

        <label class="grid gap-2">
          <span class="flex justify-between gap-4 text-sm">
            <span>Elevation</span>
            <output class="text-muted tabular-nums">{{ moonElevationLabel }}</output>
          </span>
          <USlider
            v-if="!mapViewStore.lighting.automatic"
            v-model.number="mapViewStore.lighting.moon.elevationDegrees"
            :min="-90"
            :max="85"
            :step="1"
          />
        </label>
      </div>
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
const MINUTES_PER_HOUR = 60
const MINUTES_PER_DAY = 24 * MINUTES_PER_HOUR

const sunAzimuthLabel = computed(
  () => `${Math.round(mapViewStore.value.lighting.sun.azimuthDegrees)} deg`,
)

const sunElevationLabel = computed(() => {
  return `${Math.round(mapViewStore.value.lighting.sun.elevationDegrees)} deg`
})

const moonAzimuthLabel = computed(
  () => `${Math.round(mapViewStore.value.lighting.moon.azimuthDegrees)} deg`,
)

const moonElevationLabel = computed(() => {
  return `${Math.round(mapViewStore.value.lighting.moon.elevationDegrees)} deg`
})

const inferredSunDateTimeInputValue = computed(() =>
  formatCenterLocalDateTimeInputValue(now.value, mapViewStore.value.view.center),
)

const sunTimeOverridden = computed(() => mapViewStore.value.lighting.time !== undefined)

const sunDateTimeInputValue = computed({
  get: () => mapViewStore.value.lighting.time ?? inferredSunDateTimeInputValue.value,
  set: (value) => {
    mapViewStore.value.lighting.time = value || undefined
  },
})

const sunDateInputValue = computed({
  get: () => splitDateTimeInputValue(sunDateTimeInputValue.value).date,
  set: (date) => {
    if (!date) {
      mapViewStore.value.lighting.time = undefined
      return
    }

    sunDateTimeInputValue.value = joinDateTimeInputValue(
      date,
      splitDateTimeInputValue(sunDateTimeInputValue.value).time,
    )
  },
})

const sunTimeOfDayMinutesInputValue = computed({
  get: () => parseTimeOfDayMinutes(splitDateTimeInputValue(sunDateTimeInputValue.value).time),
  set: (minutes) => {
    sunDateTimeInputValue.value = joinDateTimeInputValue(
      splitDateTimeInputValue(sunDateTimeInputValue.value).date,
      formatTimeOfDayMinutes(minutes),
    )
  },
})

const sunTimeOfDayLabel = computed(() =>
  formatTimeOfDayMinutes(sunTimeOfDayMinutesInputValue.value),
)

const centerLocalUtcOffsetLabel = computed(() =>
  formatUtcOffsetLabel(getCenterLocalUtcOffsetMinutes(mapViewStore.value.view.center)),
)

const setSunTimeSynced = () => {
  mapViewStore.value.lighting.time = undefined
}

const setSunTimeOverridden = () => {
  mapViewStore.value.lighting.time ??= inferredSunDateTimeInputValue.value
}

const splitDateTimeInputValue = (value: string) => {
  const [date = '', time = '00:00'] = value.split('T')

  return {
    date,
    time: time.slice(0, 5),
  }
}

const joinDateTimeInputValue = (date: string, time: string) => `${date}T${time}`

const parseTimeOfDayMinutes = (time: string) => {
  const match = /^(\d{2}):(\d{2})$/.exec(time)
  if (!match) return 0

  const [, hours, minutes] = match

  return clamp(Number(hours) * MINUTES_PER_HOUR + Number(minutes), 0, MINUTES_PER_DAY - 1)
}

const formatTimeOfDayMinutes = (minutes: number) => {
  const clampedMinutes = clamp(Math.round(minutes), 0, MINUTES_PER_DAY - 1)
  const hours = Math.floor(clampedMinutes / MINUTES_PER_HOUR)
  const remainingMinutes = clampedMinutes % MINUTES_PER_HOUR

  return `${hours.toString().padStart(2, '0')}:${remainingMinutes.toString().padStart(2, '0')}`
}

const formatUtcOffsetLabel = (offsetMinutes: number) => {
  const sign = offsetMinutes >= 0 ? '+' : '-'
  const absoluteOffsetMinutes = Math.abs(offsetMinutes)
  const hours = Math.floor(absoluteOffsetMinutes / 60)
  const minutes = absoluteOffsetMinutes % 60

  return `UTC${sign}${hours.toString().padStart(2, '0')}:${minutes.toString().padStart(2, '0')}`
}

const clamp = (value: number, min: number, max: number) => Math.min(Math.max(value, min), max)
</script>
