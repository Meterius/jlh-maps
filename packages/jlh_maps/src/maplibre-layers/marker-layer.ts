import type {
  ExpressionSpecification,
  Map as MapLibreMap,
  SymbolLayerSpecification,
} from 'maplibre-gl'
import { createKeyedSharedComposable } from '@/composables/helper.ts'
import {
  getMapHashKey,
  type MapLibreMapImageData,
  useImage,
  type UseImageOptions,
  useLayer,
  type UseLayerOptions,
  useOnDemandImageProvider,
} from '@/composables/maplibre'
import { svgToImage } from '@/utils/svg-to-image.ts'
import {
  makeMarkerIcon,
  type MarkerOptions,
  MarkerShape,
} from '@/maplibre-layers/common/marker-icon.ts'

// Icon Image Handling

const makeEmptyPoiMarkerImage = (
  markerIconOptions: MarkerOptions,
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

type UseSharedMarkerImageProviderParams = {
  map: MapLibreMap
  markerOptions: MarkerOptions
  pixelRatio: number
}

export type UseMarkerImageSourceProviderReturn = ReturnType<typeof useMarkerImageSourceProvider>

export type UseSharedMarkerImageProviderReturn = ReturnType<
  UseMarkerImageSourceProviderReturn['useSharedMarkerImageProvider']
>

const DEFAULT_MARKER_TEXT_COLOR = '#1f2937'
const MIN_MARKER_IMAGE_PIXEL_RATIO = 2
const MAX_MARKER_IMAGE_PIXEL_RATIO = 4
let nextSharedMarkerImageProviderId = 1

export function useMarkerImageSourceProvider(
  fetchMarkerHeadIcon: (iconName: string) => Promise<string>,
  markerHeadIconNames: string[],
  options?: { prefetch: boolean },
) {
  const useSharedMarkerImageProvider = createKeyedSharedComposable(
    ({ map, markerOptions, pixelRatio }: UseSharedMarkerImageProviderParams) =>
      [getMapHashKey(map), JSON.stringify([markerOptions, pixelRatio])].join(':'),
    ({ map, markerOptions, pixelRatio }: UseSharedMarkerImageProviderParams) => {
      const composableId = nextSharedMarkerImageProviderId++
      const imageIdPrefix = `poi-icon-${composableId}-`
      const getImageId = (iconName: string) => `${imageIdPrefix}${iconName}`
      const getIconNameForImageId = (imageId: string) =>
        imageId.startsWith(imageIdPrefix) ? imageId.slice(imageIdPrefix.length) : undefined

      const fetchImage = async (iconName: string) => {
        const markerSvg = makeMarkerIcon(await fetchMarkerHeadIcon(iconName), markerOptions)
        return svgToImage(markerSvg, {
          width: markerOptions.width,
          height: markerOptions.height,
          pixelRatio,
        })
      }

      const imageProviderOptions: UseImageOptions = {
        options: {
          pixelRatio,
        },
        onImageAdded: (image) => {
          if (image instanceof ImageBitmap) image.close()
        },
      }

      if (options?.prefetch) {
        markerHeadIconNames.forEach((iconName) => {
          useImage(map, getImageId(iconName), fetchImage(iconName), imageProviderOptions)
        })
      } else {
        useOnDemandImageProvider(map, {
          getParamsForImageId: (imageId) => {
            const iconName = getIconNameForImageId(imageId)
            if (!iconName) return null
            return { iconName }
          },
          getInitialImage: () => ({
            image: makeEmptyPoiMarkerImage(markerOptions, pixelRatio),
            options: imageProviderOptions.options,
          }),
          fetchImage: async ({ iconName }) => {
            return { image: await fetchImage(iconName) }
          },
          onImageAdded: imageProviderOptions.onImageAdded,
        })
      }

      const markerHeadIconNameToImageIdFlatEntries = markerHeadIconNames.flatMap((iconName) => [
        iconName,
        getImageId(iconName),
      ])

      return {
        makeImageIdFromIconNameExpression: (
          iconNameExpression: ExpressionSpecification,
        ): ExpressionSpecification =>
          [
            'match',
            iconNameExpression,
            ...markerHeadIconNameToImageIdFlatEntries,
            '',
          ] as ExpressionSpecification,
      }
    },
  )

  return {
    useSharedMarkerImageProvider,
  }
}

// Layer Construction

type SymbolLayerLayout = NonNullable<SymbolLayerSpecification['layout']>
type MarkerLayerNumberValue = number | ExpressionSpecification

const getDefaultMarkerImagePixelRatio = () => {
  const devicePixelRatio =
    typeof window === 'undefined' ? MIN_MARKER_IMAGE_PIXEL_RATIO : window.devicePixelRatio

  return Math.min(
    MAX_MARKER_IMAGE_PIXEL_RATIO,
    Math.max(MIN_MARKER_IMAGE_PIXEL_RATIO, Math.ceil(devicePixelRatio)),
  )
}

const getMarkerIconAnchor = (markerOptions: MarkerOptions): SymbolLayerLayout['icon-anchor'] =>
  markerOptions.shape === MarkerShape.Pin ? 'bottom' : 'bottom'

const getMarkerTextOffset = (
  markerOptions: MarkerOptions,
): SymbolLayerLayout['text-offset'] => {
  if (markerOptions.shape === MarkerShape.Pin) return [0, 0.4]
  else return [0, 0.2]
}

const makeSymbolLayerForMarkerLayer = (
  markerLayerSpecification: MarkerLayerSpecification,
  sharedMarkerImageProvider: UseSharedMarkerImageProviderReturn,
): SymbolLayerSpecification => {
  const layout = markerLayerSpecification.layout ?? {}
  const paint = (markerLayerSpecification.paint ?? {}) as MarkerLayerPaint
  const hoverFeatureStateProperty = markerLayerSpecification.marker.hoverFeatureStateProperty

  return {
    ...markerLayerSpecification,
    layout: {
      ...layout,
      'icon-image': sharedMarkerImageProvider.makeImageIdFromIconNameExpression(
        markerLayerSpecification.marker.headIconName,
      ),
      'icon-size': markerLayerSpecification.marker.scale,
      'icon-anchor': getMarkerIconAnchor(markerLayerSpecification.markerOptions),
      'icon-offset': [0, 0],
      'text-anchor': 'top',
      'text-offset': getMarkerTextOffset(
        markerLayerSpecification.markerOptions,
      ),
      'text-size': markerLayerSpecification.marker.textSize,
      'text-optional': false,
      'icon-optional': false,
    },
    paint: {
      ...paint,
      ...(hoverFeatureStateProperty
        ? {
            'text-color': [
              'case',
              ['boolean', ['feature-state', hoverFeatureStateProperty], false],
              markerLayerSpecification.marker.hoverTextColor ?? [
                'interpolate',
                ['linear'],
                0.4,
                0.0,
                paint['text-color'] ?? DEFAULT_MARKER_TEXT_COLOR,
                1.0,
                '#000000',
              ],
              paint['text-color'] ?? DEFAULT_MARKER_TEXT_COLOR,
            ] as ExpressionSpecification,
          }
        : {}),
    },
  }
}

type MarkerLayerPaint = Omit<NonNullable<SymbolLayerSpecification['paint']>, 'icon-color'>
type MarkerLayerLayout = Omit<
  NonNullable<SymbolLayerSpecification['layout']>,
  | 'text-optional'
  | 'icon-optional'
  | 'text-offset'
  | 'text-anchor'
  | 'icon-anchor'
  | 'icon-offset'
  | 'icon-size'
  | 'icon-image'
  | 'text-size'
>

export type MarkerLayerMarker = {
  scale: MarkerLayerNumberValue
  textSize: MarkerLayerNumberValue
  headIconName: ExpressionSpecification
  imagePixelRatio?: number
  hoverFeatureStateProperty?: string
  hoverTextColor?: ExpressionSpecification
}

export type MarkerLayerSpecification = Omit<SymbolLayerSpecification, 'layout' | 'paint'> & {
  paint: MarkerLayerPaint
} & {
  layout: MarkerLayerLayout
} & {
  markerOptions: MarkerOptions
  marker: MarkerLayerMarker
}

export const useMarkerLayer = (
  map: MapLibreMap,
  markerLayerSpecification: MarkerLayerSpecification,
  markerImageSourceProvider: UseMarkerImageSourceProviderReturn,
  options?: UseLayerOptions,
) => {
  const sharedMarkerImageProvider = markerImageSourceProvider.useSharedMarkerImageProvider({
    map,
    markerOptions: markerLayerSpecification.markerOptions,
    pixelRatio:
      markerLayerSpecification.marker.imagePixelRatio ?? getDefaultMarkerImagePixelRatio(),
  })

  useLayer(
    map,
    makeSymbolLayerForMarkerLayer(markerLayerSpecification, sharedMarkerImageProvider),
    options,
  )
}
