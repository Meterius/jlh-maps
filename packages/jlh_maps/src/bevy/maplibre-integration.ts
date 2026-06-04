import { releaseProxy, transfer, type Remote } from 'comlink'
import { type Map as MapLibreMap, type Tile } from 'maplibre-gl'
import { effectScope, type MaybeRefOrGetter, shallowRef, toValue } from 'vue'
import { onScopeDisposeLifo, onWatcherCleanupLifo, watchDefinedOnce } from '@/composables/helper.ts'
import { useMap } from '@indoorequal/vue-maplibre-gl'
import type {
  CanonicalTileID,
  DEMData,
  Map as InternalMap,
  Tile as InternalTile,
} from 'maplibre-gl/src/index.ts'
// @ts-expect-error Class not properly exported by dist
import { OverscaledTileID } from 'maplibre-gl/src/tile/tile_id'
import { useBevy } from '@/bevy/index.ts'
import type { MaplibreIntegration } from '@/bevy/bevy.worker.ts'
import { LRUCache } from 'lru-cache'
import { onMapEvent } from '@/composables/maplibre'

type SerializedCanonicalTileID = string

function serializeCanonicalTileId(tileId: CanonicalTileID): SerializedCanonicalTileID {
  return `${tileId.z}/${tileId.x}/${tileId.y}`
}

const SOURCE_TILE_LRU_CAPACITY = 64
const TERRAIN_TILE_LRU_CAPACITY = 64

interface MaplibreGlJsIntegrationOptions {
  sourceIds: string[]
}

type RawTileData = ArrayBuffer | ArrayBufferView

interface InternalTileManager {
  getRenderableIds?: () => string[]
  getTileByID?: (id: string) => Tile | undefined
}

