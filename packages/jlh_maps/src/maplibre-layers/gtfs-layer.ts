import type {
  ExpressionSpecification,
  GeoJSONFeature,
  Map as MapLibreMap,
  SymbolLayerSpecification,
} from 'maplibre-gl'
import { useLayer, type UseLayerOptions, useSource } from '@/composables/maplibre'
import { TILESERVER_GTFS_PMTILES_URL } from '@/external/endpoints.ts'
import { loadLucideIconSvg } from '@/utils/lucide-icon-svg.ts'
import {
  type UseLucideIconImageSourceProviderReturn,
  useLucideIconImageSourceProvider,
} from '@/maplibre-layers/common/lucide-icon-image-source-provider.ts'
import {
  type MarkerLayerMarker,
  type MarkerLayerSpecification,
  useMarkerImageSourceProvider,
  useMarkerLayer,
} from '@/maplibre-layers/marker-layer.ts'
import { DEFAULT_BOX_MARKER_ICON_OPTIONS } from '@/maplibre-layers/common/marker-icon.ts'

export const GTFS_SOURCE_ID = 'gtfs'

export const GTFS_SOURCE_LAYER_ID = 'stops'

const GTFS_MIN_ZOOM = 10
const GTFS_SYMBOL_ICON_PIXEL_RATIO = 2
const GTFS_MARKER_ICON_PIXEL_RATIO = 4
const GTFS_ROOT_STOP_MARKER_COLOR = 'rgb(14 116 144)'

const GTFS_ROOT_STOP_MARKER_ICON_NAMES = [
  'route',
  'tram-front',
  'train-front',
  'bus-front',
  'ship',
  'cable-car',
  'mountain',
  'plane',
  'car-taxi-front',
]

enum GtfsStopLocationType {
  StopOrPlatform = 0,
  Station = 1,
  EntranceExit = 2,
  GenericNode = 3,
  BoardingArea = 4,
}

enum GtfsIconName {
  BoardingArea = 'circle-dot',
  EntranceExit = 'door-open',
  GenericNode = 'waypoints',
  Platform = 'circle-dot',
  Station = 'landmark',
  Stop = 'signpost',
  Unknown = 'map-pin',
}

export enum GtfsRouteType {
  Tram = '0',
  Subway = '1',
  Rail = '2',
  Bus = '3',
  Ferry = '4',
  CableTram = '5',
  AerialLift = '6',
  Funicular = '7',
  Trolleybus = '11',
  Monorail = '12',
  Coach = '200',
  Air = '1100',
  Taxi = '1500',
}

export enum GtfsRouteIconName {
  Air = 'plane',
  Bus = 'bus-front',
  Cable = 'cable-car',
  Ferry = 'ship',
  Funicular = 'mountain',
  Generic = 'route',
  Rail = 'train-front',
  Taxi = 'car-taxi-front',
  Tram = 'tram-front',
}

export const GTFS_ROUTE_TYPE_ICON_MAP: Record<GtfsRouteType, GtfsRouteIconName> = {
  [GtfsRouteType.Tram]: GtfsRouteIconName.Tram,
  [GtfsRouteType.Subway]: GtfsRouteIconName.Rail,
  [GtfsRouteType.Rail]: GtfsRouteIconName.Rail,
  [GtfsRouteType.Bus]: GtfsRouteIconName.Bus,
  [GtfsRouteType.Ferry]: GtfsRouteIconName.Ferry,
  [GtfsRouteType.CableTram]: GtfsRouteIconName.Cable,
  [GtfsRouteType.AerialLift]: GtfsRouteIconName.Cable,
  [GtfsRouteType.Funicular]: GtfsRouteIconName.Funicular,
  [GtfsRouteType.Trolleybus]: GtfsRouteIconName.Bus,
  [GtfsRouteType.Monorail]: GtfsRouteIconName.Rail,
  [GtfsRouteType.Coach]: GtfsRouteIconName.Bus,
  [GtfsRouteType.Air]: GtfsRouteIconName.Air,
  [GtfsRouteType.Taxi]: GtfsRouteIconName.Taxi,
}

enum GtfsStopField {
  LocationType = 'location_type',
  ParentStationId = 'parent_station_id',
  PlatformCode = 'platform_code',
  RouteTypes = 'route_types',
  StopCode = 'stop_code',
  StopName = 'stop_name',
  VersionId = 'version_id',
  StopId = 'stop_id',
}

