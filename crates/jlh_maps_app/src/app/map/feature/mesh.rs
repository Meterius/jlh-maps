use crate::app::map::feature::tile_task_based::{
    TileTaskBased, TileTaskBasedMeta, TileTaskBasedPlugin,
};
use crate::app::map::feature::utils::poly::ring_without_closing_position;
use crate::app::map::transform::{tile_flat_world_center, world_from_lng_lat_alt};
use crate::app::maplibre_gl_js::types::{MlTerrainTile, MlTile, MlTileFeature};
use crate::app::maplibre_gl_js::utils::mercator_coordinate::{
    lng_lat_is_in_bounds, tile_uv_from_lng_lat,
};
use crate::app::maplibre_gl_js::utils::terrain::get_terrain_elevation;
use crate::app::maplibre_gl_js::utils::tile::get_tile_lnglat_bounds;
use bevy::app::App;
use bevy::asset::{Assets, Handle, RenderAssetUsages};
use bevy::ecs::system::SystemParamItem;
use bevy::math::{DVec3, dvec2};
use bevy::mesh::{Indices, Mesh, Mesh3d, PrimitiveTopology};
use bevy::prelude::{Commands, Entity, Plugin, ResMut};
use geojson::{JsonValue, Value};
use std::collections::HashMap;
use std::sync::Arc;

pub struct FeatureMeshPlugin;

impl Plugin for FeatureMeshPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TileTaskBasedPlugin::<FeatureTileMeshMeta>::new());
    }
}

// ECS

pub type FeatureTileMesh = TileTaskBased<FeatureTileMeshMeta>;

pub struct FeatureTileMeshMeta;

#[derive(Clone, Copy, Default)]
pub struct FeatureTileMeshConfig {
    pub layer_id: &'static str,
    pub base_property_keys: Option<&'static [&'static str]>,
    pub top_property_keys: Option<&'static [&'static str]>,
    pub wall_normal_smooth_angle: Option<f32>,
}

#[derive(Default)]
pub struct FeatureTileMeshState {
    mesh_handle: Option<Handle<Mesh>>,
}

impl FeatureTileMesh {
    pub fn new(config: FeatureTileMeshConfig) -> Self {
        TileTaskBased::from_parts(config, FeatureTileMeshState::default())
    }
}

impl TileTaskBasedMeta for FeatureTileMeshMeta {
    type Data = FeatureTileMeshData;
    type State = FeatureTileMeshState;
    type Config = FeatureTileMeshConfig;
    type ApplyParams = (Commands<'static, 'static>, ResMut<'static, Assets<Mesh>>);

    fn use_terrain() -> bool {
        true
    }

    fn build_data(
        tile: Arc<MlTile>,
        terrain_data: Option<Arc<MlTerrainTile>>,
        config: Self::Config,
    ) -> Self::Data {
        build_mesh_data(tile, terrain_data, config)
    }

    fn apply_data(
        entity_eid: Entity,
        params: &mut SystemParamItem<'_, '_, Self::ApplyParams>,
        _config: &Self::Config,
        state: &mut Self::State,
        data: Option<Self::Data>,
    ) -> Option<Self::Data> {
        let (commands, meshes) = params;
        apply_mesh(commands, entity_eid, state, data, meshes);
        None
    }
}

pub struct FeatureTileMeshData(Option<Mesh>);

impl FeatureTileMeshData {
    fn empty() -> Self {
        Self(None)
    }

    fn mesh(mesh: Mesh) -> Self {
        Self(Some(mesh))
    }

    fn into_mesh(self) -> Option<Mesh> {
        self.0
    }
}

#[derive(Default)]
struct FeaturePlaneMeshBuffers {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    feature_data: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl FeaturePlaneMeshBuffers {
    fn is_empty(&self) -> bool {
        self.positions.is_empty() || self.indices.is_empty()
    }

    fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, self.feature_data);
        mesh.insert_indices(Indices::U32(self.indices));
        mesh
    }
}

// Mesh Construction / Application

