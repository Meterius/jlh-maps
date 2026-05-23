import type { ExpressionSpecification, LayerSpecification, Map as MapLibreMap } from 'maplibre-gl'
import { OMT_DEFAULT_POI_METADATA, OMT_POI_SUBCLASS_METADATA } from '@/constants/omt-mapping.ts'
import { resolvePoiIconSvg, type PoiDisplayMetadata } from '@/constants/osm-mapping.ts'
import { onScopeDisposeLifo } from '@/composables/helper.ts'
import { useLayer, useOnDemandImageProvider } from '@/composables/maplibre'
import { getUsableCssColor } from '@/utils/css-color.ts'
import { makeStringPropertyMatchExpression, scaleStyleNumber } from '@/utils/maplibre.ts'
import { svgToImage, type SvgRasterImage } from '@/utils/svg-to-image.ts'
import { makeMarkerIcon } from '@/maplibre-layers/common/marker-icon.ts'

type SymbolLayerSpecification = Extract<LayerSpecification, { type: 'symbol' }>
type SymbolLayerLayout = NonNullable<SymbolLayerSpecification['layout']>
type SymbolLayerPaint = NonNullable<SymbolLayerSpecification['paint']>
type PoiMarkerImageParams = {
  imageId: string
  metadata: PoiDisplayMetadata
  marker: Required<PoiMarkerOptions>
}

type PoiMarkerOptions = {
  width?: number
  height?: number
  color?: string
  iconColor?: string
  headColor?: string
  outlineColor?: string
  shadowColor?: string
  scale?: number
  fontScale?: number
  iconScale?: number
  pixelRatio?: number
}

const DEFAULT_SOURCE_LAYER = 'poi'
const POI_MARKER_LAYER_SUFFIX = '-poi-marker'
const POI_MARKER_IMAGE_VERSION = 'v4'

const DEFAULT_MARKER_OPTIONS: Required<PoiMarkerOptions> = {
  width: 32,
  height: 36,
  color: '#2563eb',
  iconColor: '#111827',
  headColor: '#ffffff',
  outlineColor: 'rgb(15 23 42 / 0.22)',
  shadowColor: 'rgb(15 23 42 / 0.26)',
  scale: 1.25,
  fontScale: 1.25,
  iconScale: 1.0,
  pixelRatio: 2,
}

let poiIconMetadataCache: PoiDisplayMetadata[] | undefined
const poiIconSvgCache = new Map<string, Promise<string | undefined>>()
const poiMarkerImageCache = new Map<string, Promise<SvgRasterImage>>()

const sanitizeImageIdPart = (value: string) => value.replace(/[^a-zA-Z0-9_-]/g, '-')

const getMarkerStyleKey = (marker: Required<PoiMarkerOptions>) =>
  [
    marker.width,
    marker.height,
    marker.color,
    marker.iconColor,
    marker.headColor,
    marker.outlineColor,
    marker.shadowColor,
    marker.iconScale,
    marker.pixelRatio,
  ]
    .map((value) => sanitizeImageIdPart(String(value)))
    .join('-')

const getPoiMarkerImageId = (icon: string, marker: Required<PoiMarkerOptions>) =>
  `jlh-poi-marker-${POI_MARKER_IMAGE_VERSION}-${getMarkerStyleKey(marker)}-${sanitizeImageIdPart(icon)}`

const getOriginalLayerIconColor = (baseLayer: SymbolLayerSpecification) => {
  const paint = (baseLayer.paint ?? {}) as SymbolLayerPaint

  return getUsableCssColor(paint['icon-color']) ?? getUsableCssColor(paint['text-color'])
}

const makeLayerMarkerOptions = (
  baseLayer: SymbolLayerSpecification,
  marker: Required<PoiMarkerOptions>,
  overrideMarkerColor: string | undefined,
): Required<PoiMarkerOptions> => {
  const color =
    getUsableCssColor(overrideMarkerColor) ??
    getOriginalLayerIconColor(baseLayer) ??
    getUsableCssColor(marker.color) ??
    DEFAULT_MARKER_OPTIONS.color

  return {
    ...marker,
    color,
    iconColor: color,
  }
}

