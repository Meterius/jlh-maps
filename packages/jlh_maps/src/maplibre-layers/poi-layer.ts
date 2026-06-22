import type {
  ExpressionSpecification,
  Map as MapLibreMap,
  SymbolLayerSpecification,
} from 'maplibre-gl'
import { createSharedComposable } from '@vueuse/core'
import {
  OMT_DEFAULT_POI_METADATA,
  OMT_POI_SUBCLASS_METADATA,
  resolveOmtPoiIconSvg,
} from '@/constants/omt-mapping.ts'
import { getUsableCssColor } from '@/utils/css-color.ts'
import { makeStringPropertyMatchExpression, scaleStyleNumber } from '@/utils/maplibre.ts'
import {
  DEFAULT_PIN_MARKER_ICON_OPTIONS,
  type MarkerOptions,
} from '@/maplibre-layers/common/marker-icon.ts'
import type { UseLayerOptions } from '@/composables/maplibre'
import {
  type MarkerLayerMarker,
  type MarkerLayerSpecification,
  useMarkerImageSourceProvider,
  useMarkerLayer,
} from '@/maplibre-layers/marker-layer.ts'

const POI_MARKER_LAYER_SUFFIX = '-poi-marker'
const POI_MARKER_SCALE = 0.75
const POI_FONT_SCALE = 1.25

const OMT_POI_ICON_NAME_MATCH_ENTRIES = Object.entries(OMT_POI_SUBCLASS_METADATA).map(
  ([subclass, metadata]) => [subclass, metadata.iconName] as [string, string],
)

const OMT_POI_ICON_NAMES = [
  ...new Set([
    OMT_DEFAULT_POI_METADATA.iconName,
    ...OMT_POI_ICON_NAME_MATCH_ENTRIES.map(([, iconName]) => iconName),
  ]),
]

export const usePoiMarkerImageProvider = createSharedComposable(() =>
  useMarkerImageSourceProvider(
    async (iconName) => (await resolveOmtPoiIconSvg(iconName)) ?? '',
    OMT_POI_ICON_NAMES,
  ),
)

// Layer Construction

const getOriginalLayerIconColor = (baseLayer: SymbolLayerSpecification) => {
  const paint = baseLayer.paint ?? {}
  return getUsableCssColor(paint['icon-color']) ?? getUsableCssColor(paint['text-color'])
}

const makeLayerMarkerOptions = (baseLayer: SymbolLayerSpecification): MarkerOptions => {
  const color = getOriginalLayerIconColor(baseLayer) ?? DEFAULT_PIN_MARKER_ICON_OPTIONS.color

  return {
    ...DEFAULT_PIN_MARKER_ICON_OPTIONS,
    color,
    iconColor: color,
  }
}

const makePropertyIconMatchExpression = (
  property: 'class' | 'subclass',
  fallback: string | ExpressionSpecification,
) => makeStringPropertyMatchExpression(property, OMT_POI_ICON_NAME_MATCH_ENTRIES, fallback)

const makePoiIconNameExpression = () =>
  makePropertyIconMatchExpression(
    'subclass',
    makePropertyIconMatchExpression('class', OMT_DEFAULT_POI_METADATA.iconName),
  )

const makePoiMarkerLayer = (
  baseLayer: SymbolLayerSpecification,
  additionalMarkerLayerMarkerFields: Partial<Pick<MarkerLayerMarker, 'hoverFeatureStateProperty'>>,
): MarkerLayerSpecification => {
  const layout = baseLayer.layout ?? {}
  const paint = baseLayer.paint ?? {}

  return {
    ...baseLayer,
    id: `${baseLayer.id}${POI_MARKER_LAYER_SUFFIX}`,
    markerOptions: makeLayerMarkerOptions(baseLayer),
    marker: {
      scale: POI_MARKER_SCALE,
      textSize: scaleStyleNumber(layout['text-size'], POI_FONT_SCALE, 16) as number,
      headIconName: makePoiIconNameExpression(),
      ...additionalMarkerLayerMarkerFields,
    },
    layout: {
      ...layout,
      'icon-allow-overlap': layout['icon-allow-overlap'] ?? false,
      'icon-ignore-placement': layout['icon-ignore-placement'] ?? false,
      'text-field': layout['text-field'],
      'symbol-sort-key': layout['symbol-sort-key'] ?? ['to-number', ['get', 'rank']],
    },
    paint: {
      ...paint,
      'text-color': getUsableCssColor(paint['text-color']) ?? '#1f2937',
      'text-halo-color': paint['text-halo-color'] ?? '#ffffff',
      'text-halo-width': paint['text-halo-width'] ?? 1.5,
    },
  }
}

export const usePoiLayer = (
  map: MapLibreMap,
  baseLayer: SymbolLayerSpecification,
  additionalMarkerLayerMarkerFields?: Partial<Pick<MarkerLayerMarker, 'hoverFeatureStateProperty'>>,
  options: Pick<UseLayerOptions, 'visible'> = {},
) => {
  const layerId = `${baseLayer.id}${POI_MARKER_LAYER_SUFFIX}`
  const poiMarkerImageProvider = usePoiMarkerImageProvider()

  useMarkerLayer(
    map,
    makePoiMarkerLayer(baseLayer, additionalMarkerLayerMarkerFields ?? {}),
    poiMarkerImageProvider,
    {
      ...options,
      beforeId: baseLayer.id,
    },
  )

  return {
    layerId,
    baseLayerId: baseLayer.id,
  }
}
