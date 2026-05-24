import {
  getCurrentScope,
  ref,
  shallowRef,
  watch,
  watchEffect,
  type Ref,
  type ShallowRef,
} from 'vue'
import {
  MapViewCameraSettings as MapViewCameraSettingsBevy,
  MapViewSettings as MapViewSettingsBevy,
  forward_cursor_entered,
  forward_cursor_left,
  forward_cursor_moved,
  forward_focus,
  forward_keyboard_input,
  forward_mouse_button,
  forward_mouse_wheel,
  mount as mountBevyInstance,
  resize,
  set_map_view_camera_settings,
  set_map_view_settings,
  tick as tickBevyInstance,
  unmount as unmountBevyInstance,
} from 'jlh_maps_app'
import { useEventListener, useResizeObserver } from '@vueuse/core'
import { onScopeDisposeLifo } from '@/composables/helper.ts'

const DEFAULT_SUN_AZIMUTH_DEGREES = 11.31
const DEFAULT_SUN_ELEVATION_DEGREES = 32.52

interface CanvasRenderSize {
  width: number
  height: number
  scaleFactor: number
}

export interface BevyMapViewSettings {
  enable_window_cameras: boolean
  enable_buildings: boolean
  enable_waters: boolean
  enable_shadows: boolean
  sun_azimuth_degrees: number
  sun_elevation_degrees: number
}

export interface BevyMapViewCameraSettings {
  enable_color_grading: boolean
  enable_tonemapping: boolean
  enable_msaa: boolean
  enable_ssao: boolean
  enable_taa: boolean
}

interface BevyInstanceState {
  instanceId: string
  isMounted: Ref<boolean>

  debugCanvas: ShallowRef<HTMLCanvasElement | null>
  maplibreCanvas: ShallowRef<HTMLCanvasElement | null>

  debugOffscreenCanvas: ShallowRef<OffscreenCanvas | null>
  textureOffscreenCanvas: ShallowRef<OffscreenCanvas | null>

  mapViewSettings: Ref<BevyMapViewSettings>
  mapViewCameraSettings: Ref<BevyMapViewCameraSettings>
}

const bevyInstances = new Map<string, BevyInstanceState>()
const attachedCanvasInstances = new WeakMap<HTMLCanvasElement, string>()
const transferredDebugCanvases = new WeakMap<HTMLCanvasElement, OffscreenCanvas>()

let bevyInstanceCounter = 0

