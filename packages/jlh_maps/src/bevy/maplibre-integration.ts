import { releaseProxy, transfer, type Remote } from 'comlink'
import { type Map as MapLibreMap, type MapSourceDataEvent, type Tile } from 'maplibre-gl'
import { shallowRef } from 'vue'
import { onWatcherCleanupLifo, watchDefinedOnce } from '@/composables/helper.ts'
import { useMap } from '@indoorequal/vue-maplibre-gl'
import type {
  CanonicalTileID,
  DEMData,
  Map as InternalMap,
  Terrain,
  Tile as InternalTile,
} from 'maplibre-gl/src/index.ts'
// @ts-expect-error Class not properly exported by dist
import { OverscaledTileID } from 'maplibre-gl/src/tile/tile_id'
import { useBevy } from '@/bevy/index.ts'
import type { MaplibreIntegration } from '@/bevy/bevy.worker.ts'

type TileKey = string
type SourceTileKey = string

interface TileCoord {
  z: number
  x: number
  y: number
}

interface SyncedSourceTileState {
  sourceId: string
  tileKey: TileCoord
}

interface MaplibreGlJsIntegrationOptions {
  featureSourceIds?: string[]
}

type RawTileData = ArrayBuffer | ArrayBufferView

export function useMaplibreIntegration(
  instanceId: string,
  mapKey?: string | symbol,
  options: MaplibreGlJsIntegrationOptions = {},
) {
  const bevyInstance = useBevy(instanceId)
  const mapInstance = useMap(mapKey)

  const mapIntegration = shallowRef<MaplibreGlJsIntegration | null>(null)

  watchDefinedOnce(
    () => (bevyInstance.isMounted.value ? mapInstance.map : undefined),
    (map) => {
      const mountedBevyInstance = bevyInstance.bevyInstance.value
      if (!mountedBevyInstance) return

      let stopped = false
      let integration: MaplibreGlJsIntegration | null = null

      onWatcherCleanupLifo(() => {
        stopped = true
        integration?.stop()
        if (mapIntegration.value === integration) {
          mapIntegration.value = null
        }
      })

      void mountedBevyInstance
        .create_map_integration()
        .then((bevyMapIntegration) => {
          const remoteIntegration = bevyMapIntegration as Remote<MaplibreIntegration>
          if (stopped) {
            void remoteIntegration.free().finally(() => remoteIntegration[releaseProxy]())
            return
          }

          integration = new MaplibreGlJsIntegration(
            map,
            remoteIntegration,
            options.featureSourceIds ?? [],
          )
          integration.start()

          mapIntegration.value = integration
        })
        .catch((error: unknown) => {
          if (!stopped) {
            console.error('Failed to create Bevy MapLibre integration', error)
          }
        })
    },
  )

  return {
    syncOnRender: (frameIdx: number) =>
      mapIntegration.value?.syncOnRender(frameIdx) ?? Promise.resolve(),
  }
}

class MaplibreGlJsIntegration {
  private readonly terrainDataHashes = new Map<TileKey, string>()
  private readonly transmittedSourceTiles = new Map<SourceTileKey, SyncedSourceTileState>()
  private readonly pendingChangedSourceTiles = new Map<
    SourceTileKey,
    { sourceId: string; tileId: OverscaledTileID; tile: Tile }
  >()
  private readonly activeTerrainTileKeys = new Set<TileKey>()
  private readonly sourceTerrainTileKeys = new Set<TileKey>()
  private syncViewFrame: number | undefined
  private syncDataFrame: number | undefined

  private unsubscribeCallbacks: (() => void)[] = []

  private stopped: boolean = false

  constructor(
    private readonly map: MapLibreMap,
    private readonly bevyMapIntegration: Remote<MaplibreIntegration>,
    private readonly featureSourceIds: string[],
  ) {}

  start() {
    console.log('Starting maplibre integration on map: ', this.map)

    this.unsubscribeCallbacks.push(
      this.map.on('sourcedata', (event) => this.handleSourceData(event)).unsubscribe,
    )
  }

