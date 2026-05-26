use crate::app::common::debug_gizmos::DebugGizmosPlugin;
use crate::app::common::editor::EditorPlugin;
use crate::app::common::materials::MaterialsPlugin;
use crate::app::common::settings::SettingsPlugin;
use crate::app::instance_management::InstanceManagementPlugin;
use crate::app::instance_management::commands::InstanceCommandQueue;
use crate::app::map::MapPlugin;
use crate::app::map::core::spawn_map_view;
use crate::app::maplibre_gl_js::MaplibreGlJsPlugin;
use crate::app::maplibre_gl_js::integration::MaplibreMapIntegration;
use crate::app::offscreen_window_handle::OffscreenWindowHandle;
use bevy::app::PluginsState;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput, NativeKey, NativeKeyCode};
use bevy::input::mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel};
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
    CompositeAlphaMode, CursorEntered, CursorLeft, CursorMoved, ExitCondition, PresentMode,
    PrimaryWindow, RawHandleWrapper, Window, WindowEvent as BevyWindowEvent, WindowFocused,
    WindowPlugin, WindowResized, WindowResolution, WindowScaleFactorChanged, WindowWrapper,
};
use bevy_inspector_egui::bevy_egui::{EguiGlobalSettings, EguiPlugin};
use bevy_winit::WinitPlugin;
use big_space::plugin::BigSpaceDefaultPlugins;
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use tracing::info;
use wasm_bindgen::prelude::wasm_bindgen;
use web_sys::OffscreenCanvas;

#[wasm_bindgen]
pub struct BevyInstance {
    inner: Rc<BevyInstanceInner>,
}

pub(crate) struct BevyInstanceInner {
    managed_app: RefCell<ManagedBevyApp>,
    command_queue: InstanceCommandQueue,
}

struct ManagedBevyApp {
    pub app: Option<App>,
    plugins_cleaned: bool,
}

#[derive(Clone, Resource)]
pub struct AppWindows {
    pub debug: Option<Entity>,
    pub texture: Option<Entity>,
}

#[derive(Resource)]
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

thread_local! {
    static INITIALIZED: RefCell<bool> = const { RefCell::new(false) };
}

#[wasm_bindgen]
pub fn initialize() {
    let initialized = INITIALIZED.with(|initialized| {
        let prev = *initialized.borrow();
        if !prev {
            *initialized.borrow_mut() = true;
        }
        prev
    });

    if !initialized {
        console_error_panic_hook::set_once();
        let mut app = App::new();

        // Log plugin only performs settings global logger and subscribers,
        // initializing only once to avoid errors on repeat
        app.add_plugins(LogPlugin {
            filter: "info,wgpu_core=warn,wgpu_hal=warn".into(),
            ..default()
        });
    }
}

#[wasm_bindgen]
impl BevyInstance {
    #[wasm_bindgen(constructor)]
    pub fn new(debug_canvas: OffscreenCanvas, texture_canvas: OffscreenCanvas) -> Self {
        initialize();

        let command_queue = InstanceCommandQueue::default();
        let mut app = App::new();
        app.add_plugins((
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        canvas: None,
                        title: "Debug Offscreen Window".to_string(),
                        resolution: WindowResolution::new(
                            debug_canvas.width(),
                            debug_canvas.height(),
                        ),
                        present_mode: PresentMode::AutoNoVsync,
                        transparent: true,
                        composite_alpha_mode: CompositeAlphaMode::PreMultiplied,
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
                .disable::<TransformPlugin>(),
            MaterialsPlugin,
            BigSpaceDefaultPlugins,
            EguiPlugin::default(),
            SettingsPlugin {},
            DebugGizmosPlugin,
            EditorPlugin {},
            MaplibreGlJsPlugin,
            MapPlugin,
            InstanceManagementPlugin {
                command_queue: command_queue.clone(),
            },
            ExtractResourcePlugin::<AppWindows>::default(),
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
            debug: None,
            texture: None,
        });

        app.insert_non_send_resource(OffscreenCanvases {
            debug: debug_canvas,
            texture: texture_canvas,
        });

        app.add_systems(PreStartup, setup_offscreen_windows);
        app.add_systems(PreUpdate, setup_map_for_integration);

        Self {
            inner: Rc::new(BevyInstanceInner {
                managed_app: RefCell::new(ManagedBevyApp {
                    app: Some(app),
                    plugins_cleaned: false,
                }),
                command_queue,
            }),
        }
    }
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
    if app_windows.debug.is_none()
        && let Some(entity) = primary_windows.iter().next()
    {
        commands.entity(entity).insert(raw_handle(&canvases.debug));
        app_windows.debug = Some(entity);
    }

