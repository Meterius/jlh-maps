use crate::wasm_task_pool::{TaskPool, TaskPoolBackendKind, rayon_backend_available};
use bevy::app::App;
use bevy::prelude::{Plugin, Resource};
use std::ops::Deref;
use tracing::info;

pub struct AppTaskPoolPlugin;

impl Plugin for AppTaskPoolPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(AppTaskPool::new_detected());
    }
}

#[derive(Resource, Clone)]
pub struct AppTaskPool {
    pool: TaskPool,
    backend: TaskPoolBackendKind,
}

impl AppTaskPool {
    pub fn new_detected() -> Self {
        Self::new(Self::detect_backend())
    }

    pub fn new(backend: TaskPoolBackendKind) -> Self {
        let pool = match backend {
            TaskPoolBackendKind::Manual => TaskPool::new_manual(),
            TaskPoolBackendKind::Rayon => TaskPool::new_rayon(),
        };

        info!("Using {backend:?} app task pool backend");

        Self { pool, backend }
    }

    pub fn detect_backend() -> TaskPoolBackendKind {
        if rayon_backend_available() {
            TaskPoolBackendKind::Rayon
        } else {
            TaskPoolBackendKind::Manual
        }
    }

    pub fn backend(&self) -> TaskPoolBackendKind {
        self.backend
    }

    pub fn is_manual(&self) -> bool {
        self.backend == TaskPoolBackendKind::Manual
    }

    pub fn is_rayon(&self) -> bool {
        self.backend == TaskPoolBackendKind::Rayon
    }
}

impl Deref for AppTaskPool {
    type Target = TaskPool;

    fn deref(&self) -> &Self::Target {
        &self.pool
    }
}
