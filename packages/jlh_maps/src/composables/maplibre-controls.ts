import { useMap } from '@indoorequal/vue-maplibre-gl'
import {
  GeolocateControl,
  GlobeControl,
  NavigationControl,
  type GeolocateControlOptions,
  type Map as MapLibreMap,
  type NavigationControlOptions,
  type ProjectionSpecification,
  type Subscription,
} from 'maplibre-gl'
import { computed, onUnmounted, onWatcherCleanup, ref, shallowRef, watch } from 'vue'

type HiddenNativeControl = {
  onAdd(map: MapLibreMap): HTMLElement
  onRemove(): void
}

function getMapProjection(map?: MapLibreMap) {
  try {
    return map?.getProjection()
  } catch {
    return undefined
  }
}

type Point = {
  x: number
  y: number
}

function getAngleDelta(lastPoint: Point, currentPoint: Point, center: Point) {
  const point = {
    x: currentPoint.x - center.x,
    y: currentPoint.y - center.y,
  }
  const lastPointVector = {
    x: lastPoint.x - center.x,
    y: lastPoint.y - center.y,
  }
  const crossProduct = point.x * lastPointVector.y - point.y * lastPointVector.x
  const dotProduct = point.x * lastPointVector.x + point.y * lastPointVector.y

  return (Math.atan2(crossProduct, dotProduct) * 180) / Math.PI
}

function getElementPoint(element: HTMLElement, event: PointerEvent) {
  const rect = element.getBoundingClientRect()

  return {
    x: event.clientX - rect.left,
    y: event.clientY - rect.top,
  }
}

function getElementCenter(element: HTMLElement) {
  const rect = element.getBoundingClientRect()

  return {
    x: rect.width / 2,
    y: rect.height / 2,
  }
}

function useHiddenNativeControl<TControl extends HiddenNativeControl>(
  key: symbol | string | undefined,
  createControl: () => TControl,
  setup?: (control: TControl, container: HTMLElement, map: MapLibreMap) => (() => void) | void,
) {
  const mapInstance = useMap(key)
  const control = shallowRef<TControl>()
  const container = shallowRef<HTMLElement>()

  watch(
    () => mapInstance.map,
    (map) => {
      if (!map) return

      let disposed = false
      let loadSubscription: Subscription | undefined
      let setupCleanup: (() => void) | void

      const removeControl = () => {
        setupCleanup?.()
        setupCleanup = undefined

        control.value?.onRemove()
        control.value = undefined
        container.value = undefined
      }

      const mountControl = () => {
        if (disposed || control.value) return

        const nextControl = createControl()
        const nextContainer = nextControl.onAdd(map)

        nextContainer.hidden = true
        control.value = nextControl
        container.value = nextContainer
        setupCleanup = setup?.(nextControl, nextContainer, map)
      }

      if (map.loaded()) {
        mountControl()
      } else {
        loadSubscription = map.on('load', mountControl)
      }

      onWatcherCleanup(() => {
        disposed = true
        loadSubscription?.unsubscribe()
        removeControl()
      })
    },
    { immediate: true },
  )

  return {
    control,
    container,
  }
}

export function useProjectionControl(key?: symbol | string) {
  const mapInstance = useMap(key)
  const projection = shallowRef<ProjectionSpecification>()
  let updatingFromMap = false

  const updateProjection = () => {
    const map = mapInstance.map
    if (!map) return

    const nextProjection = getMapProjection(map)
    if (!nextProjection) return
    if (projection.value?.type === nextProjection.type) return

    updatingFromMap = true
    projection.value = nextProjection
    updatingFromMap = false
  }

  const syncProjection = () => {
    const map = mapInstance.map
    if (!map) return

    const currentProjection = getMapProjection(map)
    if (!currentProjection) return

    if (projection.value && currentProjection.type !== projection.value.type) {
      map.setProjection(projection.value)
      return
    }

    updateProjection()
  }

  watch(
    projection,
    (value) => {
      if (updatingFromMap || !value) return

      const map = mapInstance.map
      const currentProjection = getMapProjection(map)
      if (!map || !currentProjection || currentProjection.type === value.type) return

      map.setProjection(value)
    },
    { flush: 'sync' },
  )

  watch(
    () => mapInstance.map,
    (map) => {
      if (!map) return

      syncProjection()

      const subscriptions = [
        map.on('styledata', syncProjection),
        map.on('projectiontransition', syncProjection),
      ]

      onWatcherCleanup(() => {
        subscriptions.forEach((sub) => sub.unsubscribe())
      })
    },
    { immediate: true },
  )

  return {
    projection,
  }
}