const getPoiIconSvg = (metadata: PoiDisplayMetadata) => {
  const cached = poiIconSvgCache.get(metadata.iconName)
  if (cached) return cached

  const iconSvg = (async () =>
    (await resolvePoiIconSvg(metadata.iconSvg)) ??
    (await resolvePoiIconSvg(OMT_DEFAULT_POI_METADATA.iconSvg)))()

  poiIconSvgCache.set(metadata.iconName, iconSvg)
  iconSvg.catch(() => poiIconSvgCache.delete(metadata.iconName))

  return iconSvg
}

const loadPoiMarkerImage = async (
  metadata: PoiDisplayMetadata,
  marker: Required<PoiMarkerOptions>,
) => {
  const markerSvg = makeMarkerIcon(await getPoiIconSvg(metadata), {
    width: marker.width,
    height: marker.height,
    color: marker.color,
    iconColor: marker.iconColor,
    headColor: marker.headColor,
    outlineColor: marker.outlineColor,
    shadowColor: marker.shadowColor,
    iconScale: marker.iconScale,
  })

  return svgToImage(markerSvg, {
    width: marker.width,
    height: marker.height,
    pixelRatio: marker.pixelRatio,
    color: marker.color,
    sourceIsRenderable: true,
  })
}

const getPoiIconMetadata = () => {
  if (poiIconMetadataCache) return poiIconMetadataCache

  const seenIconNames = new Set<string>()

  poiIconMetadataCache = [
    OMT_DEFAULT_POI_METADATA,
    ...Object.values(OMT_POI_SUBCLASS_METADATA),
  ].filter((metadata) => {
    if (seenIconNames.has(metadata.iconName)) return false

    seenIconNames.add(metadata.iconName)
    return true
  })

  return poiIconMetadataCache
}

const getCachedPoiMarkerImage = (
  imageId: string,
  metadata: PoiDisplayMetadata,
  marker: Required<PoiMarkerOptions>,
) => {
  const cached = poiMarkerImageCache.get(imageId)
  if (cached) return cached

  const image = loadPoiMarkerImage(metadata, marker).then((result) => result.image)

  poiMarkerImageCache.set(imageId, image)
  image.catch(() => poiMarkerImageCache.delete(imageId))

  return image
}

const makeEmptyPoiMarkerImage = (marker: Required<PoiMarkerOptions>): SvgRasterImage => {
  const width = Math.ceil(marker.width * marker.pixelRatio)
  const height = Math.ceil(marker.height * marker.pixelRatio)

  return {
    width,
    height,
    data: new Uint8ClampedArray(width * height * 4),
  }
}

const makePropertyIconMatchExpression = (
  property: 'class' | 'subclass',
  marker: Required<PoiMarkerOptions>,
  fallback: string | ExpressionSpecification,
) =>
  makeStringPropertyMatchExpression(
    property,
    Object.entries(OMT_POI_SUBCLASS_METADATA).map(
      ([subclass, metadata]) =>
        [subclass, getPoiMarkerImageId(metadata.iconName, marker)] as [string, string],
    ),
    fallback,
  )

const makePoiIconImageExpression = (marker: Required<PoiMarkerOptions>) =>
  makePropertyIconMatchExpression(
    'subclass',
    marker,
    makePropertyIconMatchExpression(
      'class',
      marker,
      getPoiMarkerImageId(OMT_DEFAULT_POI_METADATA.iconName, marker),
    ),
  )

const isPoiSymbolLayer = (layer: LayerSpecification): layer is SymbolLayerSpecification =>
  layer.type === 'symbol' && layer['source-layer'] === DEFAULT_SOURCE_LAYER

const getPoiSymbolLayers = (map: MapLibreMap) =>
  (map.getStyle().layers ?? []).filter(isPoiSymbolLayer)