enum GtfsLayerId {
  RootStopMarkers = 'gtfs-root-stop-markers',
  HintSymbols = 'gtfs-hint-symbols',
}

export function useGtfsLayer(
  map: MapLibreMap,
  additionalMarkerLayerMarkerFields?: Partial<Pick<MarkerLayerMarker, 'hoverFeatureStateProperty'>>,
  options: UseLayerOptions = {},
) {
  useSource(map, GTFS_SOURCE_ID, {
    type: 'vector',
    url: `pmtiles://${TILESERVER_GTFS_PMTILES_URL.toString()}`,
    minzoom: GTFS_MIN_ZOOM,
    attribution: 'GTFS schedule feeds',
  })

  const lucideIconImageSourceProvider = useLucideIconImageSourceProvider({
    map,
    pixelRatio: GTFS_SYMBOL_ICON_PIXEL_RATIO,
  })

  useLayer(map, makeGtfsHintSymbolLayer(lucideIconImageSourceProvider), options)

  const markerImageSourceProvider = useMarkerImageSourceProvider(
    async (iconName) => loadLucideIconSvg(iconName) ?? '',
    GTFS_ROOT_STOP_MARKER_ICON_NAMES,
  )

  const rootStopMarkerLayerSpecification = makeGtfsRootStopMarkerLayer()
  useMarkerLayer(
    map,
    {
      ...rootStopMarkerLayerSpecification,
      marker: {
        ...rootStopMarkerLayerSpecification.marker,
        ...additionalMarkerLayerMarkerFields,
      },
    },
    markerImageSourceProvider,
    options,
  )

  return {
    stopMarkerLayer: {
      layerId: rootStopMarkerLayerSpecification.id,
      getInfoFromFeature: (feature: GeoJSONFeature) => {
        const versionId = feature.properties?.[GtfsStopField.VersionId]
        const stopId = feature.properties?.[GtfsStopField.StopId]
        const stopName = feature.properties?.[GtfsStopField.StopName]

        return {
          label: typeof stopName === 'string' ? stopName : '',
          stopRef:
            typeof versionId === 'number' && typeof stopId === 'string'
              ? {
                  versionId,
                  stopId,
                }
              : undefined,
        }
      },
    },
    layerIds: [rootStopMarkerLayerSpecification.id, GtfsLayerId.HintSymbols],
  }
}

function makeGtfsRootStopMarkerLayer(): MarkerLayerSpecification {
  const baseScale = makeMatchExpression(
    ['coalesce', ['get', GtfsStopField.RouteTypes], ''],
    {
      [GtfsRouteType.Subway]: 0.4,
      [GtfsRouteType.Rail]: 0.4,
    },
    0.3,
  )

  return {
    id: GtfsLayerId.RootStopMarkers,
    type: 'symbol',
    source: GTFS_SOURCE_ID,
    'source-layer': GTFS_SOURCE_LAYER_ID,
    minzoom: 15,
    filter: makeRootStopFilter(),
    markerOptions: {
      ...DEFAULT_BOX_MARKER_ICON_OPTIONS,
      color: GTFS_ROOT_STOP_MARKER_COLOR,
      iconColor: GTFS_ROOT_STOP_MARKER_COLOR,
    },
    marker: {
      scale: ['interpolate', ['linear'], ['zoom'], 15, baseScale, 20, ['*', baseScale, 3]],
      textSize: ['interpolate', ['linear'], ['zoom'], 15, 10.5, 16, 12, 18, 13],
      headIconName: makeMatchExpression(
        ['coalesce', ['get', GtfsStopField.RouteTypes], ''],
        GTFS_ROUTE_TYPE_ICON_MAP,
        GtfsRouteIconName.Generic,
      ),
      imagePixelRatio: GTFS_MARKER_ICON_PIXEL_RATIO,
    },
    layout: {
      'icon-allow-overlap': false,
      'icon-ignore-placement': false,
      'text-allow-overlap': false,
      'text-ignore-placement': false,
      'text-field': ['get', GtfsStopField.StopName],
      'text-font': ['Open Sans Semibold', 'Noto Sans Bold'],
      'symbol-sort-key': [
        'case',
        ['==', makeLocationTypeExpression(), GtfsStopLocationType.Station],
        1,
        2,
      ] as ExpressionSpecification,
    },
    paint: {
      'text-color': GTFS_ROOT_STOP_MARKER_COLOR,
      'text-halo-color': 'rgb(255 255 255)',
      'text-halo-width': 1.25,
      'icon-opacity': ['interpolate', ['linear'], ['zoom'], 14, 0.85, 15, 1],
      'text-opacity': ['interpolate', ['linear'], ['zoom'], 18, 0, 18.1, 1],
    },
  }
}