export function useGlobeControl(key?: symbol | string) {
  const projection = shallowRef<ProjectionSpecification>()
  const controlButton = shallowRef<HTMLButtonElement>()
  let syncProjection = () => {}

  const { control } = useHiddenNativeControl(
    key,
    () => new GlobeControl(),
    (_control, container, map) => {
      const updateProjection = () => {
        const nextProjection = getMapProjection(map)
        if (nextProjection) {
          projection.value = nextProjection
        }
      }

      controlButton.value = container.querySelector<HTMLButtonElement>('button') ?? undefined
      syncProjection = updateProjection
      updateProjection()

      const subscriptions = [
        map.on('styledata', updateProjection),
        map.on('projectiontransition', updateProjection),
      ]

      return () => {
        subscriptions.forEach((subscription) => subscription.unsubscribe())
        if (syncProjection === updateProjection) {
          syncProjection = () => {}
        }
        controlButton.value = undefined
        projection.value = undefined
      }
    },
  )

  const active = computed(() => projection.value?.type === 'globe')
  const disabled = computed(() => !control.value || !controlButton.value)
  const title = computed(() => (active.value ? 'Disable globe' : 'Enable globe'))
  const ariaPressed = computed(() => active.value)

  const trigger = () => {
    if (disabled.value) return

    controlButton.value?.click()
    syncProjection()
  }

  return {
    active,
    disabled,
    title,
    ariaPressed,
    trigger,
  }
}

type NavigationControlComposableOptions = NavigationControlOptions & {
  northRotationOffset?: number
}

