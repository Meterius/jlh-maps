import {
  getCurrentScope,
  type MaybeRefOrGetter,
  ref,
  type Ref,
  shallowRef,
  type ShallowRef,
  toValue,
  watch,
  watchEffect,
} from 'vue'
import type { MapViewCameraSettings, MapViewSettings, WindowInstanceRef } from 'jlh_maps_app'
import { useEventListener, useResizeObserver } from '@vueuse/core'
import { releaseProxy, transfer, wrap, type Remote } from 'comlink'
import { onScopeDisposeLifo, watchDefinedOnce } from '@/composables/helper.ts'
import BevyWorker from './bevy.worker?worker'
import type { BevyInstance, CanvasRenderSize } from './bevy.worker'
import { isEqual } from 'lodash'
import { coalesceTrailing } from '@/utils/helper.ts'

export type UseBevyReturn = ReturnType<typeof useBevy>

const DEFAULT_SUN_AZIMUTH_DEGREES = 11.31
const DEFAULT_SUN_ELEVATION_DEGREES = 32.52
const DEFAULT_MOON_AZIMUTH_DEGREES = 191.31
const DEFAULT_MOON_ELEVATION_DEGREES = -32.52
const DEFAULT_FEATURE_VISIBILITY_DISTANCE = 10

interface BevyInstanceState {
  instanceId: string
  isMounted: Ref<boolean>

  debugCanvas: ShallowRef<HTMLCanvasElement | null>
  maplibreCanvas: ShallowRef<HTMLCanvasElement | null>

  bevyInstance: ShallowRef<Remote<BevyInstance> | null>
  bevyWorker: ShallowRef<Worker | null>

  mapViewSettings: Ref<MapViewSettings>
  mapViewCameraSettings: Ref<MapViewCameraSettings>

  refreshWindowSizes: () => Promise<void>
}

const bevyInstances = new Map<string, BevyInstanceState>()
const attachedCanvasInstances = new WeakMap<HTMLCanvasElement, string>()

let bevyInstanceCounter = 0

