use crate::app::common::editor::GameViewCamera;
use crate::app::common::skybox_shader::SkyboxShaderCamera;
use crate::app::main::AppWindows;
use crate::app::map::camera::MapViewCamera;
use crate::app::map::feature_layers::make_bucket_manager;
use crate::app::map::lighting::{CelestialDirectionalLight, CelestialDirectionalLightKind};
use crate::app::map::terrain::TerrainTileManager;
use crate::app::map::transform::MERCATOR_WORLD_SIZE;
use crate::app::maplibre_gl_js::utils::mercator_coordinate::{LngLat, MercatorCoordinate};
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{CameraOutputMode, RenderTarget, Viewport};
use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy::window::{Window, WindowRef};
use big_space::bundles::BigSpaceRootBundle;
use big_space::prelude::{CellCoord, FloatingOrigin};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tsify::Tsify;

const FIRST_CASCADE_FAR_METERS: f64 = 2_000.0;
const SHADOW_MAX_DISTANCE_METERS: f64 = 10_000.0;
const SHADOW_MIN_DISTANCE_METERS: f64 = 1.0;
const SHADOW_DEPTH_BIAS: f32 = 0.01;
const SHADOW_NORMAL_BIAS: f32 = 1.8;

pub const MAP_VIEW_COLOR_RENDER_LAYER: usize = 1;
pub const MAP_VIEW_NON_TERRAIN_RENDER_LAYER: usize = 2;
pub const MAP_VIEW_TERRAIN_RENDER_LAYER: usize = 3;

const MAP_TEXTURE_ATLAS_COLUMNS: u32 = 2;

pub(super) struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MapViewSettings::default())
            .add_systems(PreUpdate, sync_map_texture_atlas_camera_viewports);
    }
}

#[derive(Debug, Component)]
struct MapTextureAtlasCamera {
    slot: MapTextureAtlasSlot,
}

#[derive(Debug, Clone, Copy)]
enum MapTextureAtlasSlot {
    Terrain,
    Overlay,
}

impl MapTextureAtlasSlot {
    fn index(self) -> u32 {
        match self {
            Self::Terrain => 0,
            Self::Overlay => 1,
        }
    }
}

#[derive(Debug, Reflect, Resource, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
#[tsify(from_wasm_abi)]
pub struct MapViewSettings {
    pub enable_buildings: bool,
    pub enable_waters: bool,
    pub enable_trees: bool,
    pub enable_shadows: bool,

    pub disable_lighting_hue: bool,

    pub sun_azimuth_degrees: f32,
    pub sun_elevation_degrees: f32,
    pub moon_azimuth_degrees: f32,
    pub moon_elevation_degrees: f32,

    pub feature_visibility_distance: f32,
}

impl Default for MapViewSettings {
    fn default() -> Self {
        Self {
            enable_buildings: true,
            enable_waters: true,
            enable_trees: true,
            enable_shadows: true,
            disable_lighting_hue: false,
            sun_azimuth_degrees: 11.31,
            sun_elevation_degrees: 32.52,
            moon_azimuth_degrees: 191.31,
            moon_elevation_degrees: -32.52,
            feature_visibility_distance: 10.0,
        }
    }
}

#[derive(Debug, Reflect, Component)]
pub struct MapView {
    pub maplibre_int_eid: Entity,
}

