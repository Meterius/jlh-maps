import { useMap } from '@indoorequal/vue-maplibre-gl'
import {
  type AddLayerObject,
  type CanvasSourceSpecification,
  type FilterSpecification,
  type GeoJSONSource,
  type MapGeoJSONFeature,
  type MapEventType,
  type MapLayerEventType,
  type MapLibreMap,
  type RasterSourceSpecification,
  type RasterTileSource,
  type SourceSpecification,
  type StyleImageInterface,
  type StyleImageMetadata,
} from 'maplibre-gl'
import { type MaybeRefOrGetter, ref, shallowRef, toValue, watch, type WatchSource } from 'vue'
import { createToggledComposable, onScopeDisposeLifo, watchDefinedOnce } from '@/composables/helper.ts'
import type { GeoJSON } from 'geojson'

let mapKeyCounter = 0

export function makeUniqueMapKey() {
  mapKeyCounter += 1
  return `uniq-map-${mapKeyCounter}`
}

export function useMapExtended(key?: symbol | string) {
  const mapInstance = useMap(key)

  const loaded = ref(false)
  const zoom = ref(0)
  const pitch = ref(0)
  const terrainEnabled = ref(false)

  watchDefinedOnce(
    () => mapInstance.map,
    (map) => {
      const updateLoaded = () => {
        loaded.value = map.loaded()
      }
      const updateZoom = () => {
        zoom.value = map.getZoom()
      }
      const updatePitch = () => {
        pitch.value = map.getPitch()
      }
      const updateTerrainEnabled = () => {
        terrainEnabled.value = map.getTerrain() !== null
      }

      updateLoaded()
      updateZoom()
      updatePitch()
      updateTerrainEnabled()

      onMapEvent(map, 'load', () => {
        updateLoaded()
        updateTerrainEnabled()
      })
      onMapEvent(map, 'styledata', updateTerrainEnabled)
      onMapEvent(map, 'zoom', updateZoom)
      onMapEvent(map, 'pitch', updatePitch)
    },
  )

  return {
    loaded,
    zoom,
    pitch,
    terrainEnabled,
    mapInstance,
  }
}

export type MapFeatureId = string | number

// Sources / Layers

export type UseSourceSpecification = SourceSpecification | CanvasSourceSpecification

export function useSource(map: MapLibreMap, sourceId: string, source: UseSourceSpecification) {
  map.addSource(sourceId, source)

  onScopeDisposeLifo(() => {
    if (map.getSource(sourceId)) {
      map.removeSource(sourceId)
    }
  })
}

export function useGeoJsonSource(map: MapLibreMap, sourceId: string, data: WatchSource<GeoJSON>) {
  useSource(map, sourceId, {
    type: 'geojson',
    data: toValue(data),
  })

  watch(
    data,
    (updatedData) => {
      map.getSource<GeoJSONSource>(sourceId)?.setData(updatedData)
    },
    { immediate: true },
  )
}

export type UseRasterTilesBasedSourceSpecification = Omit<
  RasterSourceSpecification,
  'type' | 'url' | 'tiles'
> & {
  tiles: MaybeRefOrGetter<string[]>
}

export function useRasterTilesBasedSource(
  map: MapLibreMap,
  sourceId: string,
  source: UseRasterTilesBasedSourceSpecification,
) {
  const { tiles, ...sourceOptions } = source

  useSource(map, sourceId, {
    type: 'raster',
    ...sourceOptions,
    tiles: toValue(tiles),
  })

  watch(
    () => toValue(tiles),
    (updatedTiles) => {
      map.getSource<RasterTileSource>(sourceId)?.setTiles(updatedTiles)
    },
    { immediate: true },
  )
}

export interface UseLayerOptions {
  beforeId?: string
  visible?: WatchSource<boolean>
}