    if app_windows.texture.is_none() {
        let entity = commands
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
        app_windows.texture = Some(entity);
    }

    info!("Setup offscreen Bevy windows");
}

fn release_inactive_debug_window_surface(
    offscreen_windows: Res<AppWindows>,
    mut extracted_windows: ResMut<ExtractedWindows>,
) {
    let Some(debug_window) = offscreen_windows.debug else {
        return;
    };
    let Some(window) = extracted_windows.get_mut(&debug_window) else {
        return;
    };

    if window.swap_chain_texture.is_some() {
        window.present();
        window.needs_initial_present = false;
    }
}

#[wasm_bindgen]
impl BevyInstance {
    pub fn tick(&self) -> Result<(), String> {
        let mut managed_app = self.inner.managed_app.borrow_mut();

        if !managed_app.plugins_cleaned {
            let Some(app) = managed_app.app.as_mut() else {
                return Err("Bevy instance is not mounted".to_string());
            };
            if !finish_app_plugins_if_ready(app) {
                return Ok(());
            }
            managed_app.plugins_cleaned = true;
        }

        let Some(app) = managed_app.app.as_mut() else {
            return Err("Bevy instance is not mounted".to_string());
        };
        app.update();
        Ok(())
    }

    pub fn resize(
        &self,
        debug_width: u32,
        debug_height: u32,
        map_width: u32,
        map_height: u32,
        scale_factor: f32,
    ) -> Result<(), String> {
        self.with_app_world(|world| {
            let app_windows = world.resource::<AppWindows>().clone();
            let (Some(debug), Some(texture)) = (app_windows.debug, app_windows.texture) else {
                return;
            };
            resize_window(world, debug, debug_width, debug_height, scale_factor);
            resize_window(world, texture, map_width, map_height, scale_factor);
        })
    }

    pub fn forward_focus(&self, focused: bool) -> Result<(), String> {
        self.with_debug_window(|world, window| {
            let event = WindowFocused { window, focused };
            world.write_message(event.clone());
            world.write_message(BevyWindowEvent::WindowFocused(event));
        })
    }

    pub fn forward_cursor_entered(&self) -> Result<(), String> {
        self.with_debug_window(|world, window| {
            let event = CursorEntered { window };
            world.write_message(event.clone());
            world.write_message(BevyWindowEvent::CursorEntered(event));
        })
    }

    pub fn forward_cursor_left(&self) -> Result<(), String> {
        self.with_debug_window(|world, window| {
            if let Some(mut window_component) = world.get_mut::<Window>(window) {
                window_component.set_cursor_position(None);
            }

            let event = CursorLeft { window };
            world.write_message(event.clone());
            world.write_message(BevyWindowEvent::CursorLeft(event));
        })
    }

    pub fn forward_cursor_moved(
        &self,
        x: f32,
        y: f32,
        delta_x: f32,
        delta_y: f32,
    ) -> Result<(), String> {
        self.with_debug_window(|world, window| {
            let position = Vec2::new(x, y);
            let delta = Vec2::new(delta_x, delta_y);

            if let Some(mut window_component) = world.get_mut::<Window>(window) {
                window_component.set_cursor_position(Some(position));
            }

            let event = CursorMoved {
                window,
                position,
                delta: Some(delta),
            };
            world.write_message(event.clone());
            world.write_message(BevyWindowEvent::CursorMoved(event));
            world.write_message(MouseMotion { delta });
            world.write_message(BevyWindowEvent::MouseMotion(MouseMotion { delta }));
        })
    }

    pub fn forward_mouse_button(&self, button: i16, pressed: bool) -> Result<(), String> {
        self.with_debug_window(|world, window| {
            let event = MouseButtonInput {
                button: web_mouse_button(button),
                state: button_state(pressed),
                window,
            };
            world.write_message(event);
            world.write_message(BevyWindowEvent::MouseButtonInput(event));
        })
    }

    pub fn forward_mouse_wheel(
        &self,
        delta_x: f32,
        delta_y: f32,
        delta_mode: u32,
    ) -> Result<(), String> {
        self.with_debug_window(|world, window| {
            let event = MouseWheel {
                unit: if delta_mode == 1 {
                    MouseScrollUnit::Line
                } else {
                    MouseScrollUnit::Pixel
                },
                x: delta_x,
                y: -delta_y,
                window,
            };
            world.write_message(event);
            world.write_message(BevyWindowEvent::MouseWheel(event));
        })
    }

