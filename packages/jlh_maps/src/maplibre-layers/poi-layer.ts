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

const OMT_POI_ICON_NAME_MATCH_ENTRIES = Object.entries(OMT_POI_SUBCLASS_METADATA).map(
  ([subclass, metadata]) => [subclass, metadata.iconName] as [string, string],
)

const OMT_POI_ICON_NAMES = [
  ...new Set([
    OMT_DEFAULT_POI_METADATA.iconName,
    ...OMT_POI_ICON_NAME_MATCH_ENTRIES.map(([, iconName]) => iconName),
  ]),
]

export enum PoiLayerVariant {
  Normal,
  Environmental,
}

const POI_VARIANT_PROPS = {
  [PoiLayerVariant.Normal]: {
    iconAnchorOverride: undefined,
    markerScale: 0.65,
    font: {
      scale: 1.25,
    },
    'icon-pitch-alignment': undefined,
    useCircularMarker: false,
  },
  [PoiLayerVariant.Environmental]: {
    iconAnchorOverride: 'center',
    markerScale: 0.4,
    font: null,
    'icon-pitch-alignment': 'map',
    useCircularMarker: true,
  },
} as const

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

const makeLayerMarkerOptions = (
  baseLayer: SymbolLayerSpecification,
  props: { useCircularMarker: boolean },
): MarkerOptions => {
  const color = getOriginalLayerIconColor(baseLayer) ?? DEFAULT_PIN_MARKER_ICON_OPTIONS.color

  return {
    ...DEFAULT_PIN_MARKER_ICON_OPTIONS,
    height: props.useCircularMarker
      ? DEFAULT_PIN_MARKER_ICON_OPTIONS.width
      : DEFAULT_PIN_MARKER_ICON_OPTIONS.height,
    headPadding: props.useCircularMarker
      ? DEFAULT_PIN_MARKER_ICON_OPTIONS.headPadding * 0.5
      : DEFAULT_PIN_MARKER_ICON_OPTIONS.headPadding,
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
  variant: PoiLayerVariant,
  additionalMarkerLayerMarkerFields: Partial<Pick<MarkerLayerMarker, 'hoverFeatureStateProperty'>>,
): MarkerLayerSpecification => {
  const layout = baseLayer.layout ?? {}
  const paint = baseLayer.paint ?? {}

  const props = POI_VARIANT_PROPS[variant]

  return {
    ...baseLayer,
    id: `${baseLayer.id}${POI_MARKER_LAYER_SUFFIX}`,
    markerOptions: makeLayerMarkerOptions(baseLayer, props),
    marker: {
      scale: props.markerScale,
      textSize: props.font
        ? (scaleStyleNumber(layout['text-size'], props.font.scale, 16) as number)
        : 1,
      headIconName: makePoiIconNameExpression(),
      iconAnchorOverride: props.iconAnchorOverride,
      ...additionalMarkerLayerMarkerFields,
    },
    layout: {
      ...layout,
      'icon-allow-overlap': layout['icon-allow-overlap'] ?? false,
      'icon-ignore-placement': layout['icon-ignore-placement'] ?? false,
      'text-field': props.font ? layout['text-field'] : '',
      'symbol-sort-key': layout['symbol-sort-key'] ?? ['to-number', ['get', 'rank']],
      'icon-pitch-alignment':
        props['icon-pitch-alignment'] ?? layout['icon-pitch-alignment'] ?? 'auto',
    },
    paint: {
      ...paint,
      'text-color': getUsableCssColor(paint['text-color']) ?? '#1f2937',
      'text-halo-color': paint['text-halo-color'] ?? '#ffffff',
      'text-halo-width': paint['text-halo-width'] ?? 1.5,
    },
  }
}

export type PoiLayerOptions = {
  additionalMarkerLayerMarkerFields?: Partial<Pick<MarkerLayerMarker, 'hoverFeatureStateProperty'>>
} & Pick<UseLayerOptions, 'visible'>

export const usePoiLayer = (
  map: MapLibreMap,
  baseLayer: SymbolLayerSpecification,
  variant: PoiLayerVariant,
  options: PoiLayerOptions = {},
) => {
  const layerId = `${baseLayer.id}${POI_MARKER_LAYER_SUFFIX}`
  const poiMarkerImageProvider = usePoiMarkerImageProvider()

  useMarkerLayer(
    map,
    makePoiMarkerLayer(baseLayer, variant, options.additionalMarkerLayerMarkerFields ?? {}),
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
