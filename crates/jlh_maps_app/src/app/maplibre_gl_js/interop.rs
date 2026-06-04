use crate::app::instance::{BevyInstance, BevyInstanceInner};
use crate::app::maplibre_gl_js::integration::{
    MaplibreMapIntegration, NEXT_INTEGRATION_ID, find_map_integration, with_map_data_mut,
};
use crate::app::maplibre_gl_js::types::{CanonicalTileId, MlTerrainTile, MlView};
use crate::app::maplibre_gl_js::utils::dem_data::DEMData;
use crate::app::maplibre_gl_js::utils::terrain::TerrainData;
use crate::app::task_pool::AppTaskPool;
use anyhow::anyhow;
use bevy::math::DMat4;
use bevy::prelude::{Name, World, default};
use std::collections::HashSet;
use std::rc::Weak;
use std::sync::Arc;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
pub struct MaplibreIntegration {
    instance: Weak<BevyInstanceInner>,
    integration_id: u32,
}

fn parse_serialized_canonical_tile_id(tile_id: &str) -> anyhow::Result<CanonicalTileId> {
    let mut parts = tile_id.split('/');
    let z = parts
        .next()
        .ok_or(anyhow!("Missing z coordinate in tile key"))?
        .parse()?;
    let x = parts
        .next()
        .ok_or(anyhow!("Missing x coordinate in tile key"))?
        .parse()?;
    let y = parts
        .next()
        .ok_or(anyhow!("Missing y coordinate in tile key"))?
        .parse()?;

    Ok(CanonicalTileId { z, x, y })
}

fn parse_terrain_matrix(encoded: &str) -> anyhow::Result<DMat4> {
    let terrain_matrix = serde_json::from_str::<Vec<f64>>(encoded)?;
    terrain_matrix
        .as_slice()
        .try_into()
        .map(DMat4::from_cols_array)
        .map_err(|_| anyhow!("Invalid terrain matrix format"))
}

#[wasm_bindgen]
impl BevyInstance {
    pub fn create_map_integration(&self) -> Result<MaplibreIntegration, String> {
        let id = NEXT_INTEGRATION_ID.with(|next| {
            let id = next.get();
            next.set(id.saturating_add(1).max(1));
            id
        });

        self.execute(move |world| {
            world.spawn((
                MaplibreMapIntegration { id, ..default() },
                Name::new(format!("MapLibre map integration {id}")),
            ));
        })?;

        Ok(MaplibreIntegration {
            instance: self.weak_inner(),
            integration_id: id,
        })
    }
}

