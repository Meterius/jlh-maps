use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

const DEFAULT_RAYON_RESERVED_THREADS: usize = 2;

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
    max_concurrent_tasks: Option<usize>,
}

impl Default for TaskPoolBuilder {
    fn default() -> Self {
        Self {
            backend: TaskPoolBackendKind::Manual,
            num_threads: None,
            thread_name: None,
            max_concurrent_tasks: None,
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

    pub fn max_concurrent_tasks(mut self, max_concurrent_tasks: usize) -> Self {
        self.max_concurrent_tasks = Some(max_concurrent_tasks.max(1));
        self
    }

    pub fn build(self) -> TaskPool {
        match self.backend {
            TaskPoolBackendKind::Manual => TaskPool {
                backend: TaskPoolBackend::Manual(ManualTaskQueue::default()),
            },
            TaskPoolBackendKind::Rayon => TaskPool {
                backend: TaskPoolBackend::rayon(self.max_concurrent_tasks.unwrap_or_else(|| {
                    let worker_count = self.num_threads.unwrap_or_else(rayon_worker_count);
                    background_task_limit(worker_count, DEFAULT_RAYON_RESERVED_THREADS)
                })),
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
            TaskPoolBackend::Rayon(_) => false,
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
            TaskPoolBackend::Rayon(queue) => queue.queued_len(),
        }
    }

    pub fn backend_kind(&self) -> TaskPoolBackendKind {
        match &self.backend {
            TaskPoolBackend::Manual(_) => TaskPoolBackendKind::Manual,
            TaskPoolBackend::Rayon(_) => TaskPoolBackendKind::Rayon,
        }
    }

    pub fn max_concurrent_tasks(&self) -> usize {
        match &self.backend {
            TaskPoolBackend::Manual(_) => 1,
            TaskPoolBackend::Rayon(queue) => queue.max_concurrent_tasks(),
        }
    }
}

#[derive(Clone)]
enum TaskPoolBackend {
    Manual(ManualTaskQueue),
    Rayon(RayonTaskQueue),
}

impl TaskPoolBackend {
    fn spawn(&self, task: Box<dyn RunnableTask>) {
        match self {
            Self::Manual(queue) => queue.push(task),
            Self::Rayon(queue) => queue.push(task),
        }
    }

    fn rayon(max_concurrent_tasks: usize) -> Self {
        assert_rayon_available();
        Self::Rayon(RayonTaskQueue::new(max_concurrent_tasks))
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

#[derive(Clone)]
struct RayonTaskQueue {
    inner: Arc<Mutex<RayonTaskQueueInner>>,
}

struct RayonTaskQueueInner {
    pending: VecDeque<Box<dyn RunnableTask>>,
    running: usize,
    max_running: usize,
}

impl RayonTaskQueue {
    fn new(max_running: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(RayonTaskQueueInner {
                pending: VecDeque::new(),
                running: 0,
                max_running: max_running.max(1),
            })),
        }
    }

    fn push(&self, task: Box<dyn RunnableTask>) {
        lock(&self.inner).pending.push_back(task);
        self.dispatch_pending();
    }

    fn queued_len(&self) -> usize {
        lock(&self.inner).pending.len()
    }

    fn max_concurrent_tasks(&self) -> usize {
        lock(&self.inner).max_running
    }

    fn dispatch_pending(&self) {
        loop {
            let task = {
                let mut inner = lock(&self.inner);
                if inner.running >= inner.max_running {
                    return;
                }

                let Some(task) = inner.pending.pop_front() else {
                    return;
                };

                inner.running += 1;
                task
            };

            let guard = RayonTaskGuard {
                queue: self.clone(),
            };
            spawn_rayon(Box::new(move || {
                let _guard = guard;
                task.run();
            }));
        }
    }

    fn task_finished(&self) {
        {
            let mut inner = lock(&self.inner);
            inner.running = inner.running.saturating_sub(1);
        }

        self.dispatch_pending();
    }
}

struct RayonTaskGuard {
    queue: RayonTaskQueue,
}

impl Drop for RayonTaskGuard {
    fn drop(&mut self) {
        self.queue.task_finished();
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
    pub fn take_result(&mut self) -> Option<T> {
        lock(&self.state.inner).result.take()
    }

    pub fn has_result(&self) -> bool {
        lock(&self.state.inner).result.is_some()
    }

    pub fn is_finished(&self) -> bool {
        lock(&self.state.inner).finished
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
                finished: false,
                waker: None,
            }),
        }
    }
}

impl<T> TaskState<T> {
    fn complete(&self, result: T) {
        let mut inner = lock(&self.inner);
        inner.result = Some(result);
        inner.finished = true;

        if let Some(waker) = inner.waker.take() {
            waker.wake();
        }
    }
}

struct TaskStateInner<T> {
    result: Option<T>,
    finished: bool,
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

#[cfg(feature = "wasm-threads")]
fn rayon_worker_count() -> usize {
    rayon::current_num_threads().max(1)
}

#[cfg(not(feature = "wasm-threads"))]
fn rayon_worker_count() -> usize {
    1
}

fn background_task_limit(worker_count: usize, reserved_threads: usize) -> usize {
    worker_count.saturating_sub(reserved_threads).max(1)
}

pub fn backend_available(backend: TaskPoolBackendKind) -> bool {
    match backend {
        TaskPoolBackendKind::Manual => true,
        TaskPoolBackendKind::Rayon => cfg!(feature = "wasm-threads"),
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
        assert!(task.has_result());
        assert_eq!(task.take_result(), Some(42));
        assert!(task.is_finished());
        assert!(!task.has_result());
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
        assert_eq!(a.take_result(), Some(1));
        assert_eq!(b.take_result(), None);

        assert_eq!(pool.tick_until_empty(), 1);
        assert_eq!(b.take_result(), Some(2));
    }
}
