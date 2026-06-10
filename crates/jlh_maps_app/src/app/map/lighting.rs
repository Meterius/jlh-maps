use crate::app::map::camera::MapViewCamera;
use crate::app::map::core::MapViewSettings;
use crate::app::map::utils::sky_model::{
    light_travel_direction_from_az_el_degrees, lighting_from_sun_elevation,
};
use bevy::app::{Plugin, Update};
use bevy::light::{AmbientLight, DirectionalLight};
use bevy::math::{Vec3, VectorSpace};
use bevy::prelude::{Component, Transform, Visibility};
use bevy_ecs::change_detection::{DetectChanges, Res};
use bevy_ecs::prelude::{Query, With};

const SUN_MAX_ILLUMINANCE: f32 = 4_000.0;
const MOON_MAX_ILLUMINANCE: f32 = 300.0;
const AMBIENT_MAX_BRIGHTNESS: f32 = 1_100.0;
const AMBIENT_MIN_BRIGHTNESS: f32 = 800.0;

pub struct LightingPlugin;

impl Plugin for LightingPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_systems(Update, sync_map_lighting);
    }
}

pub enum CelestialDirectionalLightKind {
    Sun,
    Moon,
}

#[derive(Component)]
pub struct CelestialDirectionalLight {
    pub kind: CelestialDirectionalLightKind,
}

fn sync_map_lighting(
    mv_settings: Res<MapViewSettings>,
    mut directional_lights: Query<
        (
            &CelestialDirectionalLight,
            &mut DirectionalLight,
            &mut Transform,
            &mut Visibility,
        ),
        With<CelestialDirectionalLight>,
    >,
    mut ambients: Query<&mut AmbientLight, With<MapViewCamera>>,
) {
    let model = lighting_from_sun_elevation(
        mv_settings.sun_elevation_degrees,
        mv_settings.moon_elevation_degrees,
    );

    let sun_direction = map_light_direction(
        mv_settings.sun_azimuth_degrees,
        mv_settings.sun_elevation_degrees,
    );

    let moon_direction = map_light_direction(
        mv_settings.moon_azimuth_degrees,
        mv_settings.moon_elevation_degrees,
    );

    for (cel_light, mut light, mut transform, mut visibility) in directional_lights.iter_mut() {
        if !mv_settings.is_changed() && !light.is_added() {
            continue;
        }

        light.shadows_enabled = mv_settings.enable_shadows;
        light.color = match cel_light.kind {
            CelestialDirectionalLightKind::Sun => model.sun_color,
            CelestialDirectionalLightKind::Moon => model.moon_color,
        };

        let direction = match cel_light.kind {
            CelestialDirectionalLightKind::Sun => sun_direction,
            CelestialDirectionalLightKind::Moon => moon_direction,
        };

        if let Some(direction) = direction {
            *visibility = Visibility::Inherited;
            light.illuminance = match cel_light.kind {
                CelestialDirectionalLightKind::Sun => SUN_MAX_ILLUMINANCE * model.sun_intensity,
                CelestialDirectionalLightKind::Moon => MOON_MAX_ILLUMINANCE * model.moon_intensity,
            };
            *transform = Transform::default().looking_to(direction, Vec3::Z);
        } else {
            *visibility = Visibility::Hidden;
        }
    }

    for mut ambient in ambients.iter_mut() {
        if !mv_settings.is_changed() && !ambient.is_added() {
            continue;
        }

        ambient.color = model.ambient_color;
        ambient.brightness =
            AMBIENT_MIN_BRIGHTNESS.lerp(AMBIENT_MAX_BRIGHTNESS, model.ambient_intensity);
    }
}

fn map_light_direction(azimuth_degrees: f32, elevation_degrees: f32) -> Option<Vec3> {
    if elevation_degrees < 0.0 {
        return None;
    }

    Some(light_travel_direction_from_az_el_degrees(
        azimuth_degrees,
        elevation_degrees,
    ))
}