  async syncOnRender(frameIdx: number) {
    if (this.stopped) return

    await Promise.all([this.syncView(frameIdx), this.syncTerrain(), this.syncVisibleSourceTiles()])
  }

  stop() {
    if (this.stopped) return
    this.stopped = true

    try {
      if (this.syncViewFrame !== undefined) {
        cancelAnimationFrame(this.syncViewFrame)
        this.syncViewFrame = undefined
      }
      if (this.syncDataFrame !== undefined) {
        cancelAnimationFrame(this.syncDataFrame)
        this.syncDataFrame = undefined
      }

      this.unsubscribeCallbacks.splice(0).forEach((unsubscribe) => unsubscribe())
      this.terrainDataHashes.clear()
      this.transmittedSourceTiles.clear()
      this.pendingChangedSourceTiles.clear()
      this.activeTerrainTileKeys.clear()
      this.sourceTerrainTileKeys.clear()
    } finally {
      void this.bevyMapIntegration.free().finally(() => this.bevyMapIntegration[releaseProxy]())
    }
  }

  private scheduleSyncData() {
    if (this.syncDataFrame !== undefined) return

    this.syncDataFrame = requestAnimationFrame(() => {
      this.syncDataFrame = undefined
      void this.syncData()
    })
  }

  private async syncView(frameId: number) {
    const center = this.map.getCenter()
    const canvas = this.map.getCanvas()
    const mainMatrix = this.getMainMatrix()
    if (!mainMatrix) return

    await this.bevyMapIntegration.sync_view(
      frameId,
      canvas.width,
      canvas.height,
      this.map.getZoom(),
      this.map.getPitch(),
      this.map.getBearing(),
      center.lng,
      center.lat,
      mainMatrix,
    )
  }

  private async syncData() {
    await this.syncSourceTiles()
  }

  private getMainMatrix(): Float64Array | undefined {
    const transform = (this.map as unknown as InternalMap).transform

    const matrix =
      transform.getProjectionDataForCustomLayer?.().mainMatrix ??
      transform.projectionData?.mainMatrix ??
      transform.modelViewProjectionMatrix

    if (!matrix) return undefined
    return matrix instanceof Float64Array ? matrix : new Float64Array(matrix)
  }

  private async syncSourceTiles() {
    if (this.featureSourceIds.length === 0) {
      await this.removeTransmittedSourceTiles([...this.transmittedSourceTiles.keys()])
      this.pendingChangedSourceTiles.clear()
      this.sourceTerrainTileKeys.clear()
      await this.pruneTerrainData()
      return
    }

    const changedTiles = [...this.pendingChangedSourceTiles.values()]
    this.pendingChangedSourceTiles.clear()

    for (const { sourceId, tileId, tile } of changedTiles) {
      await this.syncSourceTile(sourceId, tileId, tile)
    }

    this.refreshSourceTerrainTileKeys()
    await this.pruneTerrainData()
  }

  private handleSourceData(event: MapSourceDataEvent) {
    if (event.dataType !== 'source') return

    const sourceId = event.sourceId
    if (!sourceId || !this.featureSourceIds.includes(sourceId)) {
      return
    }

    const eventTile = event.tile as Tile | undefined
    if (eventTile?.tileID) {
      this.pendingChangedSourceTiles.set(
        this.getSourceTileKey(sourceId, eventTile.tileID.canonical),
        {
          sourceId,
          tileId: eventTile.tileID,
          tile: eventTile,
        },
      )
      this.scheduleSyncData()
    }
  }

  private async syncVisibleSourceTiles() {
    const visibleTiles = this.getVisibleSourceTiles()
    const visibleTileKeys = new Set(visibleTiles.map((tile) => tile.sourceTileKey))
    const removedTileKeys = [...this.transmittedSourceTiles.keys()].filter(
      (tileKey) => !visibleTileKeys.has(tileKey),
    )
    await this.removeTransmittedSourceTiles(removedTileKeys)

    for (const { sourceId, tileId, tile, sourceTileKey } of visibleTiles) {
      if (this.transmittedSourceTiles.has(sourceTileKey)) continue
      await this.syncSourceTile(sourceId, tileId, tile)
    }

    this.refreshSourceTerrainTileKeys()
    await this.pruneTerrainData()
  }

