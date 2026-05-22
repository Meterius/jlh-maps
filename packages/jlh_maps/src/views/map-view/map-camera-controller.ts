import type { Trip } from 'valhalla_client'
import { createInjectionState } from '@vueuse/core'
import { useMap } from '@indoorequal/vue-maplibre-gl'
import { getTripBounds } from '@/utils/valhalla.ts'
import { type MaybeRefOrGetter, toValue } from 'vue'
import type { FitBoundsOptions } from 'maplibre-gl'
import { createInjectOrThrow } from '@/composables/helper.ts'

export type UseMapCameraControllerOptions = {
  viewPadding: MaybeRefOrGetter<FitBoundsOptions['padding']>
}

const [provideMapCameraController, useMapCameraController] = createInjectionState(
  (mapKey: string | symbol | undefined, { viewPadding }: UseMapCameraControllerOptions) => {
    const mapInstance = useMap(mapKey)

    return {
      fitTrip: (trip: Trip, options?: Omit<FitBoundsOptions, 'padding'>) => {
        const bounds = getTripBounds(trip)

        if (!bounds) {
          console.warn('No bounds found for trip')
          return
        }

        mapInstance.map?.fitBounds(bounds, {
          ...options,
          padding: toValue(viewPadding),
        })
      },
    }
  },
)

export { provideMapCameraController }

export const useMapCameraControllerOrThrow = createInjectOrThrow(
  useMapCameraController,
  'Map camera controller was not provided',
)