const makePoiMarkerLayer = (
  baseLayer: SymbolLayerSpecification,
  marker: Required<PoiMarkerOptions>,
): SymbolLayerSpecification => {
  const layout = (baseLayer.layout ?? {}) as SymbolLayerLayout
  const paint = (baseLayer.paint ?? {}) as SymbolLayerPaint

  return {
    ...baseLayer,
    id: `${baseLayer.id}${POI_MARKER_LAYER_SUFFIX}`,
    layout: {
      ...layout,
      'icon-image': makePoiIconImageExpression(marker),
      'icon-size': marker.scale,
      'icon-anchor': 'bottom',
      'icon-offset': [0, 0],
      'icon-allow-overlap': layout['icon-allow-overlap'] ?? false,
      'icon-ignore-placement': layout['icon-ignore-placement'] ?? false,
      'text-field': layout['text-field'],
      'text-anchor': 'top',
      'text-offset': [0, 0.55],
      'text-size': scaleStyleNumber(
        layout['text-size'],
        marker.fontScale,
        16,
      ) as SymbolLayerLayout['text-size'],
      'text-optional': false,
      'icon-optional': false,
      'symbol-sort-key': layout['symbol-sort-key'] ?? ['to-number', ['get', 'rank']],
    },
    paint: {
      ...paint,
      'text-color': paint['text-color'] ?? '#1f2937',
      'text-halo-color': paint['text-halo-color'] ?? '#ffffff',
      'text-halo-width': paint['text-halo-width'] ?? 1.5,
    },
  }
}

const registerPoiMarkerImages = (map: MapLibreMap, marker: Required<PoiMarkerOptions>) => {
  const metadataByImageId = new Map(
    getPoiIconMetadata().map((metadata) => [
      getPoiMarkerImageId(metadata.iconName, marker),
      metadata,
    ]),
  )

  useOnDemandImageProvider<PoiMarkerImageParams>(map, {
    getParamsForImageId: (imageId) => {
      const metadata = metadataByImageId.get(imageId)
      if (!metadata) return null

      return {
        imageId,
        metadata,
        marker,
      }
    },
    getInitialImage: ({ marker }) => ({
      image: makeEmptyPoiMarkerImage(marker),
      options: {
        pixelRatio: marker.pixelRatio,
      },
    }),
    fetchImage: async ({ imageId, metadata, marker }) => ({
      image: await getCachedPoiMarkerImage(imageId, metadata, marker),
    }),
  })
}

const usePoiLayerWithRegisteredImages = (
  map: MapLibreMap,
  baseLayer: SymbolLayerSpecification,
  registeredMarkerKeys: Set<string>,
) => {
  const marker = { ...DEFAULT_MARKER_OPTIONS }
  const previousVisibility = map.getLayoutProperty(baseLayer.id, 'visibility')
  const layerMarker = makeLayerMarkerOptions(baseLayer, marker, undefined)
  const layerMarkerKey = getMarkerStyleKey(layerMarker)
  const layerId = `${baseLayer.id}${POI_MARKER_LAYER_SUFFIX}`

  if (!registeredMarkerKeys.has(layerMarkerKey)) {
    registeredMarkerKeys.add(layerMarkerKey)
    registerPoiMarkerImages(map, layerMarker)
  }

  useLayer(map, makePoiMarkerLayer(baseLayer, layerMarker), {
    beforeId: baseLayer.id,
  })

  map.setLayoutProperty(baseLayer.id, 'visibility', 'none')

  onScopeDisposeLifo(() => {
    if (map.getLayer(baseLayer.id)) {
      map.setLayoutProperty(baseLayer.id, 'visibility', previousVisibility ?? 'visible')
    }
  })

  return {
    layerId,
    baseLayerId: baseLayer.id,
  }
}

export const usePoiLayer = (map: MapLibreMap, baseLayer: SymbolLayerSpecification) =>
  usePoiLayerWithRegisteredImages(map, baseLayer, new Set())

export const usePoiLayers = (map: MapLibreMap) => {
  const registeredMarkerKeys = new Set<string>()
  const layers = getPoiSymbolLayers(map).map((baseLayer) =>
    usePoiLayerWithRegisteredImages(map, baseLayer, registeredMarkerKeys),
  )

  return {
    layerIds: layers.map((layer) => layer.layerId),
    baseLayerIds: layers.map((layer) => layer.baseLayerId),
  }
}