    pub fn forward_keyboard_input(
        &self,
        code: String,
        key: String,
        pressed: bool,
        repeat: bool,
    ) -> Result<(), String> {
        self.with_debug_window(|world, window| {
            let logical_key = web_logical_key(&key);
            let text = match (&logical_key, pressed) {
                (Key::Character(text), true) => Some(text.clone()),
                _ => None,
            };
            let event = KeyboardInput {
                key_code: web_key_code(&code),
                logical_key,
                state: button_state(pressed),
                text,
                repeat,
                window,
            };
            world.write_message(event.clone());
            world.write_message(BevyWindowEvent::KeyboardInput(event));
        })
    }
}

fn finish_app_plugins_if_ready(app: &mut App) -> bool {
    match app.plugins_state() {
        PluginsState::Adding => false,
        PluginsState::Ready => {
            app.finish();
            app.cleanup();
            true
        }
        PluginsState::Finished => {
            app.cleanup();
            true
        }
        PluginsState::Cleaned => true,
    }
}

fn setup_map_for_integration(
    mut commands: Commands,
    windows: Res<AppWindows>,
    integrations: Query<(Entity, &MaplibreMapIntegration), Added<MaplibreMapIntegration>>,
) {
    let (Some(debug), Some(texture)) = (windows.debug, windows.texture) else {
        return;
    };
    let app_windows = AppWindows {
        debug: Some(debug),
        texture: Some(texture),
    };
    for (int_entity, _) in integrations.iter() {
        spawn_map_view(&mut commands, int_entity, &app_windows);
    }
}

impl BevyInstance {
    pub(crate) fn enqueue(
        &self,
        command: impl FnOnce(&mut World) + Send + 'static,
    ) -> Result<(), String> {
        self.inner.enqueue(command)
    }

    pub(crate) fn weak_inner(&self) -> Weak<BevyInstanceInner> {
        Rc::downgrade(&self.inner)
    }

    fn with_app_world(&self, f: impl FnOnce(&mut World)) -> Result<(), String> {
        self.inner.with_app_world(f)
    }

    fn with_debug_window(&self, f: impl FnOnce(&mut World, Entity)) -> Result<(), String> {
        self.with_app_world(|world| {
            if let Some(debug) = world.resource::<AppWindows>().debug {
                f(world, debug);
            }
        })
    }
}

impl BevyInstanceInner {
    pub(crate) fn enqueue(
        &self,
        command: impl FnOnce(&mut World) + Send + 'static,
    ) -> Result<(), String> {
        if self.managed_app.borrow().app.is_none() {
            return Err("Bevy instance is not mounted".to_string());
        }

        self.command_queue.enqueue(command);
        Ok(())
    }

    fn with_app_world(&self, f: impl FnOnce(&mut World)) -> Result<(), String> {
        let mut managed_app = self.managed_app.borrow_mut();
        let Some(app) = managed_app.app.as_mut() else {
            return Err("Bevy instance is not mounted".to_string());
        };
        f(app.world_mut());
        Ok(())
    }
}

impl Drop for BevyInstance {
    fn drop(&mut self) {
        self.inner.managed_app.borrow_mut().app.take();
        self.inner.command_queue.clear();
        info!("Dropped Bevy instance");
    }
}

fn resize_window(world: &mut World, entity: Entity, width: u32, height: u32, scale_factor: f32) {
    let Some((scale_factor_changed, resized)) =
        world.get_mut::<Window>(entity).map(|mut window| {
            let scale_factor = scale_factor.max(1.0);
            let scale_factor_changed = (window.scale_factor() - scale_factor).abs() > f32::EPSILON;
            let size_changed =
                window.physical_width() != width || window.physical_height() != height;

            if !scale_factor_changed && !size_changed {
                return (false, None);
            }

            window.resolution.set_scale_factor(scale_factor);
            window.resolution.set_physical_resolution(width, height);

            let resized = WindowResized {
                window: entity,
                width: window.width(),
                height: window.height(),
            };
            (scale_factor_changed, Some(resized))
        })
    else {
        return;
    };

    if scale_factor_changed {
        let event = WindowScaleFactorChanged {
            window: entity,
            scale_factor: scale_factor.max(1.0) as f64,
        };
        world.write_message(event.clone());
        world.write_message(BevyWindowEvent::WindowScaleFactorChanged(event));
    }

    if let Some(event) = resized {
        world.write_message(event.clone());
        world.write_message(BevyWindowEvent::WindowResized(event));
    }
}

