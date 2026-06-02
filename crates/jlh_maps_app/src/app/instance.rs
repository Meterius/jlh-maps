use crate::app::main::setup_app;
use crate::app::task_pool::AppTaskPool;
use crate::app::task_pool::AppTaskPoolPlugin;
use bevy::app::{App, PluginsState};
use bevy::log::LogPlugin;
use bevy::prelude;
use bevy::prelude::{World, default};
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use tracing::info;
use wasm_bindgen::prelude::wasm_bindgen;
use web_sys::OffscreenCanvas;

thread_local! {
    static INITIALIZED: RefCell<bool> = const { RefCell::new(false) };
}

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
pub struct BevyInstance {
    inner: Rc<BevyInstanceInner>,
}

pub(crate) struct BevyInstanceInner {
    managed_app: RefCell<Option<ManagedBevyApp>>,
}

struct ManagedBevyApp {
    pub app: App,
    plugins_cleaned: bool,
}

#[wasm_bindgen]
impl BevyInstance {
    #[wasm_bindgen(constructor)]
    pub fn new(debug_canvas: OffscreenCanvas, texture_canvas: OffscreenCanvas) -> Self {
        initialize();

        let mut app = App::new();
        app.add_plugins(AppTaskPoolPlugin {});
        setup_app(&mut app, debug_canvas, texture_canvas);

        Self {
            inner: Rc::new(BevyInstanceInner {
                managed_app: RefCell::new(Some(ManagedBevyApp {
                    app,
                    plugins_cleaned: false,
                })),
            }),
        }
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

#[wasm_bindgen]
impl BevyInstance {
    pub fn tick(&self) -> prelude::Result<(), String> {
        let mut managed_app_ref = self.inner.managed_app.borrow_mut();

        let Some(managed_app) = managed_app_ref.as_mut() else {
            return Err("Bevy instance is not mounted".to_string());
        };

        if !managed_app.plugins_cleaned {
            if !finish_app_plugins_if_ready(&mut managed_app.app) {
                return Ok(());
            }
            managed_app.plugins_cleaned = true;
        }

        managed_app.app.update();
        Ok(())
    }

    pub fn tick_secondary(&self) -> prelude::Result<(), String> {
        let mut managed_app_ref = self.inner.managed_app.borrow_mut();

        let Some(managed_app) = managed_app_ref.as_mut() else {
            return Err("Bevy instance is not mounted".to_string());
        };

        if managed_app.plugins_cleaned {
            let world = managed_app.app.world_mut();

            let Some(app_task_pool) = world.get_resource_mut::<AppTaskPool>() else {
                return Err("AppTaskPool resource not found".to_string());
            };

            app_task_pool.tick_until_empty();
        }

        Ok(())
    }
}

impl BevyInstance {
    pub(crate) fn execute<T>(
        &self,
        command: impl FnOnce(&mut World) -> T,
    ) -> prelude::Result<T, String> {
        self.inner.execute(command)
    }

    pub(crate) fn weak_inner(&self) -> Weak<BevyInstanceInner> {
        Rc::downgrade(&self.inner)
    }
}

impl BevyInstanceInner {
    pub(crate) fn execute<T>(
        &self,
        command: impl FnOnce(&mut World) -> T,
    ) -> prelude::Result<T, String> {
        let mut managed_app_ref = self.managed_app.borrow_mut();

        let Some(managed_app) = managed_app_ref.as_mut() else {
            return Err("Bevy instance is not mounted".to_string());
        };

        Ok(command(managed_app.app.world_mut()))
    }
}

impl Drop for BevyInstance {
    fn drop(&mut self) {
        self.inner.managed_app.borrow_mut().take();
        info!("Dropped Bevy instance");
    }
}