fn apply_mesh(
    commands: &mut Commands,
    bucket_eid: Entity,
    state: &mut FeatureTileMeshState,
    data: Option<FeatureTileMeshData>,
    meshes: &mut Assets<Mesh>,
) {
    let Some(mesh) = data.and_then(FeatureTileMeshData::into_mesh) else {
        commands.entity(bucket_eid).try_remove::<Mesh3d>();
        state.mesh_handle = None;
        return;
    };

    if let Some(mesh_handle) = &state.mesh_handle {
        if let Some(existing_mesh) = meshes.get_mut(mesh_handle) {
            *existing_mesh = mesh;
        }
    } else {
        let mesh_handle = meshes.add(mesh);
        state.mesh_handle = Some(mesh_handle.clone());
        commands.entity(bucket_eid).insert(Mesh3d(mesh_handle));
    }
}

fn build_mesh_data(
    tile: Arc<MlTile>,
    terrain_data: Option<Arc<MlTerrainTile>>,
    config: FeatureTileMeshConfig,
) -> FeatureTileMeshData {
    let tile_id = tile.id;
    let bounds = get_tile_lnglat_bounds(tile_id);
    let center = tile_flat_world_center(tile_id);
    let mut buffers = FeaturePlaneMeshBuffers::default();

    for feature in tile
        .layers
        .get(config.layer_id)
        .iter()
        .flat_map(|layer| layer.features.values())
    {
        push_feature_mesh(
            feature,
            center,
            bounds,
            terrain_data.as_deref(),
            &config,
            &mut buffers,
        );
    }

    if buffers.is_empty() {
        FeatureTileMeshData::empty()
    } else {
        FeatureTileMeshData::mesh(buffers.into_mesh())
    }
}

fn push_feature_mesh(
    feature: &MlTileFeature,
    center: DVec3,
    bounds: (bevy::math::DVec2, bevy::math::DVec2),
    terrain_data: Option<&MlTerrainTile>,
    altitude_config: &FeatureTileMeshConfig,
    buffers: &mut FeaturePlaneMeshBuffers,
) -> bool {
    fn feature_altitude_property(
        properties: &HashMap<String, JsonValue>,
        keys: &[&str],
    ) -> Option<f64> {
        keys.iter()
            .find_map(|key| properties.get(*key).and_then(json_value_as_f64))
    }

    fn json_value_as_f64(value: &JsonValue) -> Option<f64> {
        match value {
            JsonValue::Number(value) => value.as_f64(),
            JsonValue::String(value) => value.parse().ok(),
            _ => None,
        }
    }

    let base_altitude = altitude_config
        .base_property_keys
        .and_then(|keys| feature_altitude_property(&feature.properties, keys))
        .unwrap_or(0.0);

    let top_altitude = altitude_config
        .top_property_keys
        .and_then(|keys| feature_altitude_property(&feature.properties, keys))
        .map(|top_altitude| top_altitude.max(base_altitude + 0.1));

    let start_position_count = buffers.positions.len();
    let start_index_count = buffers.indices.len();

    match &feature.geometry.value {
        Value::Polygon(polygon) => push_polygon_mesh(
            polygon,
            center,
            bounds,
            base_altitude,
            top_altitude,
            terrain_data,
            altitude_config.wall_normal_smooth_angle,
            &mut buffers.positions,
            &mut buffers.normals,
            &mut buffers.uvs,
            &mut buffers.feature_data,
            &mut buffers.indices,
        ),
        Value::MultiPolygon(polygons) => {
            for polygon in polygons {
                push_polygon_mesh(
                    polygon,
                    center,
                    bounds,
                    base_altitude,
                    top_altitude,
                    terrain_data,
                    altitude_config.wall_normal_smooth_angle,
                    &mut buffers.positions,
                    &mut buffers.normals,
                    &mut buffers.uvs,
                    &mut buffers.feature_data,
                    &mut buffers.indices,
                );
            }
        }
        _ => return false,
    }

    if buffers.positions.len() == start_position_count || buffers.indices.len() == start_index_count
    {
        buffers.positions.truncate(start_position_count);
        buffers.normals.truncate(start_position_count);
        buffers.uvs.truncate(start_position_count);
        buffers.feature_data.truncate(start_position_count);
        buffers.indices.truncate(start_index_count);
        false
    } else {
        true
    }
}

// Polygon Mesh Construction

fn feature_vertex_data(altitude: f64) -> [f32; 2] {
    [altitude as f32, 0.0]
}

