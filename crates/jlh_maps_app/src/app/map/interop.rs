use crate::app::main::BevyInstance;
use crate::app::map::camera::MapViewCameraSettings as MapViewCameraSettingsBevy;
use crate::app::map::core::MapViewSettings as MapViewSettingsBevy;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
pub struct MapViewSettings {
    pub enable_window_cameras: bool,
    pub enable_buildings: bool,
    pub enable_waters: bool,
    pub enable_shadows: bool,
    pub sun_azimuth_degrees: f32,
    pub sun_elevation_degrees: f32,
}

#[wasm_bindgen]
impl MapViewSettings {
    #[wasm_bindgen(constructor)]
    pub fn new(
        enable_window_cameras: bool,
        enable_buildings: bool,
        enable_waters: bool,
        enable_shadows: bool,
        sun_azimuth_degrees: f32,
        sun_elevation_degrees: f32,
    ) -> Self {
        Self {
            enable_window_cameras,
            enable_buildings,
            enable_waters,
            enable_shadows,
            sun_azimuth_degrees,
            sun_elevation_degrees,
        }
    }
}

impl From<MapViewSettings> for MapViewSettingsBevy {
    fn from(val: MapViewSettings) -> Self {
        MapViewSettingsBevy {
            enable_buildings: val.enable_buildings,
            enable_waters: val.enable_waters,
            enable_window_cameras: val.enable_window_cameras,
            enable_shadows: val.enable_shadows,
            sun_azimuth_degrees: val.sun_azimuth_degrees,
            sun_elevation_degrees: val.sun_elevation_degrees,
        }
    }
}

#[wasm_bindgen]
pub struct MapViewCameraSettings {
    pub enable_color_grading: bool,
    pub enable_tonemapping: bool,
    pub enable_msaa: bool,
    pub enable_ssao: bool,
    pub enable_taa: bool,
}

#[wasm_bindgen]
impl MapViewCameraSettings {
    #[wasm_bindgen(constructor)]
    pub fn new(
        enable_color_grading: bool,
        enable_tonemapping: bool,
        enable_msaa: bool,
        enable_ssao: bool,
        enable_taa: bool,
    ) -> Self {
        Self {
            enable_color_grading,
            enable_tonemapping,
            enable_msaa,
            enable_ssao,
            enable_taa,
        }
    }
}

impl From<MapViewCameraSettings> for MapViewCameraSettingsBevy {
    fn from(val: MapViewCameraSettings) -> Self {
        MapViewCameraSettingsBevy {
            enable_color_grading: val.enable_color_grading,
            enable_tonemapping: val.enable_tonemapping,
            enable_msaa: val.enable_msaa,
            enable_ssao: val.enable_ssao,
            enable_taa: val.enable_taa,
        }
    }
}

#[wasm_bindgen]
impl BevyInstance {
    pub fn set_map_view_settings(&self, settings: MapViewSettings) -> Result<(), String> {
        self.enqueue(move |world| {
            *world.get_resource_mut::<MapViewSettingsBevy>().unwrap() = settings.into();
        })
    }

    pub fn set_map_view_camera_settings(
        &self,
        settings: MapViewCameraSettings,
    ) -> Result<(), String> {
        self.enqueue(move |world| {
            *world
                .get_resource_mut::<MapViewCameraSettingsBevy>()
                .unwrap() = settings.into();
        })
    }
}