  private async syncSourceTile(sourceId: string, tileId: OverscaledTileID, tile: Tile) {
    const tileCoord = this.getTileCoord(tileId.canonical)
    const sourceTileKey = this.getSourceTileKey(sourceId, tileId.canonical)
    const rawTileData = this.getTileRawData(tile)

    if (!rawTileData) {
      await this.removeTransmittedSourceTiles([sourceTileKey])
      return
    }

    await this.bevyMapIntegration.update_source_tile(
      sourceId,
      tileCoord.z,
      tileCoord.x,
      tileCoord.y,
      transfer(rawTileData, [rawTileData.buffer]),
    )
    await this.syncTerrainDataForTileCoords([tileCoord])
    this.transmittedSourceTiles.set(sourceTileKey, {
      sourceId,
      tileKey: tileCoord,
    })
  }

  private getTileRawData(tile: Tile): Uint8Array | undefined {
    const rawTileData = (tile as Tile & { latestRawTileData?: RawTileData }).latestRawTileData
    if (!rawTileData) return undefined

    if (ArrayBuffer.isView(rawTileData)) {
      return new Uint8Array(
        rawTileData.buffer.slice(
          rawTileData.byteOffset,
          rawTileData.byteOffset + rawTileData.byteLength,
        ),
      )
    }

    return new Uint8Array(rawTileData.slice(0))
  }

  private get terrain(): Terrain | null {
    return (this.map as unknown as InternalMap).terrain ?? null
  }

  private async syncTerrain() {
    const terrain = this.terrain ?? undefined

    if (terrain) {
      const activeTerrainTiles = terrain.tileManager.getRenderableTiles() ?? []

      const activeTerrainTileIds = new Set(
        activeTerrainTiles.map((tile) => this.getTileKey(tile.tileID.canonical)),
      )
      this.activeTerrainTileKeys.clear()
      for (const key of activeTerrainTileIds) {
        this.activeTerrainTileKeys.add(key)
      }

      await this.pruneTerrainData()

      await this.bevyMapIntegration.sync_terrain_active_tile_ids([...activeTerrainTileIds])

      for (const tile of activeTerrainTiles) {
        const key = this.getTileKey(tile.tileID.canonical)
        await this.syncTerrainDataForTileId(key, tile.tileID, tile)
      }
    } else {
      this.activeTerrainTileKeys.clear()
      // terrain is not available, remove all terrain data that may have existed while terrain was active
      if (this.terrainDataHashes.size !== 0) {
        await this.removeTerrainData([...this.terrainDataHashes.keys()])
        this.terrainDataHashes.clear()
      }

      const activeTerrainTileIds = new Set(
        this.map
          .coveringTiles({
            tileSize: 512,
          })
          .map((tileId) => this.getTileKey(tileId.canonical)),
      )

      await this.bevyMapIntegration.sync_terrain_active_tile_ids([...activeTerrainTileIds])
    }
  }

  private async syncTerrainDataForTileCoords(tileCoords: Iterable<TileCoord>) {
    if (!this.terrain) return

    for (const tileCoord of tileCoords) {
      // @ts-expect-error No clue of what is going on here
      const tileId = new OverscaledTileID(tileCoord.z, 0, tileCoord.z, tileCoord.x, tileCoord.y)
      await this.syncTerrainDataForTileId(this.getTileCoordKey(tileCoord), tileId)
    }
  }

  private async syncTerrainDataForTileId(
    tileKey: TileKey,
    tileId: OverscaledTileID,
    renderTile?: InternalTile,
  ) {
    const terrain = this.terrain
    if (!terrain) return

    const terrainData = terrain.getTerrainData(tileId)
    const dem = terrainData.tile?.dem
    if (!dem) return

    const hash = this.getTerrainDataHash(tileKey, renderTile ?? terrainData.tile, dem)
    if (this.terrainDataHashes.get(tileKey) === hash) return

    const terrainDataBuffer = new Uint32Array(dem.data)
    await this.bevyMapIntegration.update_terrain_tile_data(
      tileKey,
      hash,
      dem.stride,
      dem.dim,
      dem.min,
      dem.max,
      dem.redFactor,
      dem.greenFactor,
      dem.blueFactor,
      dem.baseShift,
      JSON.stringify(Array.from(terrainData.u_terrain_matrix)),
      transfer(terrainDataBuffer, [terrainDataBuffer.buffer]),
    )
    this.terrainDataHashes.set(tileKey, hash)
  }

