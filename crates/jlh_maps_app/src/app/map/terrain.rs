use crate::app::common::debug_gizmos::DebugAabbGizmo;
use crate::app::common::materials::TransparentOverwriteMaterial;
use crate::app::map::core::{MAP_VIEW_COLOR_RENDER_LAYER, MAP_VIEW_DEPTH_RENDER_LAYER};
use crate::app::map::transform::MERCATOR_WORLD_SIZE;
use crate::app::maplibre_gl_js::integration::MaplibreMapIntegration;
use crate::app::maplibre_gl_js::types::{CanonicalTileId, MlTerrainTile};
use crate::app::maplibre_gl_js::utils::mercator_coordinate::{
    EARTH_CIRCUMFERENCE, LngLat, MercatorCoordinate,
};
use crate::app::maplibre_gl_js::utils::terrain::get_terrain_elevation;
use crate::app::maplibre_gl_js::utils::tile::{get_tile_lnglat_bounds, tile_transform_d};
use crate::app::task_pool::AppTaskPool;
use crate::utils::debug::SoftExpect;
use crate::utils::terrain_mesh::build_terrain_mesh_with_skirts;
use crate::wasm_task_pool::Task;
use bevy::camera::visibility::RenderLayers;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use big_space::grid::Grid;
use big_space::prelude::CellCoord;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;

const TILE_TERRAIN_MESH_RESOLUTION: u32 = 128;

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TerrainFlatMesh>()
            .init_resource::<TerrainMaterial>()
            .add_systems(Update, (sync_spawned_tiles, sync_tiles).chain());
    }
}

#[derive(Resource)]
struct TerrainFlatMesh(Handle<Mesh>);

impl FromWorld for TerrainFlatMesh {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        Self(meshes.add(Mesh::from(Plane3d::new(Vec3::Z, Vec2::ONE / 2.0))))
    }
}

#[derive(Resource)]
struct TerrainMaterial(Handle<TransparentOverwriteMaterial>);

impl FromWorld for TerrainMaterial {
    fn from_world(world: &mut World) -> Self {
        let mut materials = world.resource_mut::<Assets<TransparentOverwriteMaterial>>();
        Self(materials.add(TransparentOverwriteMaterial::new(0.4)))
    }
}

#[derive(Component)]
pub struct TerrainTileManager {
    pub maplibre_int_id: Entity,
    pub spawned_tiles: HashMap<CanonicalTileId, Entity>,
}

