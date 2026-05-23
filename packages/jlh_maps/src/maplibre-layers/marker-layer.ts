import type {
  ExpressionSpecification,
  Map as MapLibreMap,
  SymbolLayerSpecification,
} from 'maplibre-gl'
import { createKeyedSharedComposable } from '@/composables/helper.ts'
import {
  getMapHashKey,
  type MapLibreMapImageData,
  useLayer,
  type UseLayerOptions,
  useOnDemandImageProvider,
} from '@/composables/maplibre'
import { svgToImage } from '@/utils/svg-to-image.ts'
import {
  DEFAULT_MARKER_ICON_OPTIONS,
  makeMarkerIcon,
  type MarkerOptions,
} from '@/maplibre-layers/common/marker-icon.ts'

// Icon Image Handling

const makeEmptyPoiMarkerImage = (
  markerIconOptions: Required<MarkerOptions>,
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
  markerOptions: Required<MarkerOptions>
  pixelRatio: number
}

export type UseMarkerImageSourceProviderReturn = ReturnType<typeof useMarkerImageSourceProvider>

export type UseSharedMarkerImageProviderReturn = ReturnType<
  UseMarkerImageSourceProviderReturn['useSharedMarkerImageProvider']
>

const DEFAULT_MARKER_TEXT_COLOR = '#1f2937'

export function useMarkerImageSourceProvider(
  fetchMarkerHeadIcon: (iconName: string) => Promise<string>,
  markerHeadIconNames: string[],
) {
  let nextSharedMarkerImageProviderId = 1
  const useSharedMarkerImageProvider = createKeyedSharedComposable(
    ({ map, markerOptions, pixelRatio }: UseSharedMarkerImageProviderParams) =>
      [getMapHashKey(map), JSON.stringify([markerOptions, pixelRatio])].join(':'),
    ({ map, markerOptions, pixelRatio }: UseSharedMarkerImageProviderParams) => {
      const composableId = nextSharedMarkerImageProviderId++
      const imageIdPrefix = `poi-icon-${composableId}-`
      const getImageId = (iconName: string) => `${imageIdPrefix}${iconName}`
      const getIconNameForImageId = (imageId: string) =>
        imageId.startsWith(imageIdPrefix) ? imageId.slice(imageIdPrefix.length) : undefined

      useOnDemandImageProvider(map, {
        getParamsForImageId: (imageId) => {
          const iconName = getIconNameForImageId(imageId)
          if (!iconName) return null

          return {
            iconName,
            markerIconOptions: markerOptions,
            pixelRatio,
          }
        },
        getInitialImage: ({ markerIconOptions, pixelRatio }) => ({
          image: makeEmptyPoiMarkerImage(markerIconOptions, pixelRatio),
          options: {
            pixelRatio,
          },
        }),
        fetchImage: async ({ iconName, markerIconOptions, pixelRatio }) => {
          const markerSvg = makeMarkerIcon(await fetchMarkerHeadIcon(iconName), markerIconOptions)

          const image = await svgToImage(markerSvg, {
            width: markerIconOptions.width,
            height: markerIconOptions.height,
            pixelRatio,
          })

          return { image }
        },
        onImageAdded: (image) => {
          if (image instanceof ImageBitmap) image.close()
        },
      })

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
      'icon-anchor': 'bottom',
      'icon-offset': [0, 0],
      'text-anchor': 'top',
      'text-offset': [0, 0.55],
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
  | 'icon-size'
  | 'icon-image'
  | 'text-size'
>
export type MarkerLayerMarker = {
  scale: number
  textSize: number
  headIconName: ExpressionSpecification
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
  const markerLayerSpecificationWithDefaults = {
    ...markerLayerSpecification,
    markerOptions: {
      ...DEFAULT_MARKER_ICON_OPTIONS,
      ...markerLayerSpecification.markerOptions,
    },
  }

  const sharedMarkerImageProvider = markerImageSourceProvider.useSharedMarkerImageProvider({
    map,
    markerOptions: markerLayerSpecificationWithDefaults.markerOptions,
    pixelRatio: 2.0,
  })

  useLayer(
    map,
    makeSymbolLayerForMarkerLayer(markerLayerSpecificationWithDefaults, sharedMarkerImageProvider),
    options,
  )
}