export function mountBevy(
  getDebugCanvas: () => HTMLCanvasElement | null,
  getMaplibreCanvas: () => HTMLCanvasElement | null,
) {
  if (!getCurrentScope()) {
    throw new Error('mountBevy must be called within an active effect scope')
  }

  const instanceId = makeBevyInstanceId()
  const state: BevyInstanceState = {
    instanceId,
    isMounted: ref(false),
    debugCanvas: shallowRef(null),
    maplibreCanvas: shallowRef(null),
    debugOffscreenCanvas: shallowRef(null),
    textureOffscreenCanvas: shallowRef(null),
    mapViewSettings: ref({
      enable_window_cameras: false,
      enable_buildings: true,
      enable_waters: true,
      enable_shadows: true,
      sun_elevation_degrees: DEFAULT_SUN_ELEVATION_DEGREES,
      sun_azimuth_degrees: DEFAULT_SUN_AZIMUTH_DEGREES,
    }),
    mapViewCameraSettings: ref({
      enable_color_grading: true,
      enable_tonemapping: false,
      enable_msaa: true,
      enable_ssao: false,
      enable_taa: false,
    }),
  }

  bevyInstances.set(instanceId, state)

  useStaticCanvasSource(instanceId, 'debug', getDebugCanvas, state.debugCanvas)
  useStaticCanvasSource(instanceId, 'maplibre', getMaplibreCanvas, state.maplibreCanvas)

  const debugCanvasRenderSize = shallowRef<CanvasRenderSize | null>(null)
  const maplibreCanvasRenderSize = shallowRef<CanvasRenderSize | null>(null)

  const updateDebugCanvasRenderSize = () => {
    const canvas = state.debugCanvas.value
    if (!canvas) return

    debugCanvasRenderSize.value = canvasRenderSize(canvas)
  }

  const updateMaplibreCanvasRenderSize = () => {
    const canvas = state.maplibreCanvas.value
    if (!canvas) return

    maplibreCanvasRenderSize.value = canvasRenderSize(canvas)
  }

  watch(state.debugCanvas, updateDebugCanvasRenderSize, { immediate: true })
  watch(state.maplibreCanvas, updateMaplibreCanvasRenderSize, { immediate: true })

  useResizeObserver(state.debugCanvas, updateDebugCanvasRenderSize)
  useResizeObserver(state.maplibreCanvas, updateMaplibreCanvasRenderSize)

  watchEffect(() => {
    if (state.isMounted.value) {
      resizeMountedBevyInstance(state, debugCanvasRenderSize.value, maplibreCanvasRenderSize.value)
    }
  })

  watchEffect(() => {
    if (state.isMounted.value) return

    const debugCanvas = state.debugCanvas.value
    const maplibreCanvas = state.maplibreCanvas.value
    if (!debugCanvas || !maplibreCanvas) return

    const debugSize = canvasRenderSize(debugCanvas)
    const maplibreSize = canvasRenderSize(maplibreCanvas)

    debugCanvasRenderSize.value = debugSize
    maplibreCanvasRenderSize.value = maplibreSize

    try {
      mountRegisteredBevyInstance(state, debugSize, maplibreSize)
    } catch (error) {
      disposeBevyInstance(instanceId)
      throw error
    }
  })

  watchEffect(() => {
    if (!state.isMounted.value) return

    set_map_view_settings(instanceId, createMapViewSettingsSnapshot(state.mapViewSettings.value))
  })

  watchEffect(() => {
    if (!state.isMounted.value) return

    set_map_view_camera_settings(
      instanceId,
      createMapViewCameraSettingsSnapshot(state.mapViewCameraSettings.value),
    )
  })

  useForwardDebugCanvasEvents(state.debugCanvas, state.isMounted, instanceId)

  onScopeDisposeLifo(() => {
    disposeBevyInstance(instanceId)
  })

  return {
    instanceId,
  }
}

export function useBevy(instanceId: string) {
  const state = getBevyInstanceOrThrow(instanceId)

  return {
    isMounted: state.isMounted,
    debugCanvas: state.debugCanvas,
    textureCanvas: state.maplibreCanvas,
    debugOffscreenCanvas: state.debugOffscreenCanvas,
    textureOffscreenCanvas: state.textureOffscreenCanvas,
    mapViewSettings: state.mapViewSettings,
    mapViewCameraSettings: state.mapViewCameraSettings,
    tick: () => {
      if (!state.isMounted.value) return false

      tickBevyInstance(instanceId)
      return true
    },
  }
}

function makeBevyInstanceId() {
  bevyInstanceCounter += 1
  return `bevy-instance-${bevyInstanceCounter}`
}

function getBevyInstanceOrThrow(instanceId: string) {
  const state = bevyInstances.get(instanceId)

  if (!state) {
    throw new Error(`No registered Bevy instance found for id ${instanceId}`)
  }

  return state
}

function useStaticCanvasSource(
  instanceId: string,
  role: 'debug' | 'maplibre',
  getCanvas: () => HTMLCanvasElement | null,
  target: ShallowRef<HTMLCanvasElement | null>,
) {
  watch(
    getCanvas,
    (canvas) => {
      if (!canvas) return

      if (target.value) {
        if (target.value !== canvas) {
          throw new Error(`Bevy ${role} canvas is static and cannot change`)
        }

        return
      }

      assertCanvasUnattached(instanceId, canvas, role)
      target.value = canvas
      attachedCanvasInstances.set(canvas, instanceId)
    },
    { immediate: true, flush: 'post' },
  )
}

