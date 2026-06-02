import { expose, proxy, transfer } from 'comlink'
import initApp, {
  BevyInstance as BevyBevyInstance,
  type MaplibreIntegration as BevyMaplibreIntegration,
  type MapViewCameraSettings,
  type MapViewSettings,
  type WindowInstanceRef as BevyWindowInstanceRef,
} from 'jlh_maps_app'
import { TickGate, type TickGateHolder } from '@/bevy/helper.ts'

const TICK_GATE_TIMEOUT_MS = 50

let initAppPromise: Promise<unknown> | null = null

function ensureInitialized() {
  initAppPromise ??= initApp()
  return initAppPromise
}

export type CanvasRenderSize = {
  width: number
  height: number
  scaleFactor: number
}

class WorkerMaplibreIntegration {
  constructor(
    private readonly integration: BevyMaplibreIntegration,
    private readonly tickGateHolder: TickGateHolder,
  ) {}

  free() {
    try {
      this.tickGateHolder.free()
    } finally {
      this.integration.free()
    }
  }

  remove_source_tile(sourceId: string, z: number, x: number, y: number) {
    this.integration.remove_source_tile(sourceId, z, x, y)
  }

  remove_terrain_tile_data(tileKey: string) {
    this.integration.remove_terrain_tile_data(tileKey)
  }

  sync_terrain_active_tile_ids(activeTileIds: string[]) {
    this.integration.sync_terrain_active_tile_ids(activeTileIds)
  }

  sync_view(
    frameId: number,
    width: number,
    height: number,
    zoom: number,
    pitch: number,
    bearing: number,
    centerLng: number,
    centerLat: number,
    mainMatrix: Float64Array,
  ) {
    try {
      this.integration.sync_view(
        width,
        height,
        zoom,
        pitch,
        bearing,
        centerLng,
        centerLat,
        mainMatrix,
      )
    } finally {
      this.tickGateHolder.release(frameId)
    }
  }

  update_source_tile(sourceId: string, z: number, x: number, y: number, data: Uint8Array) {
    this.integration.update_source_tile(sourceId, z, x, y, data)
  }

  update_terrain_tile_data(
    tileKey: string,
    hash: string,
    stride: number,
    dim: number,
    min: number,
    max: number,
    redFactor: number,
    greenFactor: number,
    blueFactor: number,
    baseShift: number,
    terrainMatrixJson: string,
    data: Uint32Array,
  ) {
    this.integration.update_terrain_tile_data(
      tileKey,
      hash,
      stride,
      dim,
      min,
      max,
      redFactor,
      greenFactor,
      blueFactor,
      baseShift,
      terrainMatrixJson,
      data,
    )
  }
}

export type MaplibreIntegration = InstanceType<typeof WorkerMaplibreIntegration>

class WorkerBevyInstance {
  private debugCanvas: OffscreenCanvas | null = null
  private textureCanvas: OffscreenCanvas | null = null
  private bevyInstance: BevyBevyInstance | null = null
  private debugWindow: BevyWindowInstanceRef | null = null
  private textureWindow: BevyWindowInstanceRef | null = null

  private mapViewSettings: MapViewSettings | null = null
  private secondaryTickScheduled = false

  private readonly tickGate = new TickGate()

  async mount(
    textureCanvas: OffscreenCanvas,
    debugCanvas: OffscreenCanvas,
    mapViewSettings: MapViewSettings,
    mapViewCameraSettings: MapViewCameraSettings,
    debugSize: CanvasRenderSize,
    textureSize: CanvasRenderSize,
  ) {
    await ensureInitialized()

    this.debugCanvas = debugCanvas
    this.textureCanvas = textureCanvas

    this.resizeCanvases(debugSize, textureSize)
    this.bevyInstance = new BevyBevyInstance(debugCanvas, textureCanvas)
    this.set_map_view_settings(mapViewSettings)
    this.set_map_view_camera_settings(mapViewCameraSettings)
    this.refreshWindowRefs()
    this.resize(debugSize, textureSize)
  }

  free() {
    this.debugWindow?.free()
    this.textureWindow?.free()
    this.debugWindow = null
    this.textureWindow = null
    this.tickGate.free()
    this.secondaryTickScheduled = false

    this.bevyInstance?.free()
    this.bevyInstance = null

    this.textureCanvas = null
    this.debugCanvas = null
  }