function makeGtfsHintSymbolLayer(
  lucideIconImageSourceProvider: UseLucideIconImageSourceProviderReturn,
): SymbolLayerSpecification {
  return {
    id: GtfsLayerId.HintSymbols,
    type: 'symbol',
    source: GTFS_SOURCE_ID,
    'source-layer': GTFS_SOURCE_LAYER_ID,
    minzoom: 17,
    filter: ['==', makeLocationTypeExpression(), GtfsStopLocationType.EntranceExit],
    layout: {
      'icon-image': lucideIconImageSourceProvider.makeImageIdFromIconNameExpression(
        makeGtfsIconNameExpression(),
        'rgb(95 122 136)',
      ),
      'icon-size': ['interpolate', ['linear'], ['zoom'], 17, 0.4, 20, 1.0],
      'icon-anchor': 'center',
      'icon-pitch-alignment': 'map',
      'icon-rotation-alignment': 'viewport',
      'symbol-sort-key': 0,
    },
    paint: {
      'icon-opacity': 0.8,
    },
  }
}

function makeRootStopFilter(): ExpressionSpecification {
  return [
    'all',
    makeNotHasStringFieldExpression(GtfsStopField.ParentStationId),
    [
      'any',
      ['==', makeLocationTypeExpression(), GtfsStopLocationType.StopOrPlatform],
      ['==', makeLocationTypeExpression(), GtfsStopLocationType.Station],
    ],
    makeGtfsHasSpecificRouteTypeExpression(),
  ]
}

function makeGtfsIconNameExpression(): ExpressionSpecification {
  const locationType = makeLocationTypeExpression()
  const hasStopCode = makeHasStringFieldExpression(GtfsStopField.StopCode)
  const hasPlatformCode = makeHasStringFieldExpression(GtfsStopField.PlatformCode)

  return [
    'case',
    ['==', locationType, GtfsStopLocationType.Station],
    GtfsIconName.Station,
    ['==', locationType, GtfsStopLocationType.EntranceExit],
    GtfsIconName.EntranceExit,
    ['==', locationType, GtfsStopLocationType.GenericNode],
    GtfsIconName.GenericNode,
    ['==', locationType, GtfsStopLocationType.BoardingArea],
    GtfsIconName.BoardingArea,
    hasPlatformCode,
    GtfsIconName.Platform,
    hasStopCode,
    GtfsIconName.Stop,
    GtfsIconName.Unknown,
  ] as ExpressionSpecification
}

function makeGtfsHasSpecificRouteTypeExpression(): ExpressionSpecification {
  return [
    'in',
    ['coalesce', ['get', GtfsStopField.RouteTypes], ''],
    ['literal', Object.values(GtfsRouteType)],
  ]
}

function makeMatchExpression(
  value: ExpressionSpecification,
  cases: Record<string, number | string | ExpressionSpecification>,
  fallback: ExpressionSpecification | number | string,
): ExpressionSpecification {
  return [
    'match',
    value,
    ...Object.entries(cases).flatMap((v) => v),
    fallback,
  ] as unknown as ExpressionSpecification
}

function makeLocationTypeExpression(): ExpressionSpecification {
  return [
    'to-number',
    ['coalesce', ['get', GtfsStopField.LocationType], GtfsStopLocationType.StopOrPlatform],
  ] as ExpressionSpecification
}

function makeHasStringFieldExpression(fieldName: GtfsStopField): ExpressionSpecification {
  return ['!=', ['coalesce', ['get', fieldName], ''], ''] as ExpressionSpecification
}

function makeNotHasStringFieldExpression(fieldName: GtfsStopField): ExpressionSpecification {
  return ['==', ['coalesce', ['get', fieldName], ''], ''] as ExpressionSpecification
}
