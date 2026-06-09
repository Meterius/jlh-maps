import { useNow } from '@vueuse/core'
import { Body, Equator, Horizon, Observer } from 'astronomy-engine'
import { watchEffect, type Ref } from 'vue'
import type { MapViewSettings } from 'jlh_maps_app'
import { useMapViewStoreOrThrow, type MapViewStore } from '@/views/map-view/map-view-store.ts'

export type SolarPosition = {
  azimuthDegrees: number
  elevationDegrees: number
}

export type CelestialBodyPosition = SolarPosition

const MINUTES_PER_HOUR = 60
const MILLISECONDS_PER_MINUTE = 60_000
const SOLAR_CONTROLLER_UPDATE_INTERVAL_MS = 60_000

export function useMapSunController(
  bevyMapViewSettings?: () => Ref<MapViewSettings> | null | undefined,
) {
  const { mapViewStore } = useMapViewStoreOrThrow()
  const now = useNow({ interval: SOLAR_CONTROLLER_UPDATE_INTERVAL_MS })

  watchEffect(() => {
    const lighting = mapViewStore.value.lighting
    if (!lighting.automatic) return

    const [longitudeDegrees, latitudeDegrees] = mapViewStore.value.view.center
    const date = getSunCalculationDate(lighting, mapViewStore.value.view.center, now.value)
    const sunPosition = calculateAstronomyEngineBodyPosition({
      body: Body.Sun,
      date,
      latitudeDegrees,
      longitudeDegrees,
    })
    const moonPosition = calculateAstronomyEngineBodyPosition({
      body: Body.Moon,
      date,
      latitudeDegrees,
      longitudeDegrees,
    })

    lighting.sun.azimuthDegrees = roundToPrecision(sunPosition.azimuthDegrees, 2)
    lighting.sun.elevationDegrees = roundToPrecision(sunPosition.elevationDegrees, 2)
    lighting.moon.azimuthDegrees = roundToPrecision(moonPosition.azimuthDegrees, 2)
    lighting.moon.elevationDegrees = roundToPrecision(moonPosition.elevationDegrees, 2)
  })

  watchEffect(() => {
    const mapViewSettings = bevyMapViewSettings?.()
    if (!mapViewSettings) return

    const { lighting } = mapViewStore.value
    mapViewSettings.value.sunAzimuthDegrees = lighting.sun.azimuthDegrees
    mapViewSettings.value.sunElevationDegrees = lighting.sun.elevationDegrees
    mapViewSettings.value.moonAzimuthDegrees = lighting.moon.azimuthDegrees
    mapViewSettings.value.moonElevationDegrees = lighting.moon.elevationDegrees
  })

  return {
    now,
  }
}

export function getSunCalculationDate(
  lighting: Pick<MapViewStore['lighting'], 'time'>,
  center: [number, number],
  now: Date,
) {
  if (!lighting.time) return now

  return parseCenterLocalDateTimeInputValue(lighting.time, center) ?? now
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
  return calculateAstronomyEngineBodyPosition({
    body: Body.Sun,
    date,
    latitudeDegrees,
    longitudeDegrees,
  })
}

export function calculateAstronomyEngineBodyPosition({
  body,
  date,
  latitudeDegrees,
  longitudeDegrees,
}: {
  body: Body
  date: Date
  latitudeDegrees: number
  longitudeDegrees: number
}): CelestialBodyPosition {
  const observer = new Observer(
    clamp(latitudeDegrees, -90, 90),
    normalizeLongitude(longitudeDegrees),
    0,
  )
  const equatorial = Equator(body, date, observer, true, true)
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
