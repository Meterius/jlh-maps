use bevy::prelude::*;
use crate::app::common::skybox_shader::{SkyboxShaderMaterial, SkyboxShaderMesh, SkyboxShaderPlugin};
use crate::app::map::core::MapViewSettings;

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

        material.sun_direction = azimuth_elevation_to_dir4(
            mv_settings.sun_azimuth_degrees.to_radians(),
            mv_settings.sun_elevation_degrees.to_radians()
        );

        material.moon_direction = azimuth_elevation_to_dir4(
            mv_settings.moon_azimuth_degrees.to_radians(),
            mv_settings.moon_elevation_degrees.to_radians()
        );
    }
}

pub fn azimuth_elevation_to_dir4(azimuth: f32, elevation: f32) -> Vec4 {
    let horizontal = elevation.cos();

    Vec3::new(
        horizontal * azimuth.cos(),
        horizontal * azimuth.sin(),
        elevation.sin(),
    )
        .normalize_or_zero()
        .extend(0.0)
}