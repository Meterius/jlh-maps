import { useNow } from '@vueuse/core'
import { Body, Equator, Horizon, Observer } from 'astronomy-engine'
import { watchEffect, type Ref } from 'vue'
import type { MapViewSettings } from 'jlh_maps_app'
import { useMapViewStoreOrThrow, type MapViewStore } from '@/views/map-view/map-view-store.ts'

export type SolarPosition = {
  azimuthDegrees: number
  elevationDegrees: number
}

const MINUTES_PER_HOUR = 60
const MILLISECONDS_PER_MINUTE = 60_000
const SOLAR_CONTROLLER_UPDATE_INTERVAL_MS = 60_000

export function useMapSunController(
  bevyMapViewSettings?: () => Ref<MapViewSettings> | null | undefined,
) {
  const { mapViewStore } = useMapViewStoreOrThrow()
  const now = useNow({ interval: SOLAR_CONTROLLER_UPDATE_INTERVAL_MS })

  watchEffect(() => {
    if (!mapViewStore.value.sun.automatic) return

    const [longitudeDegrees, latitudeDegrees] = mapViewStore.value.view.center
    const position = calculateAstronomyEngineSolarPosition({
      date: getSunCalculationDate(
        mapViewStore.value.sun,
        mapViewStore.value.view.center,
        now.value,
      ),
      latitudeDegrees,
      longitudeDegrees,
    })

    mapViewStore.value.sun.azimuthDegrees = roundToPrecision(position.azimuthDegrees, 2)
    mapViewStore.value.sun.elevationDegrees = roundToPrecision(position.elevationDegrees, 2)
  })

  watchEffect(() => {
    const mapViewSettings = bevyMapViewSettings?.()
    if (!mapViewSettings) return

    mapViewSettings.value.sunAzimuthDegrees = mapViewStore.value.sun.azimuthDegrees
    mapViewSettings.value.sunElevationDegrees = mapViewStore.value.sun.elevationDegrees
  })

  return {
    now,
  }
}

export function getSunCalculationDate(
  sun: Pick<MapViewStore['sun'], 'time'>,
  center: [number, number],
  now: Date,
) {
  if (!sun.time) return now

  return parseCenterLocalDateTimeInputValue(sun.time, center) ?? now
}

// Time

export function formatCenterLocalDateTimeInputValue(date: Date, center: [number, number]) {
  const offsetDate = new Date(
    date.getTime() + getCenterLocalUtcOffsetMinutes(center) * MILLISECONDS_PER_MINUTE,
  )

  return [
    offsetDate.getUTCFullYear().toString().padStart(4, '0'),
    '-',
    (offsetDate.getUTCMonth() + 1).toString().padStart(2, '0'),
    '-',
    offsetDate.getUTCDate().toString().padStart(2, '0'),
    'T',
    offsetDate.getUTCHours().toString().padStart(2, '0'),
    ':',
    offsetDate.getUTCMinutes().toString().padStart(2, '0'),
  ].join('')
}

export function parseCenterLocalDateTimeInputValue(value: string, center: [number, number]) {
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})$/.exec(value)
  if (!match) return null

  const [, year, month, day, hour, minute] = match
  const localDateMilliseconds = Date.UTC(
    Number(year),
    Number(month) - 1,
    Number(day),
    Number(hour),
    Number(minute),
  )

  return new Date(
    localDateMilliseconds - getCenterLocalUtcOffsetMinutes(center) * MILLISECONDS_PER_MINUTE,
  )
}

export function getCenterLocalUtcOffsetMinutes([longitudeDegrees]: [number, number]) {
  const normalizedLongitude = normalizeLongitude(longitudeDegrees)

  return clamp(Math.round(normalizedLongitude / 15) * MINUTES_PER_HOUR, -12 * 60, 14 * 60)
}

// Astronomy Engine horizontal coordinates:
// https://github.com/cosinekitty/astronomy
export function calculateAstronomyEngineSolarPosition({
  date,
  latitudeDegrees,
  longitudeDegrees,
}: {
  date: Date
  latitudeDegrees: number
  longitudeDegrees: number
}): SolarPosition {
  const observer = new Observer(
    clamp(latitudeDegrees, -90, 90),
    normalizeLongitude(longitudeDegrees),
    0,
  )
  const equatorial = Equator(Body.Sun, date, observer, true, true)
  const horizontal = Horizon(date, observer, equatorial.ra, equatorial.dec, 'normal')

  return {
    azimuthDegrees: horizontal.azimuth,
    elevationDegrees: horizontal.altitude,
  }
}

function normalizeLongitude(longitudeDegrees: number) {
  return ((((longitudeDegrees + 180) % 360) + 360) % 360) - 180
}

function roundToPrecision(value: number, precision: number) {
  const multiplier = 10 ** precision

  return Math.round(value * multiplier) / multiplier
}

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), max)
}