export function useLayer(map: MapLibreMap, layer: AddLayerObject, options: UseLayerOptions = {}) {
  const { beforeId, visible } = options

  map.addLayer(layer, beforeId)

  if (visible !== undefined) {
    watch(
      visible,
      (value) => {
        if (map.getLayer(layer.id)) {
          map.setLayoutProperty(layer.id, 'visibility', value ? 'visible' : 'none')
        }
      },
      { immediate: true },
    )
  }

  onScopeDisposeLifo(() => {
    if (map.getLayer(layer.id)) {
      map.removeLayer(layer.id)
    }
  })
}

export function useLayerFeatureIdExclusionFilter(
  map: MapLibreMap,
  layerId: string,
  excludedFeatureIds: WatchSource<MapFeatureId[]>,
) {
  const originalFilter = map.getFilter(layerId)

  const makeFilter = (featureIds: MapFeatureId[]): FilterSpecification | null => {
    const uniqueFeatureIds = [...new Set(featureIds)]

    if (uniqueFeatureIds.length === 0) {
      return originalFilter ?? null
    }

    const exclusionFilter = ['!in', '$id', ...uniqueFeatureIds] as FilterSpecification

    return originalFilter
      ? (['all', originalFilter, exclusionFilter] as unknown as FilterSpecification)
      : exclusionFilter
  }

  watch(
    () => [...toValue(excludedFeatureIds)],
    (featureIds) => {
      if (map.getLayer(layerId)) {
        map.setFilter(layerId, makeFilter(featureIds))
      }
    },
    { immediate: true, flush: 'sync' },
  )

  onScopeDisposeLifo(() => {
    if (map.getLayer(layerId)) {
      map.setFilter(layerId, originalFilter ?? null)
    }
  })
}

export type MapLibreMapImageData =
  | HTMLImageElement
  | ImageBitmap
  | ImageData
  | {
      width: number
      height: number
      data: Uint8Array | Uint8ClampedArray
    }
  | StyleImageInterface

type ImageAddedCallback = (image: MapLibreMapImageData, imageId: string) => void

export type UseImageOptions = {
  options?: Partial<StyleImageMetadata>
  onImageAdded?: ImageAddedCallback
}

export function useImage(
  map: MapLibreMap,
  imageId: string,
  image: MapLibreMapImageData | Promise<MapLibreMapImageData>,
  { options, onImageAdded }: UseImageOptions = {},
) {
  let removed = false
  let added = false

  if (image instanceof Promise) {
    image.then((resolvedImage) => {
      if (!removed) {
        map.addImage(imageId, resolvedImage, options)
        added = true
        onImageAdded?.(resolvedImage, imageId)
      }
    })
  } else {
    map.addImage(imageId, image, options)
    added = true
    onImageAdded?.(image, imageId)
  }

  onScopeDisposeLifo(() => {
    removed = true
    if (added) {
      added = false
      map.removeImage(imageId)
    }
  })
}

export type OnDemandImageProviderOptions<T> = {
  getParamsForImageId: (imageId: string) => T | null
  getInitialImage: (params: T) => {
    image: MapLibreMapImageData
    options?: Partial<StyleImageMetadata>
  }
  fetchImage: (params: T) => Promise<{
    image: MapLibreMapImageData
  }>
  onImageAdded?: ImageAddedCallback
}

export function useOnDemandImageProvider<T>(
  map: MapLibreMap,
  options: OnDemandImageProviderOptions<T>,
) {
  let removed = false
  const registeredImages = new Set<string>()

  const missingSubscription = map.on('styleimagemissing', (event) => {
    const imageId = event.id

    if (registeredImages.has(imageId)) return

    const params = options.getParamsForImageId(imageId)
    if (params === null) return

    registeredImages.add(imageId)

    const initialImage = options.getInitialImage(params)
    map.addImage(imageId, initialImage.image, initialImage.options)
    options.onImageAdded?.(initialImage.image, imageId)

    options.fetchImage(params).then((image) => {
      if (!removed) {
        map.updateImage(imageId, image.image)
        options.onImageAdded?.(image.image, imageId)
        map.triggerRepaint()
      }
    }, console.error)
  })

  onScopeDisposeLifo(() => {
    if (removed) return

    removed = true
    missingSubscription.unsubscribe()
    registeredImages.forEach((imageId) => {
      map.removeImage(imageId)
    })
  })
}