  resize(debugSize: CanvasRenderSize, textureSize: CanvasRenderSize): boolean {
    this.refreshWindowRefs()

    if (!this.debugWindow || !this.textureWindow) return false

    this.resizeCanvases(debugSize, textureSize)
    this.debugWindow.resize(debugSize.width, debugSize.height, debugSize.scaleFactor)
    this.textureWindow.resize(textureSize.width, textureSize.height, textureSize.scaleFactor)

    return true
  }

  async tick(frameIdx: number) {
    const bevyInstance = this.bevyInstance
    const textureCanvas = this.textureCanvas
    const debugCanvas = this.debugCanvas
    if (!bevyInstance || !textureCanvas || !debugCanvas) return null

    // since instance and integration messages are handled by separate event listeners,
    // even if the messages are sent in order from the main thread, they may be processed out-of-order
    // thus requiring the tick gate mechanism
    const tickGateResult = await this.tickGate.untilTickReleased(frameIdx, TICK_GATE_TIMEOUT_MS)
    if (!tickGateResult.released) {
      console.warn(
        `Bevy tick gate timed out after ${TICK_GATE_TIMEOUT_MS}ms waiting for holder(s): ${tickGateResult.pendingHolderIds.join(', ')}`,
      )
    }

    bevyInstance.tick()
    this.refreshWindowRefs()

    try {
      const [textureBitmap, debugBitmap] = await Promise.all([
        createImageBitmap(textureCanvas),
        this.mapViewSettings?.enableWindowCameras ? createImageBitmap(debugCanvas) : null,
      ])

      return transfer(
        {
          textureBitmap,
          debugBitmap,
        },
        [textureBitmap, ...(debugBitmap ? [debugBitmap] : [])],
      )
    } catch (error) {
      console.warn('Failed to create ImageBitmap:', error)
      return null
    } finally {
      this.scheduleSecondaryTick(bevyInstance)
    }
  }

  create_map_integration() {
    const bevyInstance = this.bevyInstance
    if (!bevyInstance) {
      throw new Error('Cannot create MapLibre integration before Bevy is mounted')
    }

    const tickGateHolder = this.tickGate.registerHolder()

    try {
      return proxy(
        new WorkerMaplibreIntegration(bevyInstance.create_map_integration(), tickGateHolder),
      )
    } catch (error) {
      tickGateHolder.free()
      throw error
    }
  }

  get_debug_window() {
    if (this.debugWindow) return proxy(this.debugWindow)
    return null
  }

  set_map_view_camera_settings(settings: MapViewCameraSettings) {
    this.bevyInstance?.set_map_view_camera_settings(settings)
  }

  set_map_view_settings(settings: MapViewSettings) {
    if (this.bevyInstance) {
      this.bevyInstance.set_map_view_settings(settings)
      this.mapViewSettings = settings
    }
  }

  private resizeCanvases(debugSize: CanvasRenderSize, textureSize: CanvasRenderSize) {
    if (this.debugCanvas) {
      this.debugCanvas.width = debugSize.width
      this.debugCanvas.height = debugSize.height
    }

    if (this.textureCanvas) {
      this.textureCanvas.width = textureSize.width
      this.textureCanvas.height = textureSize.height
    }
  }

  private refreshWindowRefs() {
    const bevyInstance = this.bevyInstance
    if (!bevyInstance) return

    this.debugWindow ??= bevyInstance.get_debug_window() ?? null
    this.textureWindow ??= bevyInstance.get_texture_window() ?? null
  }

  private scheduleSecondaryTick(bevyInstance: BevyBevyInstance) {
    if (this.secondaryTickScheduled) return
    this.secondaryTickScheduled = true

    setTimeout(() => {
      this.secondaryTickScheduled = false
      if (this.bevyInstance !== bevyInstance) return

      try {
        bevyInstance.tick_secondary()
      } catch (error) {
        console.warn('Failed to execute secondary Bevy tick:', error)
      }
    }, 0)
  }
}

export type BevyInstance = InstanceType<typeof WorkerBevyInstance>

expose(new WorkerBevyInstance())
