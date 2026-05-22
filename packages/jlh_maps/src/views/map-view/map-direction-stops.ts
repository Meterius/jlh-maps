import { createInjectionState } from '@vueuse/core'
import { shallowRef } from 'vue'
import type { Trip } from 'valhalla_client'
import type { GeoLocation } from '@/components/types.ts'
import { createInjectOrThrow } from '@/composables/helper.ts'

const [provideMapDirectionStops, useMapDirectionStops] = createInjectionState(() => {
  const directionStops = shallowRef<(GeoLocation | null)[]>([null, null])
  const directionsTripPrimary = shallowRef<Trip | null>(null)
  const directionsTripAlternates = shallowRef<Trip[]>([])

  return {
    directionStops,
    directionsTripAlternates,
    directionsTripPrimary,
  }
})

export { provideMapDirectionStops }

export const useMapDirectionStopsOrThrow = createInjectOrThrow(
  useMapDirectionStops,
  'Map direction stops were not provided',
)