function mountRegisteredBevyInstance(
  state: BevyInstanceState,
  debugSize: CanvasRenderSize,
  maplibreSize: CanvasRenderSize,
) {
  const debugCanvas = state.debugCanvas.value
  const maplibreCanvas = state.maplibreCanvas.value

  if (!debugCanvas || !maplibreCanvas) return

  if (debugCanvas === maplibreCanvas) {
    throw new Error('Bevy debug canvas and MapLibre canvas must be different canvases')
  }

  try {
    debugCanvas.tabIndex = 0
    state.debugOffscreenCanvas.value = getDebugOffscreenCanvas(debugCanvas, debugSize)
    state.textureOffscreenCanvas.value = new OffscreenCanvas(
      maplibreSize.width,
      maplibreSize.height,
    )

    mountBevyInstance(
      state.instanceId,
      state.debugOffscreenCanvas.value,
      state.textureOffscreenCanvas.value,
    )
    state.isMounted.value = true

    resizeMountedBevyInstance(state, debugSize, maplibreSize)
  } catch (error) {
    const prevIsMounted = state.isMounted.value
    state.isMounted.value = false

    if (prevIsMounted) {
      unmountBevyInstance(state.instanceId)
    }

    state.debugOffscreenCanvas.value = null
    state.textureOffscreenCanvas.value = null
    throw error
  }
}

function getDebugOffscreenCanvas(
  debugCanvas: HTMLCanvasElement,
  debugSize: CanvasRenderSize,
): OffscreenCanvas {
  const transferredCanvas = transferredDebugCanvases.get(debugCanvas)

  if (transferredCanvas) {
    transferredCanvas.width = debugSize.width
    transferredCanvas.height = debugSize.height
    return transferredCanvas
  }

  debugCanvas.width = debugSize.width
  debugCanvas.height = debugSize.height

  const offscreenCanvas = debugCanvas.transferControlToOffscreen()
  transferredDebugCanvases.set(debugCanvas, offscreenCanvas)
  return offscreenCanvas
}

function resizeMountedBevyInstance(
  state: BevyInstanceState,
  debugSize: CanvasRenderSize | null,
  maplibreSize: CanvasRenderSize | null,
) {
  if (!state.isMounted.value || !debugSize || !maplibreSize) return

  if (
    state.debugOffscreenCanvas.value &&
    (state.debugOffscreenCanvas.value.width !== debugSize.width ||
      state.debugOffscreenCanvas.value.height !== debugSize.height)
  ) {
    state.debugOffscreenCanvas.value.width = debugSize.width
    state.debugOffscreenCanvas.value.height = debugSize.height
  }

  if (
    state.textureOffscreenCanvas.value &&
    (state.textureOffscreenCanvas.value.width !== maplibreSize.width ||
      state.textureOffscreenCanvas.value.height !== maplibreSize.height)
  ) {
    state.textureOffscreenCanvas.value.width = maplibreSize.width
    state.textureOffscreenCanvas.value.height = maplibreSize.height
  }

  resize(
    state.instanceId,
    debugSize.width,
    debugSize.height,
    maplibreSize.width,
    maplibreSize.height,
    maplibreSize.scaleFactor,
  )
}

function disposeBevyInstance(instanceId: string) {
  const state = bevyInstances.get(instanceId)

  if (!state) return

  bevyInstances.delete(instanceId)

  const wasMounted = state.isMounted.value
  state.isMounted.value = false

  if (wasMounted) {
    unmountBevyInstance(instanceId)
  }

  releaseAttachedCanvas(state.debugCanvas.value, instanceId)
  releaseAttachedCanvas(state.maplibreCanvas.value, instanceId)

  state.debugCanvas.value = null
  state.maplibreCanvas.value = null
  state.debugOffscreenCanvas.value = null
  state.textureOffscreenCanvas.value = null
}

function assertCanvasUnattached(
  instanceId: string,
  canvas: HTMLCanvasElement,
  role: 'debug' | 'maplibre',
) {
  const ownerInstanceId = attachedCanvasInstances.get(canvas)

  if (ownerInstanceId) {
    throw new Error(
      `Cannot mount Bevy instance ${instanceId}: ${role} canvas is already attached to Bevy instance ${ownerInstanceId}`,
    )
  }
}

function releaseAttachedCanvas(canvas: HTMLCanvasElement | null, instanceId: string) {
  if (canvas && attachedCanvasInstances.get(canvas) === instanceId) {
    attachedCanvasInstances.delete(canvas)
  }
}