// Events

export function onMapEvent<T extends keyof MapEventType>(
  map: MapLibreMap,
  type: T,
  listener: (ev: MapEventType[T] & object) => void,
) {
  const sub = map.on(type, listener)
  onScopeDisposeLifo(() => {
    sub.unsubscribe()
  })
}

export function onMapLayerEvent<T extends keyof MapLayerEventType>(
  map: MapLibreMap,
  type: T,
  layer: string,
  listener: (ev: MapLayerEventType[T] & object) => void,
) {
  const sub = map.on(type, layer, listener)
  onScopeDisposeLifo(() => {
    sub.unsubscribe()
  })
}

export type MapLayerFeatureEvent<T extends keyof MapLayerEventType> = {
  originalEvent: MapLayerEventType[T] & object
  feature: MapGeoJSONFeature
}

export function onMapLayerFeatureEvent<T extends keyof MapLayerEventType>(
  map: MapLibreMap,
  type: T,
  layer: string,
  listener: (ev: MapLayerFeatureEvent<T>) => void,
) {
  onMapLayerEvent(map, type, layer, (originalEvent) => {
    ;(originalEvent.features ?? []).forEach((feature) => {
      if (feature.layer.id === layer) {
        listener({
          originalEvent,
          feature,
        })
      }
    })
  })
}

//

export function useHoverFeatureState(
  map: MapLibreMap,
  layerId: string,
  isHoveredPropertyName: string,
  enabled: MaybeRefOrGetter<boolean> = () => true,
) {
  const layer = map.getLayer(layerId)
  if (!layer) {
    throw new Error(`Layer ${layerId} not found`)
  }

  const hoveredFeatures = shallowRef<Record<MapFeatureId, MapGeoJSONFeature>>({})

  const updateFeatureHoveredFeatureIds = (next: Record<MapFeatureId, MapGeoJSONFeature>) => {
    Object.keys(hoveredFeatures.value).forEach((featureId) => {
      map.removeFeatureState(getFeatureIdentifier(featureId), isHoveredPropertyName)
    })

    Object.keys(next).forEach((featureId) => {
      map.setFeatureState(getFeatureIdentifier(featureId), {
        [isHoveredPropertyName]: true,
      })
    })

    hoveredFeatures.value = next
  }

  const getFeatureIdentifier = (featureId: string | number) => ({
    id: featureId,
    source: layer.source,
    sourceLayer: layer.sourceLayer,
  })

  const onFeatures = (event: { features?: MapGeoJSONFeature[] }) => {
    const nextHoveredFeatures = Object.fromEntries(
      (event.features ?? []).flatMap((feature) =>
        feature.id !== undefined ? [[feature.id, feature]] : [],
      ),
    )
    updateFeatureHoveredFeatureIds(nextHoveredFeatures)
  }

  createToggledComposable(
    enabled,
    () => {
      onMapLayerEvent(map, 'mousemove', layerId, onFeatures)
      onMapLayerEvent(map, 'mouseleave', layerId, onFeatures)
      return {}
    }
  )

  watch(() => toValue(enabled), (value) => {
    if (!value) {
      updateFeatureHoveredFeatureIds({})
    }
  })

  onScopeDisposeLifo(() => {
    updateFeatureHoveredFeatureIds({})
  })

  return {
    hoveredFeatures,
  }
}

// Other

const mapHashKeys = new WeakMap<MapLibreMap, number>()
let mapHashKeyCounter = 0

export function getMapHashKey(map: MapLibreMap): number {
  let mapHashKey = mapHashKeys.get(map)

  if (mapHashKey === undefined) {
    mapHashKey = mapHashKeyCounter++
    mapHashKeys.set(map, mapHashKey)
  }

  return mapHashKey
}