export function useNavigationControl(
  key?: symbol | string,
  options: NavigationControlComposableOptions = {},
) {
  const { northRotationOffset = 135, ...navigationOptions } = options
  const mapInstance = useMap(key)
  const zoomInButton = shallowRef<HTMLButtonElement>()
  const zoomOutButton = shallowRef<HTMLButtonElement>()
  const compassButton = shallowRef<HTMLButtonElement>()
  const compassIcon = shallowRef<HTMLElement>()

  const zoomInDisabled = ref(true)
  const zoomOutDisabled = ref(true)
  const compassIconTransform = ref('')
  const zoomInTitle = ref('Zoom in')
  const zoomOutTitle = ref('Zoom out')
  const compassTitle = ref('Reset bearing')
  let compassDragState:
    | {
        element: HTMLElement
        lastPoint: Point
        moved: boolean
        pointerId: number
        startPoint: Point
      }
    | undefined
  let ignoreNextCompassClick = false

  const updateButtonState = () => {
    zoomInDisabled.value = zoomInButton.value?.disabled ?? true
    zoomOutDisabled.value = zoomOutButton.value?.disabled ?? true
    compassIconTransform.value = compassIcon.value?.style.transform || ''
    zoomInTitle.value = zoomInButton.value?.title || 'Zoom in'
    zoomOutTitle.value = zoomOutButton.value?.title || 'Zoom out'
    compassTitle.value = compassButton.value?.title || 'Reset bearing'
  }

  const { control } = useHiddenNativeControl(
    key,
    () => new NavigationControl(navigationOptions),
    (_control, container, map) => {
      const updateState = () => updateButtonState()

      zoomInButton.value =
        container.querySelector<HTMLButtonElement>('.maplibregl-ctrl-zoom-in') ?? undefined
      zoomOutButton.value =
        container.querySelector<HTMLButtonElement>('.maplibregl-ctrl-zoom-out') ?? undefined
      compassButton.value =
        container.querySelector<HTMLButtonElement>('.maplibregl-ctrl-compass') ?? undefined
      compassIcon.value =
        container.querySelector<HTMLElement>('.maplibregl-ctrl-compass .maplibregl-ctrl-icon') ??
        undefined

      updateState()

      const subscriptions = [map.on('zoom', updateState), map.on('rotate', updateState)]

      if (navigationOptions.visualizePitch) {
        subscriptions.push(map.on('pitch', updateState))
      }

      if (navigationOptions.visualizeRoll !== false) {
        subscriptions.push(map.on('roll', updateState))
      }

      return () => {
        stopCompassDrag()
        subscriptions.forEach((subscription) => subscription.unsubscribe())
        zoomInButton.value = undefined
        zoomOutButton.value = undefined
        compassButton.value = undefined
        compassIcon.value = undefined
        zoomInDisabled.value = true
        zoomOutDisabled.value = true
        compassIconTransform.value = ''
      }
    },
  )

  const moveCompassDrag = (event: PointerEvent) => {
    const map = mapInstance.map
    if (!map || !compassDragState || event.pointerId !== compassDragState.pointerId) return

    event.preventDefault()

    const point = getElementPoint(compassDragState.element, event)
    const center = getElementCenter(compassDragState.element)
    const distanceMoved = Math.hypot(
      point.x - compassDragState.startPoint.x,
      point.y - compassDragState.startPoint.y,
    )

    compassDragState.moved ||= distanceMoved > 3

    const bearingDelta = getAngleDelta(
      { x: compassDragState.lastPoint.x, y: point.y },
      point,
      center,
    )

    if (Number.isFinite(bearingDelta)) {
      map.setBearing(map.getBearing() + bearingDelta)
    }

    if (navigationOptions.visualizePitch) {
      const pitchDelta = (point.y - compassDragState.lastPoint.y) * -0.5

      if (Number.isFinite(pitchDelta)) {
        map.setPitch(map.getPitch() + pitchDelta)
      }
    }

    compassDragState.lastPoint = point
  }

  const stopCompassDrag = (event?: PointerEvent) => {
    if (event && compassDragState && event.pointerId !== compassDragState.pointerId) return

    if (compassDragState?.moved) {
      ignoreNextCompassClick = true
      window.setTimeout(() => {
        ignoreNextCompassClick = false
      })
    }

    compassDragState?.element.releasePointerCapture?.(compassDragState.pointerId)
    compassDragState = undefined

    window.removeEventListener('pointermove', moveCompassDrag)
    window.removeEventListener('pointerup', stopCompassDrag)
    window.removeEventListener('pointercancel', stopCompassDrag)
  }

  const startCompassDrag = (event: PointerEvent) => {
    if (!mapInstance.map || !control.value || !compassButton.value) return
    if (event.pointerType === 'mouse' && event.button !== 0) return

    const element = event.currentTarget
    if (!(element instanceof HTMLElement)) return

    event.preventDefault()
    stopCompassDrag()

    const point = getElementPoint(element, event)
    compassDragState = {
      element,
      lastPoint: point,
      moved: false,
      pointerId: event.pointerId,
      startPoint: point,
    }

    element.setPointerCapture?.(event.pointerId)
    window.addEventListener('pointermove', moveCompassDrag)
    window.addEventListener('pointerup', stopCompassDrag)
    window.addEventListener('pointercancel', stopCompassDrag)
  }

  const zoomIn = () => {
    if (!control.value || zoomInDisabled.value) return

    zoomInButton.value?.click()
  }

  const zoomOut = () => {
    if (!control.value || zoomOutDisabled.value) return

    zoomOutButton.value?.click()
  }

  const resetBearing = () => {
    if (ignoreNextCompassClick) {
      ignoreNextCompassClick = false
      return
    }

    if (!control.value || !compassButton.value) return

    compassButton.value.click()
  }

  onUnmounted(stopCompassDrag)

  return {
    zoomInDisabled,
    zoomInTitle,
    zoomIn,
    zoomOutDisabled,
    zoomOutTitle,
    zoomOut,
    compassDisabled: computed(() => !control.value || !compassButton.value),
    compassTitle,
    compassIconStyle: computed(() => ({
      '--compass-icon-transform': compassIconTransform.value
        ? `${compassIconTransform.value} rotate(${northRotationOffset}deg)`
        : `rotate(${northRotationOffset}deg)`,
      touchAction: 'none',
    })),
    resetBearing,
    startCompassDrag,
  }
}

