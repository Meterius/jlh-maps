import type { ExpressionSpecification, LayerSpecification, Map as MapLibreMap } from 'maplibre-gl'
import {
  OMT_DEFAULT_POI_METADATA,
  OMT_POI_SUBCLASS_METADATA,
  resolveOmtPoiIconSvg,
} from '@/constants/omt-mapping.ts'
import { createKeyedSharedComposable } from '@/composables/helper.ts'
import {
  getMapHashKey,
  type MapLibreMapImageData,
  useLayer,
  useOnDemandImageProvider,
} from '@/composables/maplibre'
import { getUsableCssColor } from '@/utils/css-color.ts'
import { makeStringPropertyMatchExpression, scaleStyleNumber } from '@/utils/maplibre.ts'
import { svgToImage } from '@/utils/svg-to-image.ts'
import {
  DEFAULT_MARKER_ICON_OPTIONS,
  makeMarkerIcon,
  type MarkerIconOptions,
} from '@/maplibre-layers/common/marker-icon.ts'

type SymbolLayerSpecification = Extract<LayerSpecification, { type: 'symbol' }>
type SymbolLayerLayout = NonNullable<SymbolLayerSpecification['layout']>
type SymbolLayerPaint = NonNullable<SymbolLayerSpecification['paint']>
type PoiMarkerImageParams = {
  iconName: string
  markerIconOptions: Required<MarkerIconOptions>
  pixelRatio: number
}

type PoiMarkerLayerOptions = {
  markerIconOptions: Required<MarkerIconOptions>
  markerScale: number
  fontScale: number
  pixelRatio: number
}

type PoiMarkerImageProviderParams = {
  map: MapLibreMap
  markerIconOptions: Required<MarkerIconOptions>
  pixelRatio: number
}

type PoiMarkerImageProvider = {
  getImageId: (iconName: string) => string
}

const POI_MARKER_LAYER_SUFFIX = '-poi-marker'

const DEFAULT_MARKER_LAYER_OPTIONS: PoiMarkerLayerOptions = {
  markerIconOptions: DEFAULT_MARKER_ICON_OPTIONS,
  markerScale: 1.25,
  fontScale: 1.25,
  pixelRatio: 2,
}

// Icon Image Handling

const loadPoiMarkerImage = async (
  iconName: string,
  markerIconOptions: Required<MarkerIconOptions>,
  pixelRatio: number,
) => {
  const markerSvg = makeMarkerIcon(await resolveOmtPoiIconSvg(iconName), markerIconOptions)

  return svgToImage(markerSvg, {
    width: markerIconOptions.width,
    height: markerIconOptions.height,
    pixelRatio,
  })
}

const makeEmptyPoiMarkerImage = (
  markerIconOptions: Required<MarkerIconOptions>,
  pixelRatio: number,
): MapLibreMapImageData => {
  const width = Math.round(markerIconOptions.width * pixelRatio)
  const height = Math.round(markerIconOptions.height * pixelRatio)

  return {
    width,
    height,
    data: new Uint8ClampedArray(width * height * 4),
  }
}

let nextPoiMarkerImageProviderId = 1

