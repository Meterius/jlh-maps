use crate::app::common::debug_gizmos::DebugGizmosPlugin;
use crate::app::common::editor::EditorPlugin;
use crate::app::common::materials::MaterialsPlugin;
use crate::app::common::settings::SettingsPlugin;
#[cfg(feature = "wasm-threads")]
use crate::app::common::wasm_threaded_app::WasmThreadedAppPlugin;
use crate::app::instance::BevyInstance;
use crate::app::map::MapPlugin;
use crate::app::map::core::spawn_map_view;
use crate::app::maplibre_gl_js::MaplibreGlJsPlugin;
use crate::app::maplibre_gl_js::integration::MaplibreMapIntegration;
use crate::app::offscreen_window_handle::OffscreenWindowHandle;
use crate::app::window_events::WindowInstanceRef;
use bevy::asset::{AssetMetaCheck, AssetPlugin};
use bevy::audio::AudioPlugin;
use bevy::light::DirectionalLightShadowMap;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_resource::WgpuFeatures;
use bevy::render::settings::{Backends, RenderCreation, WgpuSettings};
use bevy::render::view::ExtractedWindows;
use bevy::render::{Render, RenderApp};
use bevy::render::{RenderPlugin, RenderSystems};
use bevy::window::{
    CompositeAlphaMode, ExitCondition, PresentMode, PrimaryWindow, RawHandleWrapper, Window,
    WindowPlugin, WindowResolution, WindowWrapper,
};
use bevy_inspector_egui::bevy_egui::{EguiGlobalSettings, EguiPlugin};
use bevy_winit::WinitPlugin;
use big_space::plugin::BigSpaceDefaultPlugins;
use tracing::info;
use wasm_bindgen::prelude::wasm_bindgen;
use web_sys::OffscreenCanvas;

#[wasm_bindgen]
impl BevyInstance {
    pub fn get_debug_window(&self) -> Result<Option<WindowInstanceRef>, String> {
        let window_eid = self.execute(|world| {
            world
                .get_resource::<AppWindows>()
                .and_then(|windows| windows.debug_eid)
        })?;

        Ok(window_eid.map(|window_eid| WindowInstanceRef {
            instance: self.weak_inner(),
            window_eid,
        }))
    }

    pub fn get_texture_window(&self) -> Result<Option<WindowInstanceRef>, String> {
        let window_eid = self.execute(|world| {
            world
                .get_resource::<AppWindows>()
                .and_then(|windows| windows.texture_eid)
        })?;

        Ok(window_eid.map(|window_eid| WindowInstanceRef {
            instance: self.weak_inner(),
            window_eid,
        }))
    }
}

#[derive(Clone, Resource)]
pub struct AppWindows {
    pub debug_eid: Option<Entity>,
    pub texture_eid: Option<Entity>,
}

pub struct OffscreenCanvases {
    pub debug: OffscreenCanvas,
    pub texture: OffscreenCanvas,
}

impl ExtractResource for AppWindows {
    type Source = Self;

    fn extract_resource(source: &Self::Source) -> Self {
        source.clone()
    }
}

