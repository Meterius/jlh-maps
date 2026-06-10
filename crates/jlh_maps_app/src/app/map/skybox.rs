use crate::app::common::skybox_shader::{
    SkyboxShaderMaterial, SkyboxShaderParams, SkyboxShaderPlugin,
};
use crate::app::map::core::MapViewSettings;
use crate::app::map::utils::sky_model::{
    light_travel_direction_from_az_el_degrees, lighting_from_sun_elevation,
};
use bevy::prelude::*;

pub struct SkyboxPlugin;

impl Plugin for SkyboxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(SkyboxShaderPlugin);
        app.add_systems(Update, sync_skybox_material);
    }
}

fn sync_skybox_material(
    mv_settings: Res<MapViewSettings>,
    skybox_mesh_materials: Query<&MeshMaterial3d<SkyboxShaderMaterial>>,
    mut skybox_materials: ResMut<Assets<SkyboxShaderMaterial>>,
) {
    for mat in skybox_mesh_materials {
        let Some(material) = skybox_materials.get_mut(&mat.0) else {
            continue;
        };

        let lighting = lighting_from_sun_elevation(
            mv_settings.sun_elevation_degrees,
            mv_settings.moon_elevation_degrees,
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

            // Add these to MapViewSettings if you want them configurable.
            haze: 0.25,
            exposure: 1.0,
        };
    }
}
