import type {
  ExpressionSpecification,
  LayerSpecification,
  Map as MapLibreMap,
  MapStyleImageMissingEvent,
} from 'maplibre-gl'
import { OMT_DEFAULT_POI_METADATA, OMT_POI_SUBCLASS_METADATA } from '@/constants/omt-mapping.ts'
import { resolvePoiIconSvg, type PoiDisplayMetadata } from '@/constants/osm-mapping.ts'
import { onScopeDisposeLifo } from '@/composables/helper.ts'
import { useLayer } from '@/composables/maplibre'
import { getUsableCssColor } from '@/utils/css-color.ts'
import { makeStringPropertyMatchExpression, scaleStyleNumber } from '@/utils/maplibre.ts'
import {
  escapeSvgAttribute,
  getSvgPresentationAttributes,
  parseSvgElement,
  SVG_NAMESPACE,
} from '@/utils/svg.ts'
import { svgToImage, type SvgRasterImage } from '@/utils/svg-to-image.ts'

type SymbolLayerSpecification = Extract<LayerSpecification, { type: 'symbol' }>
type SymbolLayerLayout = NonNullable<SymbolLayerSpecification['layout']>
type SymbolLayerPaint = NonNullable<SymbolLayerSpecification['paint']>
type ParsedSvgContent = {
  innerHtml: string
  presentationAttributes: string
  viewBox: string
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
const DEFAULT_SVG_PRESENTATION =
  'fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"'

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
const parsedPoiIconContentCache = new Map<string, Promise<ParsedSvgContent>>()
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

const parseSvgContent = (source: string | undefined): ParsedSvgContent => {
  if (!source) {
    return {
      innerHtml: '',
      presentationAttributes: DEFAULT_SVG_PRESENTATION,
      viewBox: '0 0 24 24',
    }
  }

  const svg = parseSvgElement(source)
  if (!svg) {
    return {
      innerHtml: '',
      presentationAttributes: DEFAULT_SVG_PRESENTATION,
      viewBox: '0 0 24 24',
    }
  }

  return {
    innerHtml: svg.innerHTML,
    presentationAttributes: getSvgPresentationAttributes(svg),
    viewBox: svg.getAttribute('viewBox') ?? '0 0 24 24',
  }
}

const getParsedPoiIconContent = (metadata: PoiDisplayMetadata) => {
  const cached = parsedPoiIconContentCache.get(metadata.iconName)
  if (cached) return cached

  const parsedContent = (async () => {
    const iconSvg =
      (await resolvePoiIconSvg(metadata.iconSvg)) ??
      (await resolvePoiIconSvg(OMT_DEFAULT_POI_METADATA.iconSvg))

    return parseSvgContent(iconSvg)
  })()

  parsedPoiIconContentCache.set(metadata.iconName, parsedContent)
  parsedContent.catch(() => parsedPoiIconContentCache.delete(metadata.iconName))

  return parsedContent
}

const formatSvgNumber = (value: number) => Number(value.toFixed(3)).toString()

const getMarkerIconBounds = (marker: Required<PoiMarkerOptions>) => {
  const iconSize = 14 * marker.iconScale
  const iconPosition = 16 - iconSize / 2

  return {
    size: formatSvgNumber(iconSize),
    x: formatSvgNumber(iconPosition),
    y: formatSvgNumber(iconPosition),
  }
}

const buildPoiMarkerSvg = (
  { innerHtml, presentationAttributes, viewBox }: ParsedSvgContent,
  marker: Required<PoiMarkerOptions>,
) => {
  const iconBounds = getMarkerIconBounds(marker)
  const markerPath =
    'M16 34C15.25 34 14.55 33.68 13.95 33.05C11.3 30.3 4 22.65 4 15.8C4 8.85 9.37 3.5 16 3.5C22.63 3.5 28 8.85 28 15.8C28 22.65 20.7 30.3 18.05 33.05C17.45 33.68 16.75 34 16 34Z'

  return `
<svg xmlns="${SVG_NAMESPACE}" width="${marker.width}" height="${marker.height}" viewBox="0 0 32 36">
  <ellipse cx="16" cy="34.5" rx="7" ry="1.5" fill="${escapeSvgAttribute(marker.shadowColor)}"/>
  <path d="${markerPath}" fill="${escapeSvgAttribute(marker.color)}"/>
  <path d="${markerPath}" fill="none" stroke="${escapeSvgAttribute(marker.outlineColor)}" stroke-width="1"/>
  <circle cx="16" cy="16" r="9" fill="${escapeSvgAttribute(marker.headColor)}"/>
  <svg x="${iconBounds.x}" y="${iconBounds.y}" width="${iconBounds.size}" height="${iconBounds.size}" viewBox="${escapeSvgAttribute(viewBox)}" color="${escapeSvgAttribute(marker.iconColor)}" ${presentationAttributes}>
    ${innerHtml}
  </svg>
</svg>`.trim()
}

const loadPoiMarkerImage = async (
  metadata: PoiDisplayMetadata,
  marker: Required<PoiMarkerOptions>,
) => {
  const markerSvg = buildPoiMarkerSvg(await getParsedPoiIconContent(metadata), marker)

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
  const addedImageIds = new Set<string>()
  let disposed = false

  const handleStyleImageMissing = (event: MapStyleImageMissingEvent) => {
    const metadata = metadataByImageId.get(event.id)
    if (!metadata || disposed || map.hasImage(event.id)) return

    map.addImage(event.id, makeEmptyPoiMarkerImage(marker), {
      pixelRatio: marker.pixelRatio,
    })
    addedImageIds.add(event.id)

    getCachedPoiMarkerImage(event.id, metadata, marker).then((image) => {
      if (disposed || !map.hasImage(event.id)) return

      map.updateImage(event.id, image)
      map.triggerRepaint()
    }, console.error)
  }

  map.on('styleimagemissing', handleStyleImageMissing)

  onScopeDisposeLifo(() => {
    disposed = true
    map.off('styleimagemissing', handleStyleImageMissing)

    addedImageIds.forEach((imageId) => {
      if (map.hasImage(imageId)) {
        map.removeImage(imageId)
      }
    })
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
