use crate::app::common::skybox_shader::{
    SkyboxShaderHorizon, SkyboxShaderMaterial, SkyboxShaderParams, SkyboxShaderPlugin,
};
use crate::app::map::camera::maplibre_camera_to_center_distance_pixels;
use crate::app::map::core::MapViewSettings;
use crate::app::map::utils::sky_model::{
    light_travel_direction_from_az_el_degrees, lighting_from_sun_elevation,
};
use crate::app::maplibre_gl_js::types::MlView;
use bevy::prelude::*;

// Keep the MapLibre-specific horizon policy here so the WGSL only receives a
// generic screen-space horizon line. These values mirror:
// - maplibre-gl/src/geo/projection/mercator_utils.ts
//   `maxMercatorHorizonAngle` and `getMercatorHorizon`
// - maplibre-gl/src/webgl/program/sky_program.ts
//   `skyUniformValues`
const MAPLIBRE_MAX_MERCATOR_HORIZON_ANGLE_DEGREES: f64 = 89.25;
const MAPLIBRE_DEFAULT_SKY_HORIZON_BLEND: f32 = 0.5;
const HORIZON_SEAM_WIDTH_PX: f32 = 2.0;

pub struct SkyboxPlugin;

impl Plugin for SkyboxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(SkyboxShaderPlugin);
        app.add_systems(Update, sync_skybox_material);
    }
}

fn sync_skybox_material(
    mv_settings: Res<MapViewSettings>,
    ml_views: Query<&MlView>,
    skybox_mesh_materials: Query<&MeshMaterial3d<SkyboxShaderMaterial>>,
    mut skybox_materials: ResMut<Assets<SkyboxShaderMaterial>>,
) {
    let Ok(view) = ml_views.single() else {
        return;
    };
    let horizon = skybox_horizon_from_ml_view(view);

    for mat in skybox_mesh_materials {
        let Some(material) = skybox_materials.get_mut(&mat.0) else {
            continue;
        };

        let lighting = lighting_from_sun_elevation(
            mv_settings.sun_elevation_degrees,
            mv_settings.moon_elevation_degrees,
            mv_settings.disable_lighting_hue,
        );

        material.sky.sun_direction = -light_travel_direction_from_az_el_degrees(
            mv_settings.sun_azimuth_degrees,
            mv_settings.sun_elevation_degrees,
        )
        .extend(0.0);

        material.sky.moon_direction = -light_travel_direction_from_az_el_degrees(
            mv_settings.moon_azimuth_degrees,
            mv_settings.moon_elevation_degrees,
        )
        .extend(0.0);

        material.sky.sun_color = lighting
            .sun_color
            .to_linear()
            .with_alpha(lighting.sun_intensity)
            .to_vec4();

        material.sky.moon_color = lighting
            .moon_color
            .to_linear()
            .with_alpha(lighting.moon_intensity)
            .to_vec4();

        material.sky.ambient_color = lighting
            .ambient_color
            .to_linear()
            .with_alpha(lighting.ambient_intensity)
            .to_vec4();

        material.sky.params = SkyboxShaderParams {
            sun_elevation_degrees: mv_settings.sun_elevation_degrees,
            moon_elevation_degrees: mv_settings.moon_elevation_degrees,
            haze: 0.25,
            exposure: 1.0,
        };

        material.sky.horizon = horizon;
    }
}

fn skybox_horizon_from_ml_view(view: &MlView) -> SkyboxShaderHorizon {
    let width_px = view.width.max(1.0) as f32;
    let height_px = view.height.max(1.0) as f32;
    let horizon_y_top_px = maplibre_horizon_y_top_px(view.height, view.pitch);

    // MapLibre computes `u_sky_horizon_blend` as
    // `sky-horizon-blend * transform.height / 2 * pixelRatio`. `MlView.height`
    // is already the framebuffer height passed from `canvas.height`, so no
    // additional pixel-ratio term is needed here.
    let gradient_distance_px = (height_px * MAPLIBRE_DEFAULT_SKY_HORIZON_BLEND * 0.5).max(1.0);

    // MapLibre's no-roll Mercator sky uses a horizontal horizon line with a
    // normal toward positive WebGL `gl_FragCoord.y`. WGSL fragment coordinates
    // are top-left based in this render path, so the equivalent sky-side normal
    // points toward decreasing y.
    SkyboxShaderHorizon {
        position: Vec2::new(width_px * 0.5, horizon_y_top_px),
        normal: Vec2::new(0.0, -1.0),
        sky_gradient_distance_px: gradient_distance_px,
        ground_gradient_distance_px: gradient_distance_px,
        seam_width_px: HORIZON_SEAM_WIDTH_PX,
        _padding: 0.0,
    }
}

fn maplibre_horizon_y_top_px(height_px: f64, pitch_degrees: f64) -> f32 {
    let camera_to_center_px = maplibre_camera_to_center_distance_pixels(height_px);
    let pitch_to_level_radians = (90.0 - pitch_degrees).to_radians();
    let pitch_to_max_horizon_radians =
        (MAPLIBRE_MAX_MERCATOR_HORIZON_ANGLE_DEGREES - pitch_degrees).to_radians();

    // Port of MapLibre's `getMercatorHorizon`: the 0.85 factor simulates Earth
    // curvature in Mercator, while the 89.25-degree cap keeps the horizon
    // distance finite and avoids excessive far-plane/tile coverage.
    let mercator_horizon_px = camera_to_center_px
        * (pitch_to_level_radians.tan() * 0.85).min(pitch_to_max_horizon_radians.tan());

    // MapLibre stores the horizon from a bottom-left WebGL origin as
    // `height / 2 + mercator_horizon_px`; convert to the top-left framebuffer
    // y coordinate consumed by the WGSL skybox shader.
    (height_px * 0.5 - mercator_horizon_px) as f32
}
