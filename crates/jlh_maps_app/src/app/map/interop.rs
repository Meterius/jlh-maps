use crate::app::instance::BevyInstance;
use crate::app::map::camera::MapViewCameraSettings;
use crate::app::map::core::MapViewSettings;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
impl BevyInstance {
    pub fn set_map_view_settings(&self, settings: MapViewSettings) -> Result<(), String> {
        self.execute(move |world| {
            *world.get_resource_mut::<MapViewSettings>().unwrap() = settings;
        })
    }

    pub fn set_map_view_camera_settings(
        &self,
        settings: MapViewCameraSettings,
    ) -> Result<(), String> {
        self.execute(move |world| {
            *world.get_resource_mut::<MapViewCameraSettings>().unwrap() = settings;
        })
    }
}