export function mountBevy(
  getDebugCanvas: () => HTMLCanvasElement | null,
  getMaplibreCanvas: () => HTMLCanvasElement | null,
  mountDebugWindow: boolean,
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
    bevyInstance: shallowRef(null),
    bevyWorker: shallowRef(null),
    mapViewSettings: ref({
      enableBuildings: true,
      enableWaters: true,
      enableTrees: true,
      enableShadows: true,
      sunElevationDegrees: DEFAULT_SUN_ELEVATION_DEGREES,
      sunAzimuthDegrees: DEFAULT_SUN_AZIMUTH_DEGREES,
      moonElevationDegrees: DEFAULT_MOON_ELEVATION_DEGREES,
      moonAzimuthDegrees: DEFAULT_MOON_AZIMUTH_DEGREES,
      featureVisibilityDistance: DEFAULT_FEATURE_VISIBILITY_DISTANCE,
    }),
    mapViewCameraSettings: ref({
      enableColorGrading: true,
      enableTonemapping: false,
      enableMsaa: true,
      enableSsao: false,
      enableTaa: false,
    }),
    refreshWindowSizes: coalesceTrailing(async () => {
      const debugSize = targetWindowSize.debug.value
      const maplibreSize = targetWindowSize.maplibre.value

      if (
        state.bevyInstance.value &&
        maplibreSize &&
        (!isEqual(appliedWindowSize.debug.value, debugSize) ||
          !isEqual(appliedWindowSize.maplibre.value, maplibreSize))
      ) {
        const applied = await state.bevyInstance.value.resize(debugSize, maplibreSize)

        if (applied) {
          appliedWindowSize.maplibre.value = maplibreSize
          appliedWindowSize.debug.value = debugSize
        }
      }
    }),
  }

  bevyInstances.set(instanceId, state)

  useStaticCanvasSource(instanceId, 'debug', getDebugCanvas, state.debugCanvas)
  useStaticCanvasSource(instanceId, 'maplibre', getMaplibreCanvas, state.maplibreCanvas)

  const targetWindowSize = {
    debug: shallowRef<CanvasRenderSize | null>(null),
    maplibre: shallowRef<CanvasRenderSize | null>(null),
  }

  const appliedWindowSize = {
    debug: shallowRef<CanvasRenderSize | null>(null),
    maplibre: shallowRef<CanvasRenderSize | null>(null),
  }

  const updateDebugCanvasRenderSize = () => {
    const canvas = state.debugCanvas.value
    if (!canvas) {
      targetWindowSize.debug.value = null
      return
    }

    targetWindowSize.debug.value = canvasRenderSize(canvas)
  }

  const updateMaplibreCanvasRenderSize = () => {
    const canvas = state.maplibreCanvas.value
    if (!canvas) return

    targetWindowSize.maplibre.value = canvasRenderSize(canvas)
  }

  watch(state.debugCanvas, updateDebugCanvasRenderSize, { immediate: true })
  watch(state.maplibreCanvas, updateMaplibreCanvasRenderSize, { immediate: true })

  useResizeObserver(state.debugCanvas, updateDebugCanvasRenderSize)
  useResizeObserver(state.maplibreCanvas, updateMaplibreCanvasRenderSize)

  watchDefinedOnce(
    () => {
      const maplibreCanvas = state.maplibreCanvas.value
      const debugCanvas = mountDebugWindow ? state.debugCanvas.value : null

      if (!maplibreCanvas || (mountDebugWindow && !debugCanvas)) return undefined

      return {
        debugCanvas,
        maplibreCanvas,
      }
    },
    ({ debugCanvas, maplibreCanvas }) => {
      const debugSize = debugCanvas ? canvasRenderSize(debugCanvas) : null
      const maplibreSize = canvasRenderSize(maplibreCanvas)

      targetWindowSize.maplibre.value = maplibreSize
      targetWindowSize.debug.value = debugSize

      mountRegisteredBevyInstance(state, debugSize, maplibreSize)
        .catch((error: unknown) => {
          console.error('Failed to mount Bevy instance', error)
          return disposeBevyInstance(instanceId)
        })
        .catch(console.error)
    },
  )

  watchEffect(() => {
    if (!state.isMounted.value) return

    state.bevyInstance.value
      ?.set_map_view_settings({ ...state.mapViewSettings.value })
      .catch(console.error)
  })

  watchEffect(() => {
    if (!state.isMounted.value) return

    state.bevyInstance.value
      ?.set_map_view_camera_settings({
        ...state.mapViewCameraSettings.value,
      })
      .catch(console.error)
  })

  // TODO: replace by bi-directional communication, i.e. let the worker handle the update instead of on-demand poll which
  // will swallow initial events
  const debugWindow = shallowRef<Remote<WindowInstanceRef> | null>(null)
  let debugWindowRequested = false

  const releaseDebugWindowProxy = () => {
    const debugWindowProxy = debugWindow.value
    debugWindow.value = null
    debugWindowProxy?.[releaseProxy]()
  }

  const requestDebugWindow = () => {
    if (debugWindowRequested || debugWindow.value) return

    const bevyInstance = state.bevyInstance.value
    if (!state.isMounted.value || !bevyInstance) return

    debugWindowRequested = true
    bevyInstance
      .get_debug_window()
      .then((debugWindowRemote) => {
        if (!debugWindowRemote) return

        if (bevyInstances.get(instanceId) === state && state.bevyInstance.value === bevyInstance) {
          debugWindow.value = debugWindowRemote
        } else {
          debugWindowRemote[releaseProxy]()
        }
      }, console.error)
      .finally(() => {
        debugWindowRequested = false
      })
  }

  watchEffect(() => {
    if (state.isMounted.value) {
      requestDebugWindow()
    } else {
      releaseDebugWindowProxy()
    }
  })

  useForwardCanvasEvents(state.debugCanvas, () => {
    requestDebugWindow()
    return debugWindow.value
  })

  onScopeDisposeLifo(() => {
    releaseDebugWindowProxy()
    disposeBevyInstance(instanceId).catch(console.error)
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
    bevyInstance: state.bevyInstance,
    mapViewSettings: state.mapViewSettings,
    mapViewCameraSettings: state.mapViewCameraSettings,
    tick: async (frameIdx: number) => {
      const bevyInstance = state.bevyInstance.value
      if (!state.isMounted.value || !bevyInstance) return null

      return (await Promise.all([state.refreshWindowSizes(), bevyInstance.tick(frameIdx)]))[1]
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

async function mountRegisteredBevyInstance(
  state: BevyInstanceState,
  debugSize: CanvasRenderSize | null,
  maplibreSize: CanvasRenderSize,
) {
  const debugCanvas = state.debugCanvas.value
  const maplibreCanvas = state.maplibreCanvas.value

  if (!maplibreCanvas) return

  if (debugCanvas && debugCanvas === maplibreCanvas) {
    throw new Error('Bevy debug canvas and MapLibre canvas must be different canvases')
  }

  try {
    if (debugCanvas) {
      debugCanvas.tabIndex = 0
    }

    const worker = new BevyWorker()
    state.bevyWorker.value = worker

    const debugOffscreenCanvas = debugSize
      ? new OffscreenCanvas(debugSize.width, debugSize.height)
      : null
    const textureOffscreenCanvas = new OffscreenCanvas(maplibreSize.width, maplibreSize.height)
    const terrainTextureOffscreenCanvas = new OffscreenCanvas(
      maplibreSize.width,
      maplibreSize.height,
    )

    state.bevyInstance.value = wrap<BevyInstance>(worker)
    await state.bevyInstance.value.mount(
      transfer(textureOffscreenCanvas, [textureOffscreenCanvas]),
      transfer(terrainTextureOffscreenCanvas, [terrainTextureOffscreenCanvas]),
      debugOffscreenCanvas
        ? transfer(debugOffscreenCanvas, [debugOffscreenCanvas])
        : debugOffscreenCanvas,
      bevyAssetBaseUrl(),
      { ...state.mapViewSettings.value },
      { ...state.mapViewCameraSettings.value },
      debugSize,
      maplibreSize,
    )

    if (bevyInstances.get(state.instanceId) !== state) {
      await releaseMountedBevyInstance(state)
      return
    }

    state.isMounted.value = true
  } catch (error) {
    state.isMounted.value = false
    releaseMountedBevyInstance(state).catch(console.error)
    throw error
  }
}

function bevyAssetBaseUrl() {
  return new URL(`${import.meta.env.BASE_URL}bevy-assets/`, document.baseURI).toString()
}

async function disposeBevyInstance(instanceId: string) {
  const state = bevyInstances.get(instanceId)

  if (!state) return

  bevyInstances.delete(instanceId)

  state.isMounted.value = false

  const bevyInstance = state.bevyInstance.value
  state.bevyInstance.value = null
  const bevyWorker = state.bevyWorker.value
  state.bevyWorker.value = null

  releaseAttachedCanvas(state.debugCanvas.value, instanceId)
  releaseAttachedCanvas(state.maplibreCanvas.value, instanceId)

  state.debugCanvas.value = null
  state.maplibreCanvas.value = null

  if (bevyInstance) {
    await releaseRemoteBevyInstance(bevyInstance, bevyWorker)
  } else {
    bevyWorker?.terminate()
  }
}

async function releaseMountedBevyInstance(state: BevyInstanceState) {
  const bevyInstance = state.bevyInstance.value
  const bevyWorker = state.bevyWorker.value
  state.bevyInstance.value = null
  state.bevyWorker.value = null

  if (bevyInstance) {
    await releaseRemoteBevyInstance(bevyInstance, bevyWorker)
  } else {
    bevyWorker?.terminate()
  }
}

async function releaseRemoteBevyInstance(
  bevyInstance: Remote<BevyInstance>,
  bevyWorker: Worker | null,
) {
  try {
    await bevyInstance.free()
  } finally {
    bevyInstance[releaseProxy]()
    bevyWorker?.terminate()
  }
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

function useForwardCanvasEvents(
  canvas: ShallowRef<HTMLCanvasElement | null>,
  window: MaybeRefOrGetter<Remote<WindowInstanceRef> | null>,
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

  const withWindow = (callback: (windowDefined: Remote<WindowInstanceRef>) => Promise<unknown>) => {
    const windowDefined = toValue(window)
    if (windowDefined) {
      callback(windowDefined).catch(console.error)
    }
  }

  useEventListener(canvas, 'pointerenter', () => {
    withWindow((windowDefined) => windowDefined.forward_cursor_entered())
  })

  useEventListener(canvas, 'pointerleave', () => {
    withWindow((windowDefined) => windowDefined.forward_cursor_left())
  })

  useEventListener(canvas, 'pointermove', (event) => {
    withWindow(async (windowDefined) => {
      event.preventDefault()
      const position = canvasPosition(event)
      if (!position) return

      await windowDefined.forward_cursor_moved(
        position.x,
        position.y,
        event.movementX,
        event.movementY,
      )
    })
  })

  useEventListener(canvas, 'pointerdown', (event) => {
    withWindow(async (windowDefined) => {
      const currentCanvas = canvas.value
      if (!currentCanvas) return

      event.preventDefault()
      currentCanvas.focus({ preventScroll: true })
      if (!currentCanvas.hasPointerCapture(event.pointerId)) {
        currentCanvas.setPointerCapture(event.pointerId)
      }

      await windowDefined.forward_mouse_button(event.button, true)
    })
  })

  useEventListener(canvas, 'pointerup', (event) => {
    withWindow(async (windowDefined) => {
      const currentCanvas = canvas.value
      if (!currentCanvas) return

      event.preventDefault()

      if (currentCanvas.hasPointerCapture(event.pointerId)) {
        currentCanvas.releasePointerCapture(event.pointerId)
      }

      await windowDefined.forward_mouse_button(event.button, false)
    })
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
      withWindow(async (windowDefined) => {
        event.preventDefault()
        await windowDefined.forward_mouse_wheel(event.deltaX, event.deltaY, event.deltaMode)
      })
    },
    { passive: false },
  )

  useEventListener(canvas, 'focus', () => {
    withWindow((windowDefined) => windowDefined.forward_focus(true))
  })

  useEventListener(canvas, 'blur', () => {
    withWindow((windowDefined) => windowDefined.forward_focus(false))
  })

  useEventListener(canvas, 'keydown', (event) => {
    withWindow((windowDefined) =>
      windowDefined.forward_keyboard_input(event.code, event.key, true, event.repeat),
    )
  })

  useEventListener(canvas, 'keyup', (event) => {
    withWindow((windowDefined) =>
      windowDefined.forward_keyboard_input(event.code, event.key, false, event.repeat),
    )
  })
}

function canvasRenderSize(canvas: HTMLCanvasElement): CanvasRenderSize {
  return {
    width: Math.max(canvas.clientWidth, 1),
    height: Math.max(canvas.clientHeight, 1),
    scaleFactor: devicePixelRatio,
  }
}