#[wasm_bindgen]
impl MaplibreIntegration {
    #[allow(clippy::too_many_arguments)]
    pub fn sync_view(
        &self,
        width: f64,
        height: f64,
        zoom: f64,
        pitch: f64,
        bearing: f64,
        center_lng: f64,
        center_lat: f64,
        main_matrix: Vec<f64>,
    ) -> Result<(), String> {
        if main_matrix.len() != 16 {
            return Err(format!(
                "Expected 16 main matrix values, got {}",
                main_matrix.len()
            ));
        }

        let view = MlView {
            width,
            height,
            zoom,
            pitch,
            bearing,
            center_lng,
            center_lat,
            main_matrix,
        };
        let integration_id = self.integration_id;

        self.execute(move |world| {
            with_map_data_mut(world, integration_id, |map_data| {
                map_data.view = view;
            });
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_terrain_tile_data(
        &self,
        serialized_canonical_tile_id: String,
        hash: u64,
        stride: u32,
        dim: u32,
        min: f64,
        max: f64,
        red_factor: f64,
        green_factor: f64,
        blue_factor: f64,
        base_shift: f64,
        terrain_exaggeration: f64,
        terrain_matrix_json: String,
        data: Vec<u32>,
    ) -> Result<(), String> {
        let tile_key = parse_serialized_canonical_tile_id(&serialized_canonical_tile_id)
            .map_err(|err| err.to_string())?;
        let terrain_matrix =
            parse_terrain_matrix(&terrain_matrix_json).map_err(|err| err.to_string())?;

        let tile_data = MlTerrainTile {
            id: tile_key,
            hash,
            terrain_data: TerrainData {
                dem_data: DEMData {
                    data,
                    stride,
                    dim,
                    min,
                    max,
                    red_factor,
                    green_factor,
                    blue_factor,
                    base_shift,
                },
                terrain_matrix,
                exaggeration: terrain_exaggeration,
            },
        };
        let integration_id = self.integration_id;

        self.execute(move |world| {
            with_map_data_mut(world, integration_id, |map_data| {
                map_data.terrain.tiles.insert(tile_key, Arc::new(tile_data));
            });
        })
    }

    pub fn remove_terrain_tile_data(
        &self,
        serialized_canonical_tile_id: String,
    ) -> Result<(), String> {
        let tile_id = parse_serialized_canonical_tile_id(&serialized_canonical_tile_id)
            .map_err(|err| err.to_string())?;
        let integration_id = self.integration_id;

        self.execute(move |world| {
            with_map_data_mut(world, integration_id, |map_data| {
                map_data.terrain.tiles.remove(&tile_id);
            });
        })
    }

    pub fn update_source_tile(
        &self,
        source_id: String,
        serialized_canonical_tile_id: String,
        data: Vec<u8>,
    ) -> Result<(), String> {
        let tile_id = parse_serialized_canonical_tile_id(&serialized_canonical_tile_id)
            .map_err(|err| err.to_string())?;

        let integration_id = self.integration_id;

        self.execute(move |world| {
            let Some(task_pool) = world.get_resource::<AppTaskPool>().cloned() else {
                tracing::warn!(
                    "Failed to schedule MapLibre source tile parse: AppTaskPool resource not found"
                );
                return;
            };

            with_map_data_mut(world, integration_id, |map_data| {
                map_data
                    .data
                    .update_tile(source_id, tile_id, data, &task_pool);
            });
        })
    }

    pub fn remove_source_tile(
        &self,
        source_id: String,
        serialized_canonical_tile_id: String,
    ) -> Result<(), String> {
        let tile_id = parse_serialized_canonical_tile_id(&serialized_canonical_tile_id)
            .map_err(|err| err.to_string())?;

        let integration_id = self.integration_id;

        self.execute(move |world| {
            with_map_data_mut(world, integration_id, |map_data| {
                map_data.data.remove_tile(&source_id, &tile_id);
            });
        })
    }

    pub fn sync_source_renderable_tile_ids(
        &self,
        source_id: String,
        renderable_tile_ids: Vec<String>,
    ) -> Result<(), String> {
        let renderable_tile_ids = renderable_tile_ids
            .into_iter()
            .map(|v| parse_serialized_canonical_tile_id(&v))
            .collect::<Result<HashSet<_>, _>>()
            .map_err(|err| err.to_string())?;

        let integration_id = self.integration_id;

        self.execute(move |world| {
            with_map_data_mut(world, integration_id, |map_data| {
                map_data
                    .data
                    .set_renderable_tiles(source_id, renderable_tile_ids);
            });
        })
    }

    pub fn sync_terrain_active_tile_ids(&self, active_tile_ids: Vec<String>) -> Result<(), String> {
        let active_tile_ids = active_tile_ids
            .into_iter()
            .map(|v| parse_serialized_canonical_tile_id(&v))
            .collect::<Result<HashSet<_>, _>>()
            .map_err(|err| err.to_string())?;

        let integration_id = self.integration_id;

        self.execute(move |world| {
            with_map_data_mut(world, integration_id, |map_data| {
                map_data.terrain.active_tile_ids = active_tile_ids;
            });
        })
    }
}

impl MaplibreIntegration {
    fn execute(&self, command: impl FnOnce(&mut World)) -> Result<(), String> {
        let Some(instance) = self.instance.upgrade() else {
            return Err("Bevy instance is not mounted".to_string());
        };

        instance.execute(command)
    }
}

impl Drop for MaplibreIntegration {
    fn drop(&mut self) {
        let Some(instance) = self.instance.upgrade() else {
            return;
        };

        let integration_id = self.integration_id;

        let _ = instance.execute(move |world| {
            if let Some(entity) = find_map_integration(world, integration_id) {
                world.despawn(entity);
            }
        });
    }
}