  private async removeTransmittedSourceTiles(tileKeys: SourceTileKey[]) {
    for (const tileKey of tileKeys) {
      const tile = this.transmittedSourceTiles.get(tileKey)
      if (tile) {
        await this.bevyMapIntegration.remove_source_tile(
          tile.sourceId,
          tile.tileKey.z,
          tile.tileKey.x,
          tile.tileKey.y,
        )
      }
      this.transmittedSourceTiles.delete(tileKey)
    }
  }

  private async removeTerrainData(tileKeys: string[]) {
    for (const tileKey of tileKeys) {
      await this.bevyMapIntegration.remove_terrain_tile_data(tileKey)
      this.terrainDataHashes.delete(tileKey)
    }
  }

  private async pruneTerrainData() {
    await this.removeTerrainData(
      [...this.terrainDataHashes.keys()].filter(
        (key) => !this.activeTerrainTileKeys.has(key) && !this.sourceTerrainTileKeys.has(key),
      ),
    )
  }

  private getTerrainDataHash(
    tileKey: TileKey,
    sourceTile: InternalTile | null | undefined,
    dem: DEMData,
  ) {
    const sourceTileID = sourceTile?.tileID?.key ?? sourceTile?.tileID?.toString?.() ?? 'none'
    const rttStamp = sourceTile ? (this.getRttContentStamp(sourceTile) ?? 'none') : 'none'

    return [
      tileKey,
      sourceTileID,
      dem.uid,
      dem.stride,
      dem.dim,
      dem.min,
      dem.max,
      dem.redFactor,
      dem.greenFactor,
      dem.blueFactor,
      dem.baseShift,
      rttStamp,
    ].join('|')
  }

  private getTileKey(tileId: CanonicalTileID): TileKey {
    return `${tileId.z}/${tileId.x}/${tileId.y}`
  }

  private getTileCoordKey(tileCoord: TileCoord): TileKey {
    return `${tileCoord.z}/${tileCoord.x}/${tileCoord.y}`
  }

  private getTileCoord(tileId: CanonicalTileID): TileCoord {
    return {
      z: tileId.z,
      x: tileId.x,
      y: tileId.y,
    }
  }

  private getVisibleSourceTiles() {
    return this.featureSourceIds.flatMap((sourceId) => {
      const tileManager = this.getTileManager(sourceId)
      if (!tileManager) return []

      return tileManager.getRenderableIds().flatMap((tileIdKey) => {
        const tile = tileManager.getTileByID(tileIdKey)
        if (!tile?.tileID) return []

        return [
          {
            sourceId,
            tileId: tile.tileID,
            tile,
            sourceTileKey: this.getSourceTileKey(sourceId, tile.tileID.canonical),
          },
        ]
      })
    })
  }

  private getTileManager(sourceId: string) {
    const map = this.map as unknown as InternalMap
    return (map.styleManager ?? map.style)?.tileManagers?.[sourceId]
  }

  private getSourceTileKey(sourceId: string, tileId: CanonicalTileID) {
    return `${sourceId}/${this.getTileKey(tileId)}`
  }

  private refreshSourceTerrainTileKeys() {
    this.sourceTerrainTileKeys.clear()
    for (const tile of this.transmittedSourceTiles.values()) {
      this.sourceTerrainTileKeys.add(this.getTileCoordKey(tile.tileKey))
    }
  }

  private getRttContentStamp(tile: InternalTile) {
    if (!tile.rtt?.length) return undefined

    const fingerprints = tile.rttFingerprint ? Object.entries(tile.rttFingerprint) : []
    if (fingerprints.length === 0) return undefined

    return fingerprints
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([source, fingerprint]) => `${source}:${fingerprint}`)
      .join('|')
  }
}