pub fn setup_app(
    app: &mut App,
    debug_canvas: OffscreenCanvas,
    texture_canvas: OffscreenCanvas,
    asset_base_url: String,
) {
    app.add_plugins((
        DefaultPlugins
            .set(AssetPlugin {
                file_path: asset_base_url,
                meta_check: AssetMetaCheck::Never,
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    canvas: None,
                    title: "Debug Offscreen Window".to_string(),
                    resolution: WindowResolution::new(debug_canvas.width(), debug_canvas.height()),
                    present_mode: PresentMode::AutoNoVsync,
                    transparent: true,
                    composite_alpha_mode: CompositeAlphaMode::PreMultiplied,
                    // used by winit which is not used, if enabled causes bevy_egui
                    // to try to install hidden input in DOM which is unavailable in workers causing
                    // a panic
                    prevent_default_event_handling: false,
                    ..default()
                }),
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .set(RenderPlugin {
                render_creation: RenderCreation::Automatic(WgpuSettings {
                    features: WgpuFeatures::default(),
                    backends: Some(Backends::BROWSER_WEBGPU),
                    ..default()
                }),
                ..default()
            })
            .disable::<LogPlugin>()
            .disable::<WinitPlugin>()
            .disable::<TransformPlugin>()
            .disable::<GilrsPlugin>()
            .disable::<AudioPlugin>(),
        MaterialsPlugin,
        BigSpaceDefaultPlugins,
        EguiPlugin::default(),
        SettingsPlugin {},
        DebugGizmosPlugin,
        EditorPlugin {},
        MaplibreGlJsPlugin,
        MapPlugin,
        ExtractResourcePlugin::<AppWindows>::default(),
        #[cfg(feature = "wasm-threads")]
        WasmThreadedAppPlugin,
    ));

    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app.add_systems(
            Render,
            release_inactive_debug_window_surface
                .in_set(RenderSystems::Render)
                .after(bevy::render::renderer::render_system),
        );
    }

    app.insert_resource(ClearColor(Color::NONE));

    app.insert_resource(DirectionalLightShadowMap { size: 4096 });

    app.insert_resource(EguiGlobalSettings {
        // requires winit which is disabled as windows need manual management
        enable_ime: false,
        ..default()
    });

    app.insert_resource(AppWindows {
        debug_eid: None,
        texture_eid: None,
    });

    app.insert_non_send_resource(OffscreenCanvases {
        debug: debug_canvas,
        texture: texture_canvas,
    });

    app.add_systems(PreStartup, setup_offscreen_windows);
    app.add_systems(PreUpdate, setup_map_for_integration);
}

fn raw_handle(canvas: &OffscreenCanvas) -> RawHandleWrapper {
    RawHandleWrapper::new(&WindowWrapper::new(OffscreenWindowHandle::new(canvas))).expect(
        "to create offscreen raw handle wrapper. If this fails, multiple threads are trying to access the same canvas!",
    )
}

fn setup_offscreen_windows(
    mut commands: Commands,
    canvases: NonSend<OffscreenCanvases>,
    mut app_windows: ResMut<AppWindows>,
    primary_windows: Query<Entity, (Added<Window>, With<PrimaryWindow>)>,
) {
    if app_windows.debug_eid.is_none()
        && let Some(window_eid) = primary_windows.iter().next()
    {
        commands
            .entity(window_eid)
            .insert(raw_handle(&canvases.debug));
        app_windows.debug_eid = Some(window_eid);
    }

    if app_windows.texture_eid.is_none() {
        let texture_window_eid = commands
            .spawn((
                Window {
                    canvas: None,
                    title: "Map Texture Offscreen Window".to_string(),
                    resolution: WindowResolution::new(
                        canvases.texture.width(),
                        canvases.texture.height(),
                    ),
                    present_mode: PresentMode::AutoNoVsync,
                    transparent: true,
                    composite_alpha_mode: CompositeAlphaMode::PreMultiplied,
                    ..default()
                },
                raw_handle(&canvases.texture),
            ))
            .id();
        app_windows.texture_eid = Some(texture_window_eid);
    }

    info!("Setup offscreen Bevy windows");
}

fn release_inactive_debug_window_surface(
    offscreen_windows: Res<AppWindows>,
    mut extracted_windows: ResMut<ExtractedWindows>,
) {
    let Some(debug_window_eid) = offscreen_windows.debug_eid else {
        return;
    };
    let Some(window) = extracted_windows.get_mut(&debug_window_eid) else {
        return;
    };

    if window.swap_chain_texture.is_some() {
        window.present();
        window.needs_initial_present = false;
    }
}

fn setup_map_for_integration(
    mut commands: Commands,
    windows: Res<AppWindows>,
    integrations: Query<(Entity, &MaplibreMapIntegration), Added<MaplibreMapIntegration>>,
) {
    let (Some(debug_eid), Some(texture_eid)) = (windows.debug_eid, windows.texture_eid) else {
        return;
    };
    let app_windows = AppWindows {
        debug_eid: Some(debug_eid),
        texture_eid: Some(texture_eid),
    };
    for (integration_eid, _) in integrations.iter() {
        spawn_map_view(&mut commands, integration_eid, &app_windows);
    }
}
