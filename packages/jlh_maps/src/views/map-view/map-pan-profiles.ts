import type { DragPanOptions } from 'maplibre-gl'
import { useMap } from '@indoorequal/vue-maplibre-gl'
import { watchDefinedOnce } from '@/composables/helper.ts'
import { onWatcherCleanup } from 'vue'

const DESKTOP_DRAG_PAN_OPTIONS: DragPanOptions = {
  linearity: 0.35,
  maxSpeed: 3200,
  deceleration: 6000,
  easing: lateBrakeDragPanEasing,
}

const MOBILE_DRAG_PAN_OPTIONS: DragPanOptions = {
  linearity: 0.7,
  maxSpeed: 4800,
  deceleration: 2000,
  easing: gradualFadeDragPanEasing,
}

function lateBrakeDragPanEasing(t: number) {
  const x = clampUnit(t)

  return (4 * x - x ** 4) / 3
}

function gradualFadeDragPanEasing(t: number) {
  const x = clampUnit(t)

  return 1 - (1 - x) ** 2
}

function clampUnit(value: number) {
  return Math.max(0, Math.min(1, value))
}

export function usePanProfiles(mapKey: string | symbol | undefined) {
  const mapInstance = useMap(mapKey)

  const { stop } = watchDefinedOnce(
    () => mapInstance.map,
    (map) => {
      const canvasContainer = map.getCanvasContainer()
      const useDesktopProfile = () => map.dragPan.enable(DESKTOP_DRAG_PAN_OPTIONS)
      const useMobileProfile = () => map.dragPan.enable(MOBILE_DRAG_PAN_OPTIONS)
      const usePointerProfile = (event: PointerEvent) => {
        if (event.pointerType === 'touch' || event.pointerType === 'pen') {
          useMobileProfile()
          return
        }

        useDesktopProfile()
      }

      if (window.matchMedia('(pointer: coarse)').matches) {
        useMobileProfile()
      } else {
        useDesktopProfile()
      }

      canvasContainer.addEventListener('pointerdown', usePointerProfile, { capture: true })
      canvasContainer.addEventListener('mousedown', useDesktopProfile, { capture: true })
      canvasContainer.addEventListener('touchstart', useMobileProfile, {
        capture: true,
        passive: true,
      })

      onWatcherCleanup(() => {
        canvasContainer.removeEventListener('pointerdown', usePointerProfile, { capture: true })
        canvasContainer.removeEventListener('mousedown', useDesktopProfile, { capture: true })
        canvasContainer.removeEventListener('touchstart', useMobileProfile, { capture: true })

        // reset drag pan configuration
        map.dragPan.enable()
      })
    },
  )

  return {
    // remove pan profiles, automatically invoked when unmounted
    remove: () => stop(),
  }
}