const useSharedPoiMarkerImageProvider = createKeyedSharedComposable(
  ({ map, markerIconOptions, pixelRatio }: PoiMarkerImageProviderParams) =>
    [getMapHashKey(map), JSON.stringify([markerIconOptions, pixelRatio])].join(':'),
  ({
    map,
    markerIconOptions,
    pixelRatio,
  }: PoiMarkerImageProviderParams): PoiMarkerImageProvider => {
    const composableId = nextPoiMarkerImageProviderId++
    const imageIdPrefix = `poi-icon-${composableId}-`
    const getImageId = (iconName: string) => `${imageIdPrefix}${iconName}`
    const getIconNameForImageId = (imageId: string) =>
      imageId.startsWith(imageIdPrefix) ? imageId.slice(imageIdPrefix.length) : undefined

    useOnDemandImageProvider<PoiMarkerImageParams>(map, {
      getParamsForImageId: (imageId) => {
        const iconName = getIconNameForImageId(imageId)
        if (!iconName) return null

        return {
          iconName,
          markerIconOptions,
          pixelRatio,
        }
      },
      getInitialImage: ({ markerIconOptions, pixelRatio }) => ({
        image: makeEmptyPoiMarkerImage(markerIconOptions, pixelRatio),
        options: {
          pixelRatio,
        },
      }),
      fetchImage: async ({ iconName, markerIconOptions, pixelRatio }) => ({
        image: await loadPoiMarkerImage(iconName, markerIconOptions, pixelRatio),
      }),
      onImageAdded: (image) => {
        if (image instanceof ImageBitmap) image.close()
      },
    })

    return {
      getImageId,
    }
  },
)

// Layer Construction

const getOriginalLayerIconColor = (baseLayer: SymbolLayerSpecification) => {
  const paint = (baseLayer.paint ?? {}) as SymbolLayerPaint

  return getUsableCssColor(paint['icon-color']) ?? getUsableCssColor(paint['text-color'])
}

const makeLayerMarkerOptions = (
  baseLayer: SymbolLayerSpecification,
  marker: PoiMarkerLayerOptions,
): PoiMarkerLayerOptions => {
  const color =
    getOriginalLayerIconColor(baseLayer) ??
    getUsableCssColor(marker.markerIconOptions.color) ??
    DEFAULT_MARKER_ICON_OPTIONS.color

  return {
    ...marker,
    markerIconOptions: {
      ...marker.markerIconOptions,
      color,
      iconColor: color,
    },
  }
}

const makePropertyIconMatchExpression = (
  property: 'class' | 'subclass',
  imageProvider: PoiMarkerImageProvider,
  fallback: string | ExpressionSpecification,
) =>
  makeStringPropertyMatchExpression(
    property,
    Object.entries(OMT_POI_SUBCLASS_METADATA).map(
      ([subclass, metadata]) =>
        [subclass, imageProvider.getImageId(metadata.iconName)] as [string, string],
    ),
    fallback,
  )

const makePoiIconImageExpression = (imageProvider: PoiMarkerImageProvider) =>
  makePropertyIconMatchExpression(
    'subclass',
    imageProvider,
    makePropertyIconMatchExpression(
      'class',
      imageProvider,
      imageProvider.getImageId(OMT_DEFAULT_POI_METADATA.iconName),
    ),
  )

const makePoiMarkerLayer = (
  baseLayer: SymbolLayerSpecification,
  marker: PoiMarkerLayerOptions,
  imageProvider: PoiMarkerImageProvider,
): SymbolLayerSpecification => {
  const layout = (baseLayer.layout ?? {}) as SymbolLayerLayout
  const paint = (baseLayer.paint ?? {}) as SymbolLayerPaint

  return {
    ...baseLayer,
    id: `${baseLayer.id}${POI_MARKER_LAYER_SUFFIX}`,
    layout: {
      ...layout,
      'icon-image': makePoiIconImageExpression(imageProvider),
      'icon-size': marker.markerScale,
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

export const usePoiLayer = (map: MapLibreMap, baseLayer: SymbolLayerSpecification) => {
  const layerMarker = makeLayerMarkerOptions(baseLayer, DEFAULT_MARKER_LAYER_OPTIONS)

  const imageProvider = useSharedPoiMarkerImageProvider({
    map,
    markerIconOptions: layerMarker.markerIconOptions,
    pixelRatio: layerMarker.pixelRatio,
  })

  const layerId = `${baseLayer.id}${POI_MARKER_LAYER_SUFFIX}`

  useLayer(map, makePoiMarkerLayer(baseLayer, layerMarker, imageProvider), {
    beforeId: baseLayer.id,
  })

  return {
    layerId,
    baseLayerId: baseLayer.id,
  }
}
