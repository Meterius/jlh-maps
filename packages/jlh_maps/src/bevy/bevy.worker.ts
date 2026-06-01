import { expose, proxy, transfer } from 'comlink'
import initApp, {
  BevyInstance as BevyBevyInstance,
  type MaplibreIntegration as BevyMaplibreIntegration,
  type MapViewCameraSettings,
  type MapViewSettings,
  type WindowInstanceRef as BevyWindowInstanceRef,
} from 'jlh_maps_app'

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
  constructor(private readonly integration: BevyMaplibreIntegration) {}

  free() {
    this.integration.free()
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
    width: number,
    height: number,
    zoom: number,
    pitch: number,
    bearing: number,
    centerLng: number,
    centerLat: number,
    mainMatrix: Float64Array,
  ) {
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

  async tick() {
    const bevyInstance = this.bevyInstance
    const textureCanvas = this.textureCanvas
    const debugCanvas = this.debugCanvas
    if (!bevyInstance || !textureCanvas || !debugCanvas) return null

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
    }
  }

  create_map_integration() {
    const bevyInstance = this.bevyInstance
    if (!bevyInstance) {
      throw new Error('Cannot create MapLibre integration before Bevy is mounted')
    }

    return proxy(new WorkerMaplibreIntegration(bevyInstance.create_map_integration()))
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
}

export type BevyInstance = InstanceType<typeof WorkerBevyInstance>

expose(new WorkerBevyInstance())