async function checkGeolocationSupport() {
  if (!window.navigator.geolocation) return false
  if (!window.navigator.permissions) return true

  try {
    const permission = await window.navigator.permissions.query({ name: 'geolocation' })
    return permission.state !== 'denied'
  } catch {
    return true
  }
}

type GeolocateControlUiState = 'off' | 'active' | 'background' | 'error'

export function useGeolocateControl(key?: symbol | string, options: GeolocateControlOptions = {}) {
  const supported = ref(false)
  const loading = ref(false)
  const state = ref<GeolocateControlUiState>('off')
  let loadingTimeout: ReturnType<typeof setTimeout> | undefined
  let turningOff = false

  const stopLoading = () => {
    if (loadingTimeout) {
      clearTimeout(loadingTimeout)
      loadingTimeout = undefined
    }

    loading.value = false
  }

  const startLoading = () => {
    stopLoading()
    loading.value = true
    loadingTimeout = setTimeout(stopLoading, 10000)
  }

  const activate = () => {
    state.value = 'active'
  }

  const deactivate = () => {
    state.value = 'off'
    stopLoading()
  }

  const enterBackground = () => {
    state.value = 'background'
    stopLoading()
  }

  const { control } = useHiddenNativeControl(
    key,
    () => new GeolocateControl(options),
    (geolocate) => {
      let disposed = false

      const finish = () => {
        stopLoading()
      }

      const onTrackUserLocationEnd = () => {
        if (turningOff) {
          turningOff = false
          deactivate()
          return
        }

        enterBackground()
      }

      const onGeolocate = () => {
        if (options.trackUserLocation && state.value !== 'background') {
          state.value = 'active'
        }

        finish()
      }

      const onError = (event: unknown) => {
        if ((event as GeolocationPositionError | undefined)?.code === 1) {
          supported.value = false
        }

        state.value = 'error'
        stopLoading()
      }

      const subscriptions = [
        geolocate.on('geolocate', onGeolocate),
        geolocate.on('outofmaxbounds', () => {
          state.value = 'error'
          finish()
        }),
        geolocate.on('error', onError),
        geolocate.on('trackuserlocationstart', activate),
        geolocate.on('trackuserlocationend', onTrackUserLocationEnd),
        geolocate.on('userlocationfocus', activate),
        geolocate.on('userlocationlostfocus', enterBackground),
      ]

      checkGeolocationSupport().then((value) => {
        if (!disposed) {
          supported.value = value
        }
      })

      return () => {
        disposed = true
        turningOff = false
        subscriptions.forEach((subscription) => subscription.unsubscribe())
        supported.value = false
        state.value = 'off'
        stopLoading()
      }
    },
  )

  const disabled = computed(() => !control.value || !supported.value)
  const active = computed(() => state.value === 'active' || state.value === 'background')
  const icon = computed(() => (state.value === 'active' ? 'lucide:locate-fixed' : 'lucide:locate'))
  const title = computed(() =>
    disabled.value
      ? 'Location not available'
      : state.value === 'active'
        ? 'Stop locating'
        : state.value === 'background'
          ? 'Refocus location'
          : 'Find my location',
  )
  const ariaPressed = computed(() =>
    options.trackUserLocation ? String(active.value || loading.value) : undefined,
  )

  const trigger = () => {
    if (disabled.value || !control.value) return

    if (options.trackUserLocation) {
      if (state.value === 'active' || loading.value) {
        turningOff = true
      } else if (state.value !== 'background') {
        startLoading()
      }
    } else {
      startLoading()
    }

    if (!control.value.trigger()) {
      turningOff = false
      stopLoading()
    }

    if (turningOff) {
      turningOff = false
      deactivate()
    }
  }

  onUnmounted(stopLoading)

  return {
    disabled,
    loading,
    active,
    icon,
    state,
    title,
    ariaPressed,
    trigger,
  }
}
