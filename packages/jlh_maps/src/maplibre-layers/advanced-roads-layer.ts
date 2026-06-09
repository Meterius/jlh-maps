import type { ExpressionSpecification, FilterSpecification, MapLibreMap } from 'maplibre-gl'
import {
  RoadsSourceLayerName,
  RoadsSourceFieldName,
  RoadsSourceIntersectionMarkingType,
  RoadsSourceLaneType,
  RoadsSourceNetworkFeatureType,
} from 'jlh_maps_roads_source_schema/schema'
import { useLayer, type UseLayerOptions, useSource } from '@/composables/maplibre'
import { TILESERVER_ROADS_PMTILES_URL } from '@/external/endpoints.ts'

export const ADVANCED_ROADS_SOURCE_ID = 'advanced_roads'
export const ADVANCED_ROADS_BEFORE_LAYER_ID = 'River labels'

const ADVANCED_ROADS_GEOMETRY_LAYER_ID = RoadsSourceLayerName.Network
const ADVANCED_ROADS_LANE_POLYGONS_LAYER_ID = RoadsSourceLayerName.Lanes
const ADVANCED_ROADS_INTERSECTION_MARKINGS_LAYER_ID = RoadsSourceLayerName.IntersectionMarkings
const ADVANCED_ROADS_MIN_ZOOM = 15

export enum AdvancedRoadsLayerId {
  Casing = 'advanced-roads-casing',
  Intersections = 'advanced-roads-intersections',
  LanePolygons = 'advanced-roads-lane-polygons',
  Geometry = 'advanced-roads-geometry',
  IntersectionMarkings = 'advanced-roads-intersection-markings',
}

const ADVANCED_ROADS_LAYER_IDS = Object.values(AdvancedRoadsLayerId)

function makeSurfaceOrElevatedLayerFilter(): FilterSpecification {
  return ['>=', ['coalesce', ['get', RoadsSourceFieldName.Layer], 0], 0]
}

function makeGeometryFeatureFilter(
  featureType: RoadsSourceNetworkFeatureType,
): FilterSpecification {
  return [
    'all',
    ['==', ['get', RoadsSourceFieldName.Type], featureType],
    makeSurfaceOrElevatedLayerFilter(),
  ] as FilterSpecification
}

function makeLaneTypeMatchExpression<T>(
  cases: [RoadsSourceLaneType | RoadsSourceLaneType[], T][],
  fallback: T,
): ExpressionSpecification {
  return [
    'match',
    ['get', RoadsSourceFieldName.Type],
    ...cases.flat(),
    fallback,
  ] as unknown as ExpressionSpecification
}

export function useAdvancedRoadsLayers(map: MapLibreMap, options: UseLayerOptions = {}) {
  useSource(map, ADVANCED_ROADS_SOURCE_ID, {
    type: 'vector',
    url: `pmtiles://${TILESERVER_ROADS_PMTILES_URL.toString()}`,
    minzoom: ADVANCED_ROADS_MIN_ZOOM,
    attribution: 'osm2streets / OpenStreetMap contributors',
  })

  useDetailedAdvancedRoadsLayers(map, options)
  return {
    layerIds: [...ADVANCED_ROADS_LAYER_IDS],
  }
}