pub fn spawn_map_view(
    commands: &mut Commands,
    maplibre_integration_eid: Entity,
    app_windows: &AppWindows,
) {
    let map_view_eid = commands
        .spawn((
            Name::new("Map View"),
            BigSpaceRootBundle::default(),
            Visibility::default(),
            MapView {
                maplibre_int_eid: maplibre_integration_eid,
            },
            TerrainTileManager {
                maplibre_int_eid: maplibre_integration_eid,
                spawned_tile_eids: HashMap::default(),
            },
            make_bucket_manager(maplibre_integration_eid),
        ))
        .id();

    let world_per_meter = MERCATOR_WORLD_SIZE
        * MercatorCoordinate::from_lng_lat(LngLat::new(13.0, 52.0), 0.0)
            .meter_in_mercator_coordinate_units();
    let first_cascade_far_bound = (world_per_meter * FIRST_CASCADE_FAR_METERS) as f32;
    let maximum_distance = (world_per_meter * SHADOW_MAX_DISTANCE_METERS) as f32;
    let minimum_distance = (world_per_meter * SHADOW_MIN_DISTANCE_METERS) as f32;

    let directional_light = DirectionalLight {
        shadows_enabled: true,
        shadow_depth_bias: SHADOW_DEPTH_BIAS,
        shadow_normal_bias: SHADOW_NORMAL_BIAS,
        ..default()
    };

    commands.entity(map_view_eid).with_children(|parent| {
        parent.spawn((
            Name::new("Sun Directional Light"),
            directional_light,
            CascadeShadowConfigBuilder {
                num_cascades: 3,
                first_cascade_far_bound,
                maximum_distance,
                minimum_distance,
                ..default()
            }
            .build(),
            CellCoord::default(),
            Visibility::Inherited,
            CelestialDirectionalLight {
                kind: CelestialDirectionalLightKind::Sun,
            },
            RenderLayers::from_layers(&[MAP_VIEW_COLOR_RENDER_LAYER]),
        ));

        parent.spawn((
            Name::new("Moon Directional Light"),
            directional_light,
            CascadeShadowConfigBuilder {
                num_cascades: 3,
                first_cascade_far_bound,
                maximum_distance,
                minimum_distance,
                ..default()
            }
            .build(),
            CellCoord::default(),
            Visibility::Inherited,
            CelestialDirectionalLight {
                kind: CelestialDirectionalLightKind::Moon,
            },
            RenderLayers::from_layers(&[MAP_VIEW_COLOR_RENDER_LAYER]),
        ));
    });

    let ambient_light = AmbientLight { ..default() };

    if let Some(debug_eid) = app_windows.debug_eid {
        commands.entity(map_view_eid).with_children(|parent| {
            parent.spawn((
                Transform::default(),
                CellCoord::default(),
                Camera3d::default(),
                Camera {
                    clear_color: ClearColorConfig::Custom(Color::NONE),
                    output_mode: CameraOutputMode::Write {
                        clear_color: ClearColorConfig::Custom(Color::NONE),
                        blend_state: None,
                    },
                    ..default()
                },
                ambient_light.clone(),
                RenderTarget::Window(WindowRef::Entity(debug_eid)),
                GameViewCamera,
                RenderLayers::from_layers(&[
                    MAP_VIEW_COLOR_RENDER_LAYER,
                    MAP_VIEW_NON_TERRAIN_RENDER_LAYER,
                ]),
                MapViewCamera {
                    maplibre_int_eid: maplibre_integration_eid,
                },
            ));
        });
    }

    commands.entity(map_view_eid).with_children(|parent| {
        parent.spawn((
            Name::new("MapLibre Texture Camera"),
            Transform::default(),
            CellCoord::default(),
            Camera3d::default(),
            FloatingOrigin,
            SkyboxShaderCamera {
                layers: RenderLayers::layer(MAP_VIEW_NON_TERRAIN_RENDER_LAYER),
            },
            Camera {
                order: 1,
                clear_color: ClearColorConfig::None,
                output_mode: CameraOutputMode::Write {
                    clear_color: ClearColorConfig::None,
                    blend_state: None,
                },
                ..default()
            },
            RenderTarget::Window(WindowRef::Entity(
                app_windows
                    .texture_eid
                    .expect("map texture offscreen window to be set"),
            )),
            RenderLayers::from_layers(&[
                MAP_VIEW_COLOR_RENDER_LAYER,
                MAP_VIEW_NON_TERRAIN_RENDER_LAYER,
            ]),
            ambient_light.clone(),
            MapTextureAtlasCamera {
                slot: MapTextureAtlasSlot::Overlay,
            },
            MapViewCamera {
                maplibre_int_eid: maplibre_integration_eid,
            },
        ));
    });

    commands.entity(map_view_eid).with_children(|parent| {
        parent.spawn((
            Name::new("MapLibre Terrain Texture Camera"),
            Transform::default(),
            CellCoord::default(),
            Camera3d::default(),
            Camera {
                clear_color: ClearColorConfig::Custom(Color::NONE),
                output_mode: CameraOutputMode::Write {
                    clear_color: ClearColorConfig::Custom(Color::NONE),
                    blend_state: None,
                },
                ..default()
            },
            RenderTarget::Window(WindowRef::Entity(
                app_windows
                    .texture_eid
                    .expect("map texture offscreen window to be set"),
            )),
            RenderLayers::from_layers(&[
                MAP_VIEW_COLOR_RENDER_LAYER,
                MAP_VIEW_TERRAIN_RENDER_LAYER,
            ]),
            ambient_light,
            MapTextureAtlasCamera {
                slot: MapTextureAtlasSlot::Terrain,
            },
            MapViewCamera {
                maplibre_int_eid: maplibre_integration_eid,
            },
        ));
    });
}

fn sync_map_texture_atlas_camera_viewports(
    mut cameras: Query<(&mut Camera, &MapTextureAtlasCamera, &RenderTarget)>,
    windows: Query<&Window>,
) {
    for (mut camera, atlas_camera, render_target) in &mut cameras {
        let RenderTarget::Window(WindowRef::Entity(window_eid)) = render_target else {
            continue;
        };
        let Ok(window) = windows.get(*window_eid) else {
            continue;
        };

        let atlas_size = window.physical_size();
        let slot_width = atlas_size.x / MAP_TEXTURE_ATLAS_COLUMNS;
        if slot_width == 0 || atlas_size.y == 0 {
            camera.viewport = None;
            continue;
        }

        let viewport = Viewport {
            physical_position: UVec2::new(slot_width * atlas_camera.slot.index(), 0),
            physical_size: UVec2::new(slot_width, atlas_size.y),
            depth: 0.0..1.0,
        };

        if camera.viewport.as_ref().is_none_or(|current| {
            current.physical_position != viewport.physical_position
                || current.physical_size != viewport.physical_size
        }) {
            camera.viewport = Some(viewport);
        }
    }
}
