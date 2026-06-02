use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TaskPoolBackendKind {
    Manual,
    Rayon,
}

#[derive(Debug, Clone)]
pub struct TaskPoolBuilder {
    backend: TaskPoolBackendKind,
    num_threads: Option<usize>,
    thread_name: Option<String>,
}

impl Default for TaskPoolBuilder {
    fn default() -> Self {
        Self {
            backend: TaskPoolBackendKind::Manual,
            num_threads: None,
            thread_name: None,
        }
    }
}

impl TaskPoolBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn backend(mut self, backend: TaskPoolBackendKind) -> Self {
        self.backend = backend;
        self
    }

    // Accepted for API symmetry with Bevy's TaskPoolBuilder. wasm-bindgen-rayon
    // uses a global Rayon pool initialized by JS, so this does not create threads.
    pub fn num_threads(mut self, num_threads: usize) -> Self {
        self.num_threads = Some(num_threads);
        self
    }

    // Accepted for API symmetry. Worker naming is owned by the JS/Rayon setup.
    pub fn thread_name(mut self, thread_name: impl Into<String>) -> Self {
        self.thread_name = Some(thread_name.into());
        self
    }

    pub fn build(self) -> TaskPool {
        match self.backend {
            TaskPoolBackendKind::Manual => TaskPool {
                backend: TaskPoolBackend::Manual(ManualTaskQueue::default()),
            },
            TaskPoolBackendKind::Rayon => TaskPool {
                backend: TaskPoolBackend::rayon(),
            },
        }
    }
}

#[derive(Clone)]
pub struct TaskPool {
    backend: TaskPoolBackend,
}

impl TaskPool {
    pub fn builder() -> TaskPoolBuilder {
        TaskPoolBuilder::new()
    }

    pub fn spawn<T>(&self, task: impl FnOnce() -> T + Send + 'static) -> Task<T>
    where
        T: Send + 'static,
    {
        let state = Arc::new(TaskState::default());
        let handle = Task {
            state: state.clone(),
        };

        self.backend.spawn(Box::new(move || {
            state.complete(task());
        }));

        handle
    }

    pub fn spawn_detached(&self, task: impl FnOnce() + Send + 'static) {
        self.backend.spawn(Box::new(task));
    }

    pub fn try_tick(&self) -> bool {
        match &self.backend {
            TaskPoolBackend::Manual(queue) => queue.try_tick(),
            TaskPoolBackend::Rayon => false,
        }
    }

    pub fn tick_n(&self, max_tasks: usize) -> usize {
        let mut completed = 0;
        for _ in 0..max_tasks {
            if !self.try_tick() {
                break;
            }
            completed += 1;
        }
        completed
    }

    pub fn tick_until_empty(&self) -> usize {
        let mut completed = 0;
        while self.try_tick() {
            completed += 1;
        }
        completed
    }

    pub fn queued_len(&self) -> usize {
        match &self.backend {
            TaskPoolBackend::Manual(queue) => queue.queued_len(),
            TaskPoolBackend::Rayon => 0,
        }
    }

    pub fn backend_kind(&self) -> TaskPoolBackendKind {
        match self.backend {
            TaskPoolBackend::Manual(_) => TaskPoolBackendKind::Manual,
            TaskPoolBackend::Rayon => TaskPoolBackendKind::Rayon,
        }
    }
}

#[derive(Clone)]
enum TaskPoolBackend {
    Manual(ManualTaskQueue),
    Rayon,
}

impl TaskPoolBackend {
    fn spawn(&self, task: Box<dyn RunnableTask>) {
        match self {
            Self::Manual(queue) => queue.push(task),
            Self::Rayon => spawn_rayon(task),
        }
    }

    fn rayon() -> Self {
        assert_rayon_available();
        Self::Rayon
    }
}

#[derive(Clone, Default)]
pub struct ManualTaskQueue {
    pending: Arc<Mutex<VecDeque<Box<dyn RunnableTask>>>>,
}

impl ManualTaskQueue {
    fn push(&self, task: Box<dyn RunnableTask>) {
        lock(&self.pending).push_back(task);
    }