function useDetailedAdvancedRoadsLayers(map: MapLibreMap, options: UseLayerOptions) {
  useLayer(
    map,
    {
      id: AdvancedRoadsLayerId.Casing,
      type: 'fill',
      source: ADVANCED_ROADS_SOURCE_ID,
      'source-layer': ADVANCED_ROADS_GEOMETRY_LAYER_ID,
      minzoom: ADVANCED_ROADS_MIN_ZOOM,
      filter: makeGeometryFeatureFilter(RoadsSourceNetworkFeatureType.Road),
      paint: {
        'fill-color': '#020617',
      },
    },
    options,
  )

  useLayer(
    map,
    {
      id: AdvancedRoadsLayerId.Intersections,
      type: 'fill',
      source: ADVANCED_ROADS_SOURCE_ID,
      'source-layer': ADVANCED_ROADS_GEOMETRY_LAYER_ID,
      minzoom: ADVANCED_ROADS_MIN_ZOOM,
      filter: makeGeometryFeatureFilter(RoadsSourceNetworkFeatureType.Intersection),
      paint: {
        'fill-color': '#111827',
      },
    },
    options,
  )

  useLayer(
    map,
    {
      id: AdvancedRoadsLayerId.LanePolygons,
      type: 'fill',
      source: ADVANCED_ROADS_SOURCE_ID,
      'source-layer': ADVANCED_ROADS_LANE_POLYGONS_LAYER_ID,
      minzoom: ADVANCED_ROADS_MIN_ZOOM,
      filter: makeSurfaceOrElevatedLayerFilter(),
      paint: {
        'fill-color': makeLaneTypeMatchExpression(
          [
            [RoadsSourceLaneType.Driving, '#334155'],
            [RoadsSourceLaneType.Biking, '#16a34a'],
            [RoadsSourceLaneType.Bus, '#dc2626'],
            [
              [
                RoadsSourceLaneType.ParkingDiagonal,
                RoadsSourceLaneType.ParkingParallel,
                RoadsSourceLaneType.ParkingPerpendicular,
              ],
              '#64748b',
            ],
            [RoadsSourceLaneType.Sidewalk, '#d1d5db'],
            [RoadsSourceLaneType.Shoulder, '#9ca3af'],
            [RoadsSourceLaneType.SharedLeftTurn, '#f97316'],
            [RoadsSourceLaneType.Construction, '#fb923c'],
            [RoadsSourceLaneType.LightRail, '#7c3aed'],
            [
              [
                RoadsSourceLaneType.BufferCurb,
                RoadsSourceLaneType.BufferFlexPosts,
                RoadsSourceLaneType.BufferJerseyBarrier,
                RoadsSourceLaneType.BufferPlanters,
                RoadsSourceLaneType.BufferStripes,
                RoadsSourceLaneType.BufferVerge,
              ],
              '#94a3b8',
            ],
            [RoadsSourceLaneType.Footway, '#facc15'],
            [RoadsSourceLaneType.SharedUse, '#2dd4bf'],
          ],
          '#475569',
        ),
        'fill-outline-color': [
          'case',
          [
            'in',
            ['get', RoadsSourceFieldName.Type],
            [
              'literal',
              [
                RoadsSourceLaneType.Footway,
                RoadsSourceLaneType.Shoulder,
                RoadsSourceLaneType.Sidewalk,
              ],
            ],
          ],
          '#f8fafc',
          [
            'in',
            ['get', RoadsSourceFieldName.Type],
            ['literal', [RoadsSourceLaneType.Biking, RoadsSourceLaneType.SharedUse]],
          ],
          '#bbf7d0',
          '#0f172a',
        ],
      },
    },
    options,
  )

  useLayer(
    map,
    {
      id: AdvancedRoadsLayerId.Geometry,
      type: 'line',
      source: ADVANCED_ROADS_SOURCE_ID,
      'source-layer': ADVANCED_ROADS_GEOMETRY_LAYER_ID,
      minzoom: ADVANCED_ROADS_MIN_ZOOM,
      filter: makeGeometryFeatureFilter(RoadsSourceNetworkFeatureType.Road),
      paint: {
        'line-color': '#020617',
        'line-width': ['interpolate', ['linear'], ['zoom'], 13, 0.4, 18, 1.2],
      },
    },
    options,
  )

  useLayer(
    map,
    {
      id: AdvancedRoadsLayerId.IntersectionMarkings,
      type: 'fill',
      source: ADVANCED_ROADS_SOURCE_ID,
      'source-layer': ADVANCED_ROADS_INTERSECTION_MARKINGS_LAYER_ID,
      minzoom: ADVANCED_ROADS_MIN_ZOOM,
      paint: {
        'fill-color': [
          'match',
          ['get', RoadsSourceFieldName.Type],
          RoadsSourceIntersectionMarkingType.SidewalkCorner,
          '#e5e7eb',
          RoadsSourceIntersectionMarkingType.MarkedCrossingLine,
          '#f8fafc',
          RoadsSourceIntersectionMarkingType.UnmarkedCrossingOutline,
          '#93c5fd',
          '#38bdf8',
        ],
      },
    },
    options,
  )
}