fn sync_spawned_tiles(
    mut commands: Commands,
    map_ints: Query<&MaplibreMapIntegration>,
    mut managers: Query<(Entity, &mut TerrainTileManager)>,
    grids: Query<&Grid>,
    flat_mesh: Res<TerrainFlatMesh>,
    material: Res<TerrainMaterial>,
) {
    for (manager_id, mut manager) in managers.iter_mut() {
        let maplibre_int_id = manager.maplibre_int_id;

        let Some(map_int) = map_ints.get(maplibre_int_id).ok().soft_expect("") else {
            continue;
        };
        let Some(grid) = grids.get(manager_id).ok().soft_expect("") else {
            continue;
        };

        for &tile_id in map_int.terrain.active_tile_ids.iter() {
            if let Entry::Vacant(entry) = manager.spawned_tiles.entry(tile_id) {
                let (tile_cell, tile_transform) = terrain_tile_transform(grid, tile_id);
                let tile_e_id = commands
                    .spawn((
                        Name::new(format!("Terrain Tile {tile_id:?}")),
                        tile_transform,
                        tile_cell,
                        Visibility::Inherited,
                        Mesh3d(flat_mesh.0.clone()),
                        MeshMaterial3d(material.0.clone()),
                        DebugAabbGizmo,
                        NotShadowCaster,
                        TerrainTile {
                            maplibre_int_id,
                            maplibre_tile_id: tile_id,
                            terrain_hash: None,
                            pending_mesh_task: None,
                        },
                        RenderLayers::from_layers(&[
                            MAP_VIEW_DEPTH_RENDER_LAYER,
                            MAP_VIEW_COLOR_RENDER_LAYER,
                        ]),
                    ))
                    .id();
                commands.entity(manager_id).add_child(tile_e_id);
                entry.insert(tile_e_id);
            }
        }

        for (tile_id, tile_entity) in &manager.spawned_tiles {
            let visibility = if map_int.terrain.active_tile_ids.contains(tile_id) {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            commands.entity(*tile_entity).insert(visibility);
        }

        for (_, tile_entity) in manager.spawned_tiles.extract_if(|tile_id, _| {
            !map_int.terrain.active_tile_ids.contains(tile_id)
                && !map_int.terrain.tiles.contains_key(tile_id)
        }) {
            commands.entity(tile_entity).despawn();
        }
    }
}

#[derive(Component)]
pub struct TerrainTile {
    pub maplibre_int_id: Entity,
    pub maplibre_tile_id: CanonicalTileId,
    pub terrain_hash: Option<u64>,
    pending_mesh_task: Option<PendingTerrainMeshTask>,
}

struct PendingTerrainMeshTask {
    terrain_hash: u64,
    task: Task<Mesh>,
}

fn sync_tiles(
    map_ints: Query<&MaplibreMapIntegration>,
    mut tiles: Query<(&mut TerrainTile, &mut Mesh3d)>,
    task_pool: Res<AppTaskPool>,
    flat_mesh: Res<TerrainFlatMesh>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (mut tile, mut tile_mesh) in tiles.iter_mut() {
        let Some(map_int) = map_ints.get(tile.maplibre_int_id).ok().soft_expect("") else {
            continue;
        };

        let terrain_data = map_int.terrain.tiles.get(&tile.maplibre_tile_id);
        let terrain_hash = terrain_data.map(|terrain_data| terrain_data.hash);

        // apply task result or abort task if terrain has changed
        if let Some(mut task) = tile.pending_mesh_task.take()
            && terrain_hash == Some(task.terrain_hash) {
                if task.task.is_finished() {
                    if let Some(mesh) = task.task.take_result() {
                        tile.terrain_hash = Some(task.terrain_hash);
                        *tile_mesh = Mesh3d(meshes.add(mesh));
                    } else {
                        tile.terrain_hash = None;
                        *tile_mesh = Mesh3d(flat_mesh.0.clone());
                    }
                } else {
                    tile.pending_mesh_task = Some(task);
                }
            }

        // check if terrain is different and no task is pending
        if terrain_hash == tile.terrain_hash || tile.pending_mesh_task.is_some() {
            continue;
        }

        // either enqueue task to generate terrain, or if terrain is empty, apply flat mesh
        if let Some(terrain_data) = terrain_data {
            tile.pending_mesh_task = Some(PendingTerrainMeshTask {
                terrain_hash: terrain_data.hash,
                task: {
                    let tile_id = tile.maplibre_tile_id;
                    let terrain_data = Arc::clone(terrain_data);
                    task_pool.spawn(move || build_terrain_tile_mesh(tile_id, terrain_data))
                },
            });
        } else {
            tile.terrain_hash = None;
            *tile_mesh = Mesh3d(flat_mesh.0.clone());
        }
    }
}

fn terrain_tile_transform(grid: &Grid, tile_id: CanonicalTileId) -> (CellCoord, Transform) {
    let (tile_pos, tile_size) = tile_transform_d(tile_id, 0.);
    let (tile_cell, tile_cell_pos) = grid.translation_to_grid(tile_pos);
    let tile_transform =
        Transform::from_translation(tile_cell_pos).with_scale(tile_size.as_vec2().extend(1.0));

    (tile_cell, tile_transform)
}

fn build_terrain_tile_mesh(tile_id: CanonicalTileId, terrain_data: Arc<MlTerrainTile>) -> Mesh {
    let bounds = get_tile_lnglat_bounds(tile_id);

    let get_elevation = |uv: Vec2| {
        let uv = vec2(0.0, 1.0) + vec2(1.0, -1.0) * uv;

        let lnglat = bounds.0 + (bounds.1 - bounds.0) * uv.as_dvec2();

        let dem_elev = get_terrain_elevation(&terrain_data.terrain_data, uv).unwrap_or(0.0) as f64;

        (MercatorCoordinate::from_lng_lat(LngLat::new(lnglat.x, lnglat.y), dem_elev).z
            * MERCATOR_WORLD_SIZE) as f32
    };

    build_terrain_mesh_with_skirts(
        &get_elevation,
        TILE_TERRAIN_MESH_RESOLUTION,
        terrain_skirt_delta(tile_id),
    )
}

fn terrain_skirt_delta(tile_id: CanonicalTileId) -> f32 {
    let bounds = get_tile_lnglat_bounds(tile_id);

    let center = (bounds.0 + bounds.1) * 0.5;
    let frame_delta_meters = EARTH_CIRCUMFERENCE / 2.0_f64.powi(tile_id.z as i32) / 5.0;

    (MercatorCoordinate::from_lng_lat(LngLat::new(center.x, center.y), frame_delta_meters).z
        * MERCATOR_WORLD_SIZE) as f32
}
