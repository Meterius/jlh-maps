export enum RoadsSourceLayerName {
  Network = "network",
  Lanes = "lanes",
  IntersectionMarkings = "intersection_markings",
}

export const ROADS_SOURCE_LAYER_NAMES = [
  RoadsSourceLayerName.Network,
  RoadsSourceLayerName.Lanes,
  RoadsSourceLayerName.IntersectionMarkings,
] as const;

export type RoadsSourceLayerNameValue =
  (typeof ROADS_SOURCE_LAYER_NAMES)[number];

export enum RoadsSourceFieldName {
  AllowedTurns = "allowed_turns",
  Direction = "direction",
  DestinationIntersectionId = "dst_i",
  Id = "id",
  Index = "index",
  Layer = "layer",
  OsmNodeIds = "osm_node_ids",
  OsmWayIds = "osm_way_ids",
  Road = "road",
  SourceIntersectionId = "src_i",
  SpeedLimit = "speed_limit",
  Type = "type",
  Width = "width",
}

export enum RoadsSourceNetworkFeatureType {
  Intersection = "intersection",
  Road = "road",
}

export enum RoadsSourceLaneType {
  Biking = "Biking",
  BufferCurb = "Buffer(Curb)",
  BufferFlexPosts = "Buffer(FlexPosts)",
  BufferJerseyBarrier = "Buffer(JerseyBarrier)",
  BufferPlanters = "Buffer(Planters)",
  BufferStripes = "Buffer(Stripes)",
  BufferVerge = "Buffer(Verge)",
  Bus = "Bus",
  Construction = "Construction",
  Driving = "Driving",
  Footway = "Footway",
  LightRail = "LightRail",
  ParkingDiagonal = "Parking(Diagonal)",
  ParkingParallel = "Parking(Parallel)",
  ParkingPerpendicular = "Parking(Perpendicular)",
  SharedLeftTurn = "SharedLeftTurn",
  SharedUse = "SharedUse",
  Shoulder = "Shoulder",
  Sidewalk = "Sidewalk",
}

export const ROADS_SOURCE_PEDESTRIAN_LANE_TYPES = [
  RoadsSourceLaneType.Footway,
  RoadsSourceLaneType.Shoulder,
  RoadsSourceLaneType.Sidewalk,
] as const;

export const ROADS_SOURCE_BICYCLE_LANE_TYPES = [
  RoadsSourceLaneType.Biking,
  RoadsSourceLaneType.SharedUse,
] as const;

export const ROADS_SOURCE_PARKING_LANE_TYPES = [
  RoadsSourceLaneType.ParkingDiagonal,
  RoadsSourceLaneType.ParkingParallel,
  RoadsSourceLaneType.ParkingPerpendicular,
] as const;

export const ROADS_SOURCE_BUFFER_LANE_TYPES = [
  RoadsSourceLaneType.BufferCurb,
  RoadsSourceLaneType.BufferFlexPosts,
  RoadsSourceLaneType.BufferJerseyBarrier,
  RoadsSourceLaneType.BufferPlanters,
  RoadsSourceLaneType.BufferStripes,
  RoadsSourceLaneType.BufferVerge,
] as const;

export enum RoadsSourceDirection {
  Backward = "Backward",
  Forward = "Forward",
}

export enum RoadsSourceIntersectionMarkingType {
  MarkedCrossingLine = "marked crossing line",
  SidewalkCorner = "sidewalk corner",
  UnmarkedCrossingOutline = "unmarked crossing outline",
}

export type RoadsSourceNetworkProperties = {
  dst_i?: number;
  id?: number;
  layer?: number;
  osm_node_ids?: unknown;
  osm_way_ids?: unknown;
  src_i?: number;
  type?: RoadsSourceNetworkFeatureType | string;
};

export type RoadsSourceLaneProperties = {
  allowed_turns?: unknown;
  direction?: RoadsSourceDirection | string;
  index?: number;
  layer?: number;
  osm_way_ids?: unknown;
  road?: number;
  speed_limit?: string;
  type?: RoadsSourceLaneType | string;
  width?: number;
};

export type RoadsSourceIntersectionMarkingProperties = {
  type?: RoadsSourceIntersectionMarkingType | string;
};

export type VectorLayerFieldType = "Boolean" | "Number" | "String";

export type VectorLayerMetadata = {
  description: string;
  fields: Record<string, VectorLayerFieldType>;
  id: RoadsSourceLayerName;
};

export const ROADS_SOURCE_VECTOR_LAYER_FIELDS = {
  [RoadsSourceLayerName.Network]: {
    dst_i: "Number",
    id: "Number",
    layer: "Number",
    osm_node_ids: "String",
    osm_way_ids: "String",
    src_i: "Number",
    type: "String",
  },
  [RoadsSourceLayerName.Lanes]: {
    allowed_turns: "String",
    direction: "String",
    index: "Number",
    layer: "Number",
    osm_way_ids: "String",
    road: "Number",
    speed_limit: "String",
    type: "String",
    width: "Number",
  },
  [RoadsSourceLayerName.IntersectionMarkings]: {
    type: "String",
  },
} as const satisfies Record<
  RoadsSourceLayerName,
  Record<string, VectorLayerFieldType>
>;

export const ROADS_SOURCE_VECTOR_LAYERS = ROADS_SOURCE_LAYER_NAMES.map(
  (id) => ({
    id,
    description: `osm2streets source ${id}`,
    fields: ROADS_SOURCE_VECTOR_LAYER_FIELDS[id],
  }),
) satisfies VectorLayerMetadata[];