function createMapViewSettingsSnapshot(settings: BevyMapViewSettings) {
  return new MapViewSettingsBevy(
    settings.enable_window_cameras,
    settings.enable_buildings,
    settings.enable_waters,
    settings.enable_shadows,
    settings.sun_azimuth_degrees,
    settings.sun_elevation_degrees,
  )
}

function createMapViewCameraSettingsSnapshot(settings: BevyMapViewCameraSettings) {
  return new MapViewCameraSettingsBevy(
    settings.enable_color_grading,
    settings.enable_tonemapping,
    settings.enable_msaa,
    settings.enable_ssao,
    settings.enable_taa,
  )
}

function useForwardDebugCanvasEvents(
  canvas: ShallowRef<HTMLCanvasElement | null>,
  isMounted: Ref<boolean>,
  instanceId: string,
) {
  const canvasPosition = (event: MouseEvent | PointerEvent) => {
    const currentCanvas = canvas.value
    if (!currentCanvas) return null

    const rect = currentCanvas.getBoundingClientRect()
    return {
      x: event.clientX - rect.left,
      y: event.clientY - rect.top,
    }
  }

  const onlyMounted = (callback: () => void) => {
    if (!isMounted.value) return

    callback()
  }

  useEventListener(canvas, 'pointerenter', () => {
    onlyMounted(() => forward_cursor_entered(instanceId))
  })

  useEventListener(canvas, 'pointerleave', () => {
    onlyMounted(() => forward_cursor_left(instanceId))
  })

  useEventListener(canvas, 'pointermove', (event) => {
    if (!isMounted.value) return

    event.preventDefault()
    const position = canvasPosition(event)
    if (!position) return

    forward_cursor_moved(instanceId, position.x, position.y, event.movementX, event.movementY)
  })

  useEventListener(canvas, 'pointerdown', (event) => {
    const currentCanvas = canvas.value
    if (!isMounted.value || !currentCanvas) return

    event.preventDefault()
    currentCanvas.focus({ preventScroll: true })
    if (!currentCanvas.hasPointerCapture(event.pointerId)) {
      currentCanvas.setPointerCapture(event.pointerId)
    }
    forward_mouse_button(instanceId, event.button, true)
  })

  useEventListener(canvas, 'pointerup', (event) => {
    const currentCanvas = canvas.value
    if (!isMounted.value || !currentCanvas) return

    event.preventDefault()
    if (currentCanvas.hasPointerCapture(event.pointerId)) {
      currentCanvas.releasePointerCapture(event.pointerId)
    }
    forward_mouse_button(instanceId, event.button, false)
  })

  useEventListener(canvas, 'pointercancel', (event) => {
    const currentCanvas = canvas.value
    if (!currentCanvas) return

    if (currentCanvas.hasPointerCapture(event.pointerId)) {
      currentCanvas.releasePointerCapture(event.pointerId)
    }
  })

  useEventListener(
    canvas,
    'wheel',
    (event) => {
      if (!isMounted.value) return

      event.preventDefault()
      forward_mouse_wheel(instanceId, event.deltaX, event.deltaY, event.deltaMode)
    },
    { passive: false },
  )

  useEventListener(canvas, 'focus', () => {
    onlyMounted(() => forward_focus(instanceId, true))
  })

  useEventListener(canvas, 'blur', () => {
    onlyMounted(() => forward_focus(instanceId, false))
  })

  useEventListener(canvas, 'keydown', (event) => {
    onlyMounted(() => forward_keyboard_input(instanceId, event.code, event.key, true, event.repeat))
  })

  useEventListener(canvas, 'keyup', (event) => {
    onlyMounted(() =>
      forward_keyboard_input(instanceId, event.code, event.key, false, event.repeat),
    )
  })
}

function canvasRenderSize(canvas: HTMLCanvasElement): CanvasRenderSize {
  const clientWidth = Math.round(canvas.clientWidth * devicePixelRatio)
  const clientHeight = Math.round(canvas.clientHeight * devicePixelRatio)
  const width = Math.max(1, clientWidth || canvas.width)
  const height = Math.max(1, clientHeight || canvas.height)
  const scaleFactor = Math.max(
    1,
    canvas.clientWidth > 0 ? width / canvas.clientWidth : devicePixelRatio,
  )

  return {
    width,
    height,
    scaleFactor,
  }
}