export function useMaplibreIntegration(
  instanceId: string,
  mapKey: string | symbol | undefined,
  options: MaplibreGlJsIntegrationOptions,
) {
  const bevyInstance = useBevy(instanceId)
  const mapInstance = useMap(mapKey)

  const terrainSyncRet = shallowRef<ReturnType<typeof useTerrainSync> | null>(null)
  const viewSyncRet = shallowRef<ReturnType<typeof useViewSync> | null>(null)
  const sourceSyncRet = shallowRef<ReturnType<typeof useSourceSync>[] | null>(null)

  const additionalRequestedTerrainTiles = () => (sourceSyncRet.value ?? []).flatMap((ret) => ret.transmittedRenderableTiles.value)

  watchDefinedOnce(
    () => (bevyInstance.isMounted.value ? mapInstance.map : undefined),
    (map) => {
      const mountedBevyInstance = bevyInstance.bevyInstance.value
      if (!mountedBevyInstance) return

      let stopped = false
      const scope = effectScope(true)

      onWatcherCleanupLifo(() => {
        stopped = true
        scope.stop()
      })

      void mountedBevyInstance
        .create_map_integration()
        .then((bevyMapIntegration) => {
          const remoteIntegration = bevyMapIntegration as Remote<MaplibreIntegration>

          if (stopped) {
            void remoteIntegration.free().finally(() => remoteIntegration[releaseProxy]())
            return
          }

          scope.run(() => {
            onScopeDisposeLifo(() => {
              void remoteIntegration.free().finally(() => remoteIntegration[releaseProxy]())
            })

            terrainSyncRet.value = useTerrainSync(map, remoteIntegration, additionalRequestedTerrainTiles)
            viewSyncRet.value = useViewSync(map, remoteIntegration)
            sourceSyncRet.value = options.sourceIds.map((sourceId) => useSourceSync(
              map,
              remoteIntegration,
              sourceId,
            ))

            onScopeDisposeLifo(() => {
              terrainSyncRet.value = null
              viewSyncRet.value = null
              sourceSyncRet.value = null
            })
          })
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
      Promise.all([
        // source tile sync should run before terrain, so additional request terrain tiles is up to date
        Promise.all((sourceSyncRet.value ?? []).map((ret) => ret.syncSourceTiles())),
        terrainSyncRet.value?.syncTerrain() ?? Promise.resolve(),
        viewSyncRet.value?.syncView(frameIdx) ?? Promise.resolve(),
      ]),
  }
}

// handles synchronization of the terrain properties of the integration
// - terrain data is only synchronized when calling `syncTerrain` (see `syncTerrain` for details)
function useTerrainSync(
  map: MapLibreMap,
  bevyMapIntegration: Remote<MaplibreIntegration>,
  additionalRequestedTerrainTiles: MaybeRefOrGetter<CanonicalTileID[]>,
) {
  type TerrainTileData = { hash: string, tileId: CanonicalTileID }

  // invariant: any tile id has transmitted but not removed terrain tile data if and only if it contained in inactive or active
  const activeTerrainTileData = new Map<SerializedCanonicalTileID, TerrainTileData>
  const inactiveTerrainTileDataCache = new LRUCache<string, TerrainTileData, unknown>({
    max: TERRAIN_TILE_LRU_CAPACITY,
    dispose: (_value, key, reason) => {
      // entries are deleted only if they cleared on disposal which handles removal, or
      // if they are moved from inactive to active, hence the guard
      if (reason !== 'delete' && reason !== 'set') {
        bevyMapIntegration.remove_terrain_tile_data(key).catch(console.error)
      }
    }
  })

  const activateTerrainTileData = (tileIdSer: string) => {
    const data = inactiveTerrainTileDataCache.get(tileIdSer)
    if (data) {
      inactiveTerrainTileDataCache.delete(tileIdSer)
      activeTerrainTileData.set(tileIdSer, data)
    }
  }

  const deactivateTerrainTileData = (tileIdSer: string) => {
    const data = activeTerrainTileData.get(tileIdSer)
    if (data) {
      activeTerrainTileData.delete(tileIdSer)
      inactiveTerrainTileDataCache.set(tileIdSer, data)
    }
  }

  const getActiveTileIds = () => {
    const activeTileIds = new Map<SerializedCanonicalTileID, CanonicalTileID>()

    if (map.terrain) {
      (map.terrain.tileManager.getRenderableTiles() ?? []).forEach((tile) => {
        activeTileIds.set(serializeCanonicalTileId(tile.tileID.canonical), tile.tileID.canonical)
      });
    } else {
        map
          .coveringTiles({
            tileSize: 512,
          })
          .forEach((tileId) => {
            activeTileIds.set(serializeCanonicalTileId(tileId.canonical), tileId.canonical)
          })
    }

    return activeTileIds
  }

  const makeTerrainDataHash = (
      sourceTile: InternalTile | null | undefined,
      dem: DEMData,
  ) => {
      const sourceTileID = sourceTile?.tileID?.key ?? sourceTile?.tileID?.toString?.() ?? 'none'
      const rttStamp = sourceTile ? (makeRttContentStamp(sourceTile) ?? 'none') : 'none'

      return [
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

  const makeRttContentStamp = (tile: InternalTile) => {
    if (!tile.rtt?.length) return undefined

    const fingerprints = tile.rttFingerprint ? Object.entries(tile.rttFingerprint) : []
    if (fingerprints.length === 0) return undefined

    return fingerprints
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([source, fingerprint]) => `${source}:${fingerprint}`)
      .join('|')
  }

  // checks whether terrain has changed, either due to terrain data changes or being removed,
  // transmits/removes the terrain data and updates the corresponding activeTerrainTileData/inactiveTerrainTileData entry
  const syncTerrainDataForTile = async (tileId: CanonicalTileID) => {
    const tileIdSer = serializeCanonicalTileId(tileId)
    const active = activeTerrainTileData.has(tileIdSer)

    if (map.terrain) {
      // @ts-expect-error Weird class export issue
      const overscaledTileId = new OverscaledTileID(tileId.z, 0, tileId.z, tileId.x, tileId.y)

      const terrainData = map.terrain.getTerrainData(overscaledTileId)
      const dem = terrainData.tile?.dem
      if (!dem) return

      const hash = makeTerrainDataHash(terrainData.tile, dem)
      const data = active ? activeTerrainTileData.get(tileIdSer) : inactiveTerrainTileDataCache.get(tileIdSer)

      if (data?.hash === hash) return

      if (active) {
        activeTerrainTileData.set(tileIdSer, { hash, tileId })
      } else {
        inactiveTerrainTileDataCache.set(tileIdSer, { hash, tileId })
      }

      const terrainDataBuffer = new Uint32Array(dem.data)
      await bevyMapIntegration.update_terrain_tile_data(
        tileIdSer,
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
    } else {
      if (active && activeTerrainTileData.has(tileIdSer)) {
        activeTerrainTileData.delete(tileIdSer)
        await bevyMapIntegration.remove_terrain_tile_data(tileIdSer)
      } else if (!active && inactiveTerrainTileDataCache.has(tileIdSer)) {
        inactiveTerrainTileDataCache.delete(tileIdSer)
        await bevyMapIntegration.remove_terrain_tile_data(tileIdSer)
      }
    }
  };

  // synchronizes terrain data:
  // - updates the active terrain tile list of the integration
  // - deletes stale terrain data using an LRU cache for inactive terrain tiles
  // - updates data of active terrain tiles
  // guarantees:
  // - all update messages are sent before yielding
  const syncTerrain = async () => {
    const activeTileIds = getActiveTileIds();

    // only sync as active tiles those used for terrain tiles
    const activeTileIdSyncPromise = bevyMapIntegration.sync_terrain_active_tile_ids([...activeTileIds.keys()])

    // additionally make the requested tile ids be synchronized
    toValue(additionalRequestedTerrainTiles).forEach((tileId) => {
      activeTileIds.set(serializeCanonicalTileId(tileId), tileId)
    })

    // move from or into inactive cache (also handles removing transmitted tile data on LRU dispose)
    for (const tileIdSer of activeTileIds.keys()) { activateTerrainTileData(tileIdSer) }
    activeTerrainTileData.forEach((_value, tileIdSer) => {
      if (!activeTileIds.has(tileIdSer)) {
        deactivateTerrainTileData(tileIdSer)
      }
    })

    await Promise.all([
      activeTileIdSyncPromise,
        ...[...activeTileIds.values()].map(syncTerrainDataForTile),
      ...[...inactiveTerrainTileDataCache.values()].map((data) => syncTerrainDataForTile(data.tileId)),
    ])
  };

  onScopeDisposeLifo(() => {
    [...activeTerrainTileData.keys(), ...inactiveTerrainTileDataCache.keys()].forEach((tileIdSer) => {
      bevyMapIntegration.remove_terrain_tile_data(tileIdSer).catch(console.error)
    })
    bevyMapIntegration.sync_terrain_active_tile_ids([]).catch(console.error)
    activeTerrainTileData.clear()
    inactiveTerrainTileDataCache.clear()
  })

  return {
    syncTerrain,
  }
}

// handles synchronization of camera data
// note: `syncView` is expected to be called before a `tick` of bevy can be issued to ensure
// camera data is up to date every frame
function useViewSync(map: MapLibreMap, bevyMapIntegration: Remote<MaplibreIntegration>) {
  const getMainMatrix = () => {
    const transform = (map as unknown as InternalMap).transform
    const matrix = transform.getProjectionDataForCustomLayer().mainMatrix
    if (!matrix) return undefined
    return new Float64Array(matrix)
  }

  const syncView = async (frameId: number) => {
    const center = map.getCenter()
    const canvas = map.getCanvas()
    const mainMatrix = getMainMatrix()
    if (!mainMatrix) return

    await bevyMapIntegration.sync_view(
      frameId,
      canvas.width,
      canvas.height,
      map.getZoom(),
      map.getPitch(),
      map.getBearing(),
      center.lng,
      center.lat,
      mainMatrix,
    )
  }

  return {
    syncView
  }
}

// handles synchronization of source vector tile data:
// - sourcedata events cause tile-based data synchronization
// - `syncRenderableSourceTiles` updates the renderable source tiles list and transmits the initial tile data
function useSourceSync(
  map: MapLibreMap,
  bevyMapIntegration: Remote<MaplibreIntegration>,
  sourceId: string,
) {
  const transmittedRenderableTiles = shallowRef<CanonicalTileID[]>([])

  // invariant: any tile id has transmitted but not removed source tile data if and only if it contained in inactive or active
  const activeSourceTiles = new Set<SerializedCanonicalTileID>()
  const inactiveSourceTileCache = new LRUCache<SerializedCanonicalTileID, object, unknown>({
    max: SOURCE_TILE_LRU_CAPACITY,
    dispose: (_data, tileIdSer, reason) => {
      if (reason !== 'delete' && reason !== 'set') {
        bevyMapIntegration.remove_source_tile(sourceId, tileIdSer).catch(console.error)
      }
    },
  })

  const activateSourceTile = (tileIdSer: string) => {
    const data = inactiveSourceTileCache.get(tileIdSer)
    if (data) {
      inactiveSourceTileCache.delete(tileIdSer)
      activeSourceTiles.add(tileIdSer)
    }
  }

  const deactivateSourceTile = (tileIdSer: string) => {
    if (activeSourceTiles.has(tileIdSer)) {
      inactiveSourceTileCache.set(tileIdSer, {})
      activeSourceTiles.delete(tileIdSer)
    }
  }

  const getTileManager = (): InternalTileManager | undefined => {
    const mapInternal = map as unknown as InternalMap
    return (mapInternal.styleManager ?? mapInternal.style)?.tileManagers?.[sourceId] as
      | InternalTileManager
      | undefined
  }

  const getRenderableTiles = (): Tile[] => {
    const tileManager = getTileManager()
    if (!tileManager) return []

    return (tileManager.getRenderableIds?.() ?? []).flatMap((tileIdKey) => {
      const tile = tileManager.getTileByID?.(tileIdKey)
      return tile ? [tile] : []
    })
  }

  const getRawTileData = (tile: Tile): Uint8Array | undefined => {
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

  const syncSourceTile = async (tileIdSer: string, tile: Tile) => {
    const active = activeSourceTiles.has(tileIdSer)
    const rawTileData = getRawTileData(tile)

    if (!rawTileData) {
      if (activeSourceTiles.has(tileIdSer)) {
        activeSourceTiles.delete(tileIdSer)
      }
      if (inactiveSourceTileCache.has(tileIdSer)) {
        inactiveSourceTileCache.delete(tileIdSer)
      }

      await bevyMapIntegration.remove_source_tile(sourceId, tileIdSer)
    } else {
      if (active) {
        activeSourceTiles.add(tileIdSer)
      } else {
        inactiveSourceTileCache.set(tileIdSer, {})
      }

      await bevyMapIntegration.update_source_tile(
        sourceId,
        tileIdSer,
        transfer(rawTileData, [rawTileData.buffer]),
      )
    }
  }

  onMapEvent(map, 'sourcedata', (event) => {
    if (sourceId !== event.sourceId) {
      return
    }

    const eventTile = event.tile as Tile | undefined
    if (eventTile) {
      const tileIdSer = serializeCanonicalTileId(eventTile.tileID.canonical)

      if (activeSourceTiles.has(tileIdSer) || inactiveSourceTileCache.has(tileIdSer)) {
        syncSourceTile(tileIdSer, eventTile).catch(console.error)
      }
    }
  })

  // handles tiles becoming active to transmit their data as remaining tiles will always have already transmitted
  // via sourcedata hooks
  const syncActiveSourceTile = async (tileIdSer: string, tile: Tile) => {
    if (activeSourceTiles.has(tileIdSer)) return

    activeSourceTiles.add(tileIdSer)
    await syncSourceTile(tileIdSer, tile)
  }

  // synchronizes source data:
  // - updates the renderable tile list of the source integration
  // - deletes stale tile data using an LRU cache for inactive source tiles
  // - updates data of tiles that have not previously been transmitted
  // guarantees:
  // - all update messages are sent before yielding
  const syncSourceTiles = async () => {
    const renderableTiles = getRenderableTiles()
    transmittedRenderableTiles.value = renderableTiles.map(tile => tile.tileID.canonical)

    const activeTileIdsSer = new Set(
      renderableTiles.map((tile) => serializeCanonicalTileId(tile.tileID.canonical)),
    )

    // move from or into inactive cache (also handles removing transmitted tile data on LRU dispose)
    activeTileIdsSer.forEach(activateSourceTile)
    activeSourceTiles.forEach((_value, tileIdSer) => {
      if (!activeTileIdsSer.has(tileIdSer)) {
        deactivateSourceTile(tileIdSer)
      }
    })

    await Promise.all([
      bevyMapIntegration.sync_source_renderable_tile_ids(sourceId, [...activeTileIdsSer]),
      ...renderableTiles.map((tile) =>
        syncActiveSourceTile(serializeCanonicalTileId(tile.tileID.canonical), tile),
      ),
    ])
  }

  onScopeDisposeLifo(() => {
    [...activeSourceTiles.values(), ...inactiveSourceTileCache.keys()].forEach((tileIdSer) => {
      bevyMapIntegration.remove_source_tile(sourceId, tileIdSer).catch(console.error)
    })
    bevyMapIntegration.sync_source_renderable_tile_ids(sourceId, []).catch(console.error)
    activeSourceTiles.clear()
    inactiveSourceTileCache.clear()
  })

  return {
    syncSourceTiles,
    transmittedRenderableTiles,
  }
}