#[allow(clippy::too_many_arguments)]
fn push_polygon_mesh(
    polygon: &[Vec<Vec<f64>>],
    center: DVec3,
    bounds: (bevy::math::DVec2, bevy::math::DVec2),
    base_altitude: f64,
    top_altitude: Option<f64>,
    terrain_data: Option<&MlTerrainTile>,
    wall_normal_smooth_angle: Option<f32>,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    feature_data: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) {
    let first_vertex = positions.len() as u32;
    let mut flat_coords = Vec::new();
    let mut hole_indices = Vec::new();
    let mut vertex_count = 0usize;
    let mut rings = Vec::new();
    let surface_altitude = top_altitude.unwrap_or(base_altitude);

    for (ring_index, ring) in polygon.iter().enumerate() {
        let ring_positions = ring_without_closing_position(ring);
        let lnglats = ring_positions
            .iter()
            .filter_map(|position| {
                if position.len() >= 2 {
                    Some(dvec2(position[0], position[1]))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        if lnglats.len() < 3 {
            continue;
        }

        if ring_index > 0 {
            hole_indices.push(vertex_count);
        }

        let ring_first_vertex = positions.len();
        let mut ring_world_positions = Vec::new();
        for lnglat in lnglats {
            let terrain_altitude = terrain_data
                .filter(|_| lng_lat_is_in_bounds(bounds, lnglat))
                .and_then(|terrain_data| {
                    get_terrain_elevation(
                        &terrain_data.terrain_data,
                        tile_uv_from_lng_lat(bounds, lnglat),
                    )
                })
                .map(f64::from)
                .unwrap_or(0.0);

            let world =
                world_from_lng_lat_alt(lnglat.x, lnglat.y, surface_altitude + terrain_altitude)
                    - center;

            flat_coords.push(world.x);
            flat_coords.push(world.y);
            positions.push(world.as_vec3().to_array());
            normals.push([0.0, 0.0, 1.0]);
            uvs.push(tile_uv_from_lng_lat(bounds, lnglat).to_array());
            feature_data.push(feature_vertex_data(surface_altitude + terrain_altitude));
            if top_altitude.is_some() {
                ring_world_positions.push(ExtrusionVertex {
                    top: world,
                    top_altitude: surface_altitude + terrain_altitude,
                    base: world_from_lng_lat_alt(
                        lnglat.x,
                        lnglat.y,
                        base_altitude + terrain_altitude,
                    ) - center,
                    base_altitude: base_altitude + terrain_altitude,
                });
            }
            vertex_count += 1;
        }

        if top_altitude.is_some() && positions.len() - ring_first_vertex >= 3 {
            rings.push((ring_world_positions, ring_index > 0));
        }
    }

    if vertex_count < 3 {
        positions.truncate(first_vertex as usize);
        normals.truncate(first_vertex as usize);
        uvs.truncate(first_vertex as usize);
        feature_data.truncate(first_vertex as usize);
        return;
    }

    let Ok(triangle_indices) = earcutr::earcut(&flat_coords, &hole_indices, 2) else {
        positions.truncate(first_vertex as usize);
        normals.truncate(first_vertex as usize);
        uvs.truncate(first_vertex as usize);
        feature_data.truncate(first_vertex as usize);
        return;
    };

    indices.extend(
        triangle_indices
            .into_iter()
            .filter_map(|index| u32::try_from(index).ok())
            .map(|index| first_vertex + index),
    );

    for (ring_world_positions, is_hole) in rings {
        push_extrusion_wall_mesh(
            &ring_world_positions,
            is_hole,
            positions,
            normals,
            uvs,
            feature_data,
            indices,
            wall_normal_smooth_angle,
        );
    }
}

struct ExtrusionVertex {
    top: DVec3,
    top_altitude: f64,
    base: DVec3,
    base_altitude: f64,
}

#[allow(clippy::too_many_arguments)]
fn push_extrusion_wall_mesh(
    ring_positions: &[ExtrusionVertex],
    is_hole: bool,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    feature_data: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
    wall_normal_smooth_angle: Option<f32>,
) {
    let ring_len = ring_positions.len();
    if ring_len < 2 {
        return;
    }

    let ring_area = signed_area_xy(ring_positions);
    let outward_right = (ring_area >= 0.0) != is_hole;
    let edge_normals = (0..ring_len)
        .map(|edge_index| {
            let next_edge_index = (edge_index + 1) % ring_len;
            wall_edge_normal(
                ring_positions[edge_index].top,
                ring_positions[next_edge_index].top,
                outward_right,
            )
        })
        .collect::<Vec<_>>();

    for edge_index in 0..ring_len {
        let next_edge_index = (edge_index + 1) % ring_len;
        let top_left = ring_positions[edge_index].top;
        let top_right = ring_positions[next_edge_index].top;
        let top_left_altitude = ring_positions[edge_index].top_altitude;
        let top_right_altitude = ring_positions[next_edge_index].top_altitude;
        let top_left_normal = wall_vertex_normal(
            &edge_normals,
            edge_index,
            edge_index,
            wall_normal_smooth_angle,
        );
        let top_right_normal = wall_vertex_normal(
            &edge_normals,
            next_edge_index,
            edge_index,
            wall_normal_smooth_angle,
        );

        let base_left = ring_positions[edge_index].base;
        let base_right = ring_positions[next_edge_index].base;
        let base_left_altitude = ring_positions[edge_index].base_altitude;
        let base_right_altitude = ring_positions[next_edge_index].base_altitude;

        let first_wall_vertex = positions.len() as u32;
        positions.push(top_left.as_vec3().to_array());
        positions.push(top_right.as_vec3().to_array());
        positions.push(base_right.as_vec3().to_array());
        positions.push(base_left.as_vec3().to_array());
        normals.extend([
            top_left_normal,
            top_right_normal,
            top_right_normal,
            top_left_normal,
        ]);
        uvs.push([edge_index as f32, 1.0]);
        uvs.push([next_edge_index as f32, 1.0]);
        uvs.push([next_edge_index as f32, 0.0]);
        uvs.push([edge_index as f32, 0.0]);
        feature_data.extend([
            feature_vertex_data(top_left_altitude),
            feature_vertex_data(top_right_altitude),
            feature_vertex_data(base_right_altitude),
            feature_vertex_data(base_left_altitude),
        ]);

        if outward_right {
            indices.extend([
                first_wall_vertex,
                first_wall_vertex + 2,
                first_wall_vertex + 1,
                first_wall_vertex,
                first_wall_vertex + 3,
                first_wall_vertex + 2,
            ]);
        } else {
            indices.extend([
                first_wall_vertex,
                first_wall_vertex + 1,
                first_wall_vertex + 2,
                first_wall_vertex,
                first_wall_vertex + 2,
                first_wall_vertex + 3,
            ]);
        }
    }
}

fn signed_area_xy(positions: &[ExtrusionVertex]) -> f64 {
    positions
        .iter()
        .zip(positions.iter().cycle().skip(1))
        .take(positions.len())
        .map(|(left, right)| left.top.x * right.top.y - right.top.x * left.top.y)
        .sum::<f64>()
        * 0.5
}

fn wall_edge_normal(left: DVec3, right: DVec3, outward_right: bool) -> DVec3 {
    let edge = right - left;
    if outward_right {
        DVec3::new(edge.y, -edge.x, 0.0)
    } else {
        DVec3::new(-edge.y, edge.x, 0.0)
    }
    .normalize_or_zero()
}

fn wall_vertex_normal(
    edge_normals: &[DVec3],
    vertex_index: usize,
    current_edge_index: usize,
    smooth_angle: Option<f32>,
) -> [f32; 3] {
    let current = edge_normals[current_edge_index];
    let Some(smooth_angle) = smooth_angle else {
        return current.as_vec3().to_array();
    };
    if edge_normals.len() < 3 || current.length_squared() <= f64::EPSILON {
        return current.as_vec3().to_array();
    }

    let other_edge_index = if current_edge_index == vertex_index {
        (vertex_index + edge_normals.len() - 1) % edge_normals.len()
    } else {
        vertex_index
    };
    let other = edge_normals[other_edge_index];
    if other.length_squared() <= f64::EPSILON {
        return current.as_vec3().to_array();
    }

    let cos_angle = current.dot(other).clamp(-1.0, 1.0);
    let smooth_cos = f64::from(smooth_angle.clamp(0.0, std::f32::consts::PI).cos());
    if cos_angle < smooth_cos {
        return current.as_vec3().to_array();
    }

    (current + other).normalize_or_zero().as_vec3().to_array()
}
