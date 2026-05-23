import { useMap } from '@indoorequal/vue-maplibre-gl'
import { extractOsmIdFromOmtFeatureId, type OsmId } from '@/utils/osm.ts'
import {
  type AddLayerObject,
  type CanvasSourceSpecification,
  type GeoJSONFeature,
  type GeoJSONSource,
  LngLat,
  type MapGeoJSONFeature,
  type MapLayerMouseEvent,
  type MapLibreMap,
  type MapMouseEvent,
  type RasterSourceSpecification,
  type RasterTileSource,
  type SourceSpecification,
  type StyleImageInterface,
  type StyleImageMetadata,
  type Subscription,
} from 'maplibre-gl'
import {
  type MaybeRefOrGetter,
  onUnmounted,
  ref,
  shallowRef,
  toValue,
  watch,
  type WatchSource,
} from 'vue'
import { get } from '@vueuse/core'
import { onScopeDisposeLifo, onWatcherCleanupLifo, watchDefinedOnce } from '@/composables/helper.ts'
import type { GeoJSON } from 'geojson'

const CLICK_LAYER_SYNC_BUFFER_MS = 50

export interface SelectionItem {
  osm_id?: OsmId
  coords: LngLat
  feature: GeoJSONFeature
}

let mapKeyCounter = 0

export function makeUniqueMapKey() {
  mapKeyCounter += 1
  return `uniq-map-${mapKeyCounter}`
}

export function useMapSelection(options: {
  key?: symbol | string
  targetLayers: WatchSource<string[]>
}) {
  const mapInstance = useMap(options.key)

  const selection = shallowRef<SelectionItem[]>([])
  let lastTargetLayerClick: MapMouseEvent | undefined
  let clearSelectionTimeout: ReturnType<typeof setTimeout> | undefined

  const clicksMatch = (click: MapMouseEvent, targetLayerClick: MapMouseEvent | undefined) => {
    if (!targetLayerClick) {
      return false
    }

    return (
      click.originalEvent === targetLayerClick.originalEvent ||
      (click.originalEvent.timeStamp === targetLayerClick.originalEvent.timeStamp &&
        click.point.x === targetLayerClick.point.x &&
        click.point.y === targetLayerClick.point.y)
    )
  }

  const makeOnClick = (targetLayers: string[]) => (e: MapLayerMouseEvent) => {
    console.log('Click Event', e, e.features)
    lastTargetLayerClick = e

    const features = e.features?.filter((f) => targetLayers.includes(f.layer.id)) ?? []
    const selectedFeature = features[0]

    if (selectedFeature) {
      selection.value = [
        {
          coords:
            selectedFeature.geometry.type === 'Point'
              ? new LngLat(
                  selectedFeature.geometry.coordinates[0] ?? 0,
                  selectedFeature.geometry.coordinates[1] ?? 0,
                )
              : e.lngLat,
          feature: selectedFeature,
          osm_id:
            typeof selectedFeature.id === 'number'
              ? (extractOsmIdFromOmtFeatureId(selectedFeature.id) ?? undefined)
              : undefined,
        },
      ]
    } else {
      selection.value = []
    }
  }

  const onMapClick = (e: MapMouseEvent) => {
    if (clearSelectionTimeout) {
      clearTimeout(clearSelectionTimeout)
      clearSelectionTimeout = undefined
    }

    clearSelectionTimeout = setTimeout(() => {
      clearSelectionTimeout = undefined

      if (!clicksMatch(e, lastTargetLayerClick)) {
        selection.value = []
      }
    }, CLICK_LAYER_SYNC_BUFFER_MS)
  }

  let onClickSubscription: Subscription | undefined
  let onMapClickSubscription: Subscription | undefined
  watch(
    () => ({
      map: mapInstance.map,
      targetLayers: [...get(options.targetLayers)],
    }),
    ({ map, targetLayers }) => {
      onClickSubscription?.unsubscribe()
      onMapClickSubscription?.unsubscribe()
      onClickSubscription = undefined
      onMapClickSubscription = undefined
      lastTargetLayerClick = undefined

      if (map) {
        onClickSubscription = map.on('click', targetLayers, makeOnClick(targetLayers))
        onMapClickSubscription = map.on('click', onMapClick)
      }
    },
    { immediate: true },
  )

  onUnmounted(() => {
    if (clearSelectionTimeout) {
      clearTimeout(clearSelectionTimeout)
      clearSelectionTimeout = undefined
    }

    onClickSubscription?.unsubscribe()
    onMapClickSubscription?.unsubscribe()
    onClickSubscription = undefined
    onMapClickSubscription = undefined
  })

  return {
    selection,
  }
}

export function useHoverFeatureState(
  map: MapLibreMap,
  layerId: string,
  isHoveredPropertyName: string,
) {
  const layer = map.getLayer(layerId)
  if (!layer) {
    throw new Error(`Layer ${layerId} not found`)
  }

  let hoveredFeatureIds: (string | number)[] = []
  const updateFeatureHoveredFeatureIds = (next: (string | number)[]) => {
    hoveredFeatureIds.forEach((featureId) => {
      map.removeFeatureState(getFeatureIdentifier(featureId), isHoveredPropertyName)
    })

    next.forEach((featureId) => {
      map.setFeatureState(getFeatureIdentifier(featureId), {
        [isHoveredPropertyName]: true,
      })
    })

    hoveredFeatureIds = next
  }

  const extractLayerFeatureIds = (features: MapGeoJSONFeature[]) =>
    features.flatMap((feature) =>
      feature.layer.id === layerId && feature.id !== undefined ? [feature.id] : [],
    )

  const subscriptions = [
    map.on('mousemove', layerId, (event) => {
      updateFeatureHoveredFeatureIds(extractLayerFeatureIds(event.features ?? []))
    }),
    map.on('mouseleave', layerId, (event) => {
      updateFeatureHoveredFeatureIds(extractLayerFeatureIds(event.features ?? []))
    }),
  ]

  const getFeatureIdentifier = (featureId: string | number) => ({
    id: featureId,
    source: layer.source,
    sourceLayer: layer.sourceLayer,
  })

  onScopeDisposeLifo(() => {
    subscriptions.forEach((subscription) => {
      subscription.unsubscribe()
    })
    updateFeatureHoveredFeatureIds([])
  })
}

export function useMapExtended(key?: symbol | string) {
  const mapInstance = useMap(key)

  const loaded = ref(false)
  const zoom = ref(0)
  const pitch = ref(0)

  watchDefinedOnce(
    () => mapInstance.map,
    (map) => {
      zoom.value = map.getZoom()
      loaded.value = map.loaded()
      pitch.value = map.getPitch()

      const subscriptions = [
        map.on('load', () => {
          loaded.value = true
        }),
        map.on('zoom', () => {
          zoom.value = map.getZoom()
        }),
        map.on('pitch', () => {
          pitch.value = map.getPitch()
        }),
      ]

      onWatcherCleanupLifo(() => {
        subscriptions.forEach((sub) => sub.unsubscribe())
      })
    },
  )

  return {
    loaded,
    zoom,
    pitch,
    mapInstance,
  }
}

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

export function useImage(
  map: MapLibreMap,
  imageId: string,
  image: MapLibreMapImageData,
  {
    options,
    onImageAdded,
  }: {
    options?: Partial<StyleImageMetadata>
    onImageAdded?: ImageAddedCallback
  },
) {
  map.addImage(imageId, image, options)
  onImageAdded?.(image, imageId)

  onScopeDisposeLifo(() => {
    map.removeImage(imageId)
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
