use crate::app::common::editor::GameViewCamera;
use crate::app::main::AppWindows;
use crate::app::map::camera::MapViewCamera;
use crate::app::map::feature_layers::make_bucket_manager;
use crate::app::map::terrain::TerrainTileManager;
use crate::app::map::transform::MERCATOR_WORLD_SIZE;
use crate::app::maplibre_gl_js::utils::mercator_coordinate::{LngLat, MercatorCoordinate};
use bevy::camera::{CameraOutputMode, RenderTarget};
use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy::window::WindowRef;
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

pub const MAP_VIEW_COLOR_RENDER_LAYER: usize = 0;
pub const MAP_VIEW_DEPTH_RENDER_LAYER: usize = 1;

pub(super) struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MapViewSettings::default());

        app.add_systems(PreUpdate, sync_map_sun);
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
    pub sun_azimuth_degrees: f32,
    pub sun_elevation_degrees: f32,

    pub feature_visibility_distance: f32,
}

impl Default for MapViewSettings {
    fn default() -> Self {
        Self {
            enable_buildings: true,
            enable_waters: true,
            enable_trees: true,
            enable_shadows: true,
            sun_azimuth_degrees: 11.31,
            sun_elevation_degrees: 32.52,
            feature_visibility_distance: 10.0,
        }
    }
}

#[derive(Debug, Reflect, Component)]
struct MapViewShadowLight;

fn sync_map_sun(
    mv_settings: Res<MapViewSettings>,
    mut lights: Query<(&mut DirectionalLight, &mut Transform), With<MapViewShadowLight>>,
) {
    let direction = map_sun_direction(&mv_settings);

    for (mut light, mut transform) in lights.iter_mut() {
        light.shadows_enabled = mv_settings.enable_shadows;
        *transform = Transform::default().looking_to(direction, Vec3::Z);
    }
}

fn map_sun_direction(settings: &MapViewSettings) -> Vec3 {
    let azimuth = settings.sun_azimuth_degrees.to_radians();
    let elevation = settings.sun_elevation_degrees.clamp(0.0, 89.0).to_radians();
    let horizontal = elevation.cos();

    Vec3::new(
        horizontal * azimuth.cos(),
        horizontal * azimuth.sin(),
        -elevation.sin(),
    )
    .normalize_or_zero()
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

    commands.entity(map_view_eid).with_child((
        DirectionalLight {
            color: Color::WHITE,
            illuminance: 4000.,
            shadows_enabled: true,
            shadow_depth_bias: SHADOW_DEPTH_BIAS,
            shadow_normal_bias: SHADOW_NORMAL_BIAS,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: 3,
            first_cascade_far_bound,
            maximum_distance,
            minimum_distance,
            ..default()
        }
        .build(),
        Transform::default().looking_to(map_sun_direction(&MapViewSettings::default()), Vec3::Z),
        CellCoord::default(),
        MapViewShadowLight,
    ));

    let ambient_light = AmbientLight {
        color: Color::WHITE,
        brightness: 1100.0,
        ..default()
    };

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
            ambient_light,
            MapViewCamera {
                maplibre_int_eid: maplibre_integration_eid,
            },
        ));
    });
}
