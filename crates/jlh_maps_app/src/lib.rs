pub mod app;
pub mod utils;
pub mod wasm_task_pool;

#[cfg(all(feature = "wasm-threads", target_arch = "wasm32"))]
pub use wasm_bindgen_rayon::init_thread_pool;