fn button_state(pressed: bool) -> ButtonState {
    if pressed {
        ButtonState::Pressed
    } else {
        ButtonState::Released
    }
}

fn web_mouse_button(button: i16) -> MouseButton {
    match button {
        0 => MouseButton::Left,
        1 => MouseButton::Middle,
        2 => MouseButton::Right,
        3 => MouseButton::Back,
        4 => MouseButton::Forward,
        other => MouseButton::Other(other as u16),
    }
}

fn web_logical_key(key: &str) -> Key {
    match key {
        "Alt" => Key::Alt,
        "Backspace" => Key::Backspace,
        "Control" => Key::Control,
        "Delete" => Key::Delete,
        "Enter" => Key::Enter,
        "Escape" => Key::Escape,
        "Meta" => Key::Meta,
        "Shift" => Key::Shift,
        "Tab" => Key::Tab,
        "ArrowDown" => Key::ArrowDown,
        "ArrowLeft" => Key::ArrowLeft,
        "ArrowRight" => Key::ArrowRight,
        "ArrowUp" => Key::ArrowUp,
        "F1" => Key::F1,
        "F2" => Key::F2,
        "F3" => Key::F3,
        "F4" => Key::F4,
        "F5" => Key::F5,
        "F6" => Key::F6,
        "F7" => Key::F7,
        "F8" => Key::F8,
        "F9" => Key::F9,
        "F10" => Key::F10,
        "F11" => Key::F11,
        "F12" => Key::F12,
        "" => Key::Unidentified(NativeKey::Unidentified),
        text => Key::Character(text.into()),
    }
}

fn web_key_code(code: &str) -> KeyCode {
    match code {
        "Backspace" => KeyCode::Backspace,
        "Delete" => KeyCode::Delete,
        "Enter" => KeyCode::Enter,
        "Escape" => KeyCode::Escape,
        "F1" => KeyCode::F1,
        "F2" => KeyCode::F2,
        "F3" => KeyCode::F3,
        "F4" => KeyCode::F4,
        "F5" => KeyCode::F5,
        "F6" => KeyCode::F6,
        "F7" => KeyCode::F7,
        "F8" => KeyCode::F8,
        "F9" => KeyCode::F9,
        "F10" => KeyCode::F10,
        "F11" => KeyCode::F11,
        "F12" => KeyCode::F12,
        "KeyA" => KeyCode::KeyA,
        "KeyB" => KeyCode::KeyB,
        "KeyC" => KeyCode::KeyC,
        "KeyD" => KeyCode::KeyD,
        "KeyE" => KeyCode::KeyE,
        "KeyF" => KeyCode::KeyF,
        "KeyG" => KeyCode::KeyG,
        "KeyH" => KeyCode::KeyH,
        "KeyI" => KeyCode::KeyI,
        "KeyJ" => KeyCode::KeyJ,
        "KeyK" => KeyCode::KeyK,
        "KeyL" => KeyCode::KeyL,
        "KeyM" => KeyCode::KeyM,
        "KeyN" => KeyCode::KeyN,
        "KeyO" => KeyCode::KeyO,
        "KeyP" => KeyCode::KeyP,
        "KeyQ" => KeyCode::KeyQ,
        "KeyR" => KeyCode::KeyR,
        "KeyS" => KeyCode::KeyS,
        "KeyT" => KeyCode::KeyT,
        "KeyU" => KeyCode::KeyU,
        "KeyV" => KeyCode::KeyV,
        "KeyW" => KeyCode::KeyW,
        "KeyX" => KeyCode::KeyX,
        "KeyY" => KeyCode::KeyY,
        "KeyZ" => KeyCode::KeyZ,
        "Space" => KeyCode::Space,
        "Tab" => KeyCode::Tab,
        "ArrowDown" => KeyCode::ArrowDown,
        "ArrowLeft" => KeyCode::ArrowLeft,
        "ArrowRight" => KeyCode::ArrowRight,
        "ArrowUp" => KeyCode::ArrowUp,
        "ShiftLeft" => KeyCode::ShiftLeft,
        "ShiftRight" => KeyCode::ShiftRight,
        "ControlLeft" => KeyCode::ControlLeft,
        "ControlRight" => KeyCode::ControlRight,
        "AltLeft" => KeyCode::AltLeft,
        "AltRight" => KeyCode::AltRight,
        "MetaLeft" => KeyCode::SuperLeft,
        "MetaRight" => KeyCode::SuperRight,
        _ => KeyCode::Unidentified(NativeKeyCode::Unidentified),
    }
}
