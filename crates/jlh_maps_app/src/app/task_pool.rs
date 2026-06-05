use crate::wasm_task_pool::{TaskPool, TaskPoolBackendKind, backend_available};
use bevy::app::App;
use bevy::prelude::{Plugin, Resource};
use std::ops::Deref;
use tracing::info;

pub struct AppTaskPoolPlugin;

impl Plugin for AppTaskPoolPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(AppTaskPool::new());
    }
}

#[derive(Resource, Clone)]
pub struct AppTaskPool {
    pool: TaskPool,
}

impl Default for AppTaskPool {
    fn default() -> Self {
        Self::new()
    }
}

impl AppTaskPool {
    pub fn new() -> Self {
        let backend = if backend_available(TaskPoolBackendKind::Rayon) {
            TaskPoolBackendKind::Rayon
        } else {
            TaskPoolBackendKind::Manual
        };

        let pool = TaskPool::builder().backend(backend).build();

        info!(
            "Using {backend:?} app task pool backend with max_concurrent_tasks={}",
            pool.max_concurrent_tasks()
        );

        Self { pool }
    }
}

impl Deref for AppTaskPool {
    type Target = TaskPool;

    fn deref(&self) -> &Self::Target {
        &self.pool
    }
}