    pub fn try_tick(&self) -> bool {
        let task = lock(&self.pending).pop_front();
        let Some(task) = task else {
            return false;
        };

        task.run();
        true
    }

    pub fn tick_n(&self, max_tasks: usize) -> usize {
        let mut completed = 0;
        for _ in 0..max_tasks {
            if !self.try_tick() {
                break;
            }
            completed += 1;
        }
        completed
    }

    pub fn tick_until_empty(&self) -> usize {
        let mut completed = 0;
        while self.try_tick() {
            completed += 1;
        }
        completed
    }

    pub fn queued_len(&self) -> usize {
        lock(&self.pending).len()
    }
}

trait RunnableTask: Send + 'static {
    fn run(self: Box<Self>);
}

impl<F> RunnableTask for F
where
    F: FnOnce() + Send + 'static,
{
    fn run(self: Box<Self>) {
        self();
    }
}

pub struct Task<T> {
    state: Arc<TaskState<T>>,
}

impl<T> Task<T> {
    pub fn poll_once(&mut self) -> Option<T> {
        lock(&self.state.inner).result.take()
    }

    pub fn is_finished(&self) -> bool {
        lock(&self.state.inner).result.is_some()
    }

    pub fn detach(self) {}
}

impl<T> Future for Task<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut inner = lock(&self.state.inner);
        if let Some(result) = inner.result.take() {
            return Poll::Ready(result);
        }

        inner.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

struct TaskState<T> {
    inner: Mutex<TaskStateInner<T>>,
}

impl<T> Default for TaskState<T> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(TaskStateInner {
                result: None,
                waker: None,
            }),
        }
    }
}

impl<T> TaskState<T> {
    fn complete(&self, result: T) {
        let mut inner = lock(&self.inner);
        inner.result = Some(result);

        if let Some(waker) = inner.waker.take() {
            waker.wake();
        }
    }
}

struct TaskStateInner<T> {
    result: Option<T>,
    waker: Option<Waker>,
}

#[cfg(feature = "wasm-threads")]
fn assert_rayon_available() {}

#[cfg(not(feature = "wasm-threads"))]
fn assert_rayon_available() {
    panic!(
        "TaskPoolBackendKind::Rayon requires the jlh_maps_app `wasm-threads` feature to be enabled"
    );
}

#[cfg(feature = "wasm-threads")]
fn spawn_rayon(task: Box<dyn RunnableTask>) {
    rayon::spawn(move || task.run());
}

#[cfg(not(feature = "wasm-threads"))]
fn spawn_rayon(_task: Box<dyn RunnableTask>) {
    unreachable!("Rayon backend cannot be constructed without the `wasm-threads` feature");
}

pub fn backend_available(backend: TaskPoolBackendKind) -> bool {
    match backend {
        TaskPoolBackendKind::Manual => true,
        TaskPoolBackendKind::Rayon => {
            !cfg!(target_arch = "wasm32") || cfg!(feature = "wasm-threads")
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_backend_defers_work_until_ticked() {
        let pool = TaskPool::builder()
            .backend(TaskPoolBackendKind::Manual)
            .build();
        let mut task = pool.spawn(|| 42);

        assert_eq!(pool.queued_len(), 1);
        assert!(!task.is_finished());
        assert_eq!(pool.tick_n(1), 1);
        assert!(task.is_finished());
        assert_eq!(task.poll_once(), Some(42));
    }

    #[test]
    fn manual_backend_ticks_bounded_work() {
        let pool = TaskPool::builder()
            .backend(TaskPoolBackendKind::Manual)
            .build();
        let mut a = pool.spawn(|| 1);
        let mut b = pool.spawn(|| 2);

        assert_eq!(pool.tick_n(1), 1);
        assert_eq!(pool.queued_len(), 1);
        assert_eq!(a.poll_once(), Some(1));
        assert_eq!(b.poll_once(), None);

        assert_eq!(pool.tick_until_empty(), 1);
        assert_eq!(b.poll_once(), Some(2));
    }
}
