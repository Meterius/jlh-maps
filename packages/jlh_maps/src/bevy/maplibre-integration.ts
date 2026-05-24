import {
  create_map_integration,
  remove_map_integration,
  remove_source_tile,
  remove_terrain_tile_data,
  sync_terrain_active_tile_ids,
  sync_view,
  update_source_tile,
  update_terrain_tile_data,
} from 'jlh_maps_app'
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
      const mapIntegrationId = create_map_integration(instanceId)
      const integration = new MaplibreGlJsIntegration(
        map,
        instanceId,
        mapIntegrationId,
        options.featureSourceIds ?? [],
      )
      integration.start()

      mapIntegration.value = integration

      onWatcherCleanupLifo(() => {
        integration.stop()
        remove_map_integration(instanceId, mapIntegrationId)
      })
    },
  )

  return {
    syncOnRender: () => mapIntegration.value?.syncOnRender(),
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
    private readonly instanceId: string,
    private readonly mapIntegrationId: number,
    private readonly featureSourceIds: string[],
  ) {}

  start() {
    console.log('Starting maplibre integration on map: ', this.map)

    this.unsubscribeCallbacks.push(
      this.map.on('sourcedata', (event) => this.handleSourceData(event)).unsubscribe,
    )
  }

  syncOnRender() {
    this.syncView()
    this.syncTerrain()
    this.syncVisibleSourceTiles()
  }

  stop() {
    if (this.stopped) return
    this.stopped = true

    if (this.syncViewFrame !== undefined) {
      cancelAnimationFrame(this.syncViewFrame)
      this.syncViewFrame = undefined
    }
    if (this.syncDataFrame !== undefined) {
      cancelAnimationFrame(this.syncDataFrame)
      this.syncDataFrame = undefined
    }

    this.removeTerrainData([...this.terrainDataHashes.keys()])
    this.removeTransmittedSourceTiles([...this.transmittedSourceTiles.keys()])
    this.unsubscribeCallbacks.splice(0).forEach((unsubscribe) => unsubscribe())
  }

  private scheduleSyncData() {
    if (this.syncDataFrame !== undefined) return

    this.syncDataFrame = requestAnimationFrame(() => {
      this.syncDataFrame = undefined
      this.syncData()
    })
  }

  private syncView() {
    const center = this.map.getCenter()
    const canvas = this.map.getCanvas()
    const mainMatrix = this.getMainMatrix()
    if (!mainMatrix) return

    sync_view(
      this.instanceId,
      this.mapIntegrationId,
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

  private syncData() {
    this.syncSourceTiles()
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

  private syncSourceTiles() {
    if (this.featureSourceIds.length === 0) {
      this.removeTransmittedSourceTiles([...this.transmittedSourceTiles.keys()])
      this.pendingChangedSourceTiles.clear()
      this.sourceTerrainTileKeys.clear()
      this.pruneTerrainData()
      return
    }

    const changedTiles = [...this.pendingChangedSourceTiles.values()]
    this.pendingChangedSourceTiles.clear()

    for (const { sourceId, tileId, tile } of changedTiles) {
      this.syncSourceTile(sourceId, tileId, tile)
    }

    this.refreshSourceTerrainTileKeys()
    this.pruneTerrainData()
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

  private syncVisibleSourceTiles() {
    const visibleTiles = this.getVisibleSourceTiles()
    const visibleTileKeys = new Set(visibleTiles.map((tile) => tile.sourceTileKey))
    const removedTileKeys = [...this.transmittedSourceTiles.keys()].filter(
      (tileKey) => !visibleTileKeys.has(tileKey),
    )
    this.removeTransmittedSourceTiles(removedTileKeys)

    for (const { sourceId, tileId, tile, sourceTileKey } of visibleTiles) {
      if (this.transmittedSourceTiles.has(sourceTileKey)) continue
      this.syncSourceTile(sourceId, tileId, tile)
    }

    this.refreshSourceTerrainTileKeys()
    this.pruneTerrainData()
  }

  private syncSourceTile(sourceId: string, tileId: OverscaledTileID, tile: Tile) {
    const tileCoord = this.getTileCoord(tileId.canonical)
    const sourceTileKey = this.getSourceTileKey(sourceId, tileId.canonical)
    const rawTileData = this.getTileRawData(tile)

    if (!rawTileData) {
      this.removeTransmittedSourceTiles([sourceTileKey])
      return
    }

    update_source_tile(
      this.instanceId,
      this.mapIntegrationId,
      sourceId,
      tileCoord.z,
      tileCoord.x,
      tileCoord.y,
      rawTileData,
    )
    this.syncTerrainDataForTileCoords([tileCoord])
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

  private syncTerrain() {
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

      this.pruneTerrainData()

      sync_terrain_active_tile_ids(this.instanceId, this.mapIntegrationId, [
        ...activeTerrainTileIds,
      ])

      for (const tile of activeTerrainTiles) {
        const key = this.getTileKey(tile.tileID.canonical)
        this.syncTerrainDataForTileId(key, tile.tileID, tile)
      }
    } else {
      this.activeTerrainTileKeys.clear()
      // terrain is not available, remove all terrain data that may have existed while terrain was active
      if (this.terrainDataHashes.size !== 0) {
        this.removeTerrainData([...this.terrainDataHashes.keys()])
        this.terrainDataHashes.clear()
      }

      const activeTerrainTileIds = new Set(
        this.map
          .coveringTiles({
            tileSize: 512,
          })
          .map((tileId) => this.getTileKey(tileId.canonical)),
      )

      sync_terrain_active_tile_ids(this.instanceId, this.mapIntegrationId, [
        ...activeTerrainTileIds,
      ])
    }
  }

  private syncTerrainDataForTileCoords(tileCoords: Iterable<TileCoord>) {
    if (!this.terrain) return

    for (const tileCoord of tileCoords) {
      // @ts-expect-error No clue of what is going on here
      const tileId = new OverscaledTileID(tileCoord.z, 0, tileCoord.z, tileCoord.x, tileCoord.y)
      this.syncTerrainDataForTileId(this.getTileCoordKey(tileCoord), tileId)
    }
  }

  private syncTerrainDataForTileId(
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

    update_terrain_tile_data(
      this.instanceId,
      this.mapIntegrationId,
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
      new Uint32Array(dem.data),
    )
    this.terrainDataHashes.set(tileKey, hash)
  }

  private removeTransmittedSourceTiles(tileKeys: SourceTileKey[]) {
    for (const tileKey of tileKeys) {
      const tile = this.transmittedSourceTiles.get(tileKey)
      if (tile) {
        remove_source_tile(
          this.instanceId,
          this.mapIntegrationId,
          tile.sourceId,
          tile.tileKey.z,
          tile.tileKey.x,
          tile.tileKey.y,
        )
      }
      this.transmittedSourceTiles.delete(tileKey)
    }
  }

  private removeTerrainData(tileKeys: string[]) {
    for (const tileKey of tileKeys) {
      remove_terrain_tile_data(this.instanceId, this.mapIntegrationId, tileKey)
      this.terrainDataHashes.delete(tileKey)
    }
  }

  private pruneTerrainData() {
    this.removeTerrainData(
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
