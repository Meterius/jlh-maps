use alloc::{boxed::Box, string::String, vec::Vec};
use bevy_platform::sync::Arc;
use concurrent_queue::ConcurrentQueue;
use core::{future::Future, marker::PhantomData, mem, panic::AssertUnwindSafe};
use futures_lite::FutureExt;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    thread_local,
};

use crate::{Task, block_on, executor::LocalExecutor as Executor};

thread_local! {
    static LOCAL_EXECUTOR: Executor<'static> = const { Executor::new() };
}

type Panic = Box<dyn core::any::Any + Send + 'static>;
type TaskResult<T> = Result<T, Panic>;
type LocalScopeJob<'scope> = Box<dyn FnOnce() + Send + 'scope>;

/// Used to create a [`TaskPool`].
#[derive(Debug, Default, Clone)]
pub struct TaskPoolBuilder {
    num_threads: Option<usize>,
}

/// A wasm main-thread executor marker.
///
/// The native task pool owns a real thread-local executor here. For wasm threads,
/// scope-local jobs are explicitly queued and drained by [`TaskPool::scope_with_executor`]
/// on the Bevy worker thread.
#[derive(Default)]
pub struct ThreadExecutor<'a>(PhantomData<&'a ()>);

impl<'a> ThreadExecutor<'a> {
    /// Creates a new [`ThreadExecutor`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if `self` and `other` are the same executor marker.
    pub fn is_same(&self, other: &Self) -> bool {
        core::ptr::eq(self, other)
    }
}

impl TaskPoolBuilder {
    /// Creates a new [`TaskPoolBuilder`] instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the desired Rayon pool size. The actual worker count is owned by
    /// `wasm-bindgen-rayon` and initialized by the app before Bevy starts.
    pub fn num_threads(mut self, num_threads: usize) -> Self {
        self.num_threads = Some(num_threads.max(1));
        self
    }

    /// No op for the wasm Rayon-backed task pool.
    pub fn stack_size(self, _stack_size: usize) -> Self {
        self
    }

    /// No op for the wasm Rayon-backed task pool.
    pub fn thread_name(self, _thread_name: String) -> Self {
        self
    }

    /// No op for the wasm Rayon-backed task pool.
    pub fn on_thread_spawn(self, _f: impl Fn() + Send + Sync + 'static) -> Self {
        self
    }

    /// No op for the wasm Rayon-backed task pool.
    pub fn on_thread_destroy(self, _f: impl Fn() + Send + Sync + 'static) -> Self {
        self
    }

    /// Creates a new [`TaskPool`].
    pub fn build(self) -> TaskPool {
        TaskPool::new_internal(self)
    }
}

/// A wasm task pool that delegates `Send` work to Rayon workers.
#[derive(Debug, Clone)]
pub struct TaskPool {
    configured_threads: usize,
}

impl TaskPool {
    /// Creates a new main-thread executor marker.
    pub fn get_thread_executor() -> Arc<ThreadExecutor<'static>> {
        Arc::new(ThreadExecutor::new())
    }

    /// Create a [`TaskPool`] with the default configuration.
    pub fn new() -> Self {
        TaskPoolBuilder::new().build()
    }

    fn new_internal(builder: TaskPoolBuilder) -> Self {
        Self {
            configured_threads: builder.num_threads.unwrap_or(1),
        }
    }

    /// Return the current Rayon worker count.
    pub fn thread_num(&self) -> usize {
        rayon::current_num_threads()
            .max(self.configured_threads)
            .max(1)
    }

    /// Runs a scoped set of futures on the calling Bevy worker thread.
    ///
    /// Generic Bevy task-pool scopes are used by render internals that may
    /// capture wasm thread-affine values such as `RenderDevice`.
    pub fn scope<'env, F, T>(&self, f: F) -> Vec<T>
    where
        F: for<'scope> FnOnce(&'scope Scope<'scope, 'env, T>),
        T: Send + 'static,
    {
        self.scope_inner(false, f)
    }

    /// Allows the ECS scheduler to spawn `Send` system futures on Rayon. Scope
    /// and external jobs run on the calling Bevy worker thread before returning.
    pub fn scope_with_executor<'env, F, T>(
        &self,
        _tick_task_pool_executor: bool,
        _thread_executor: Option<&ThreadExecutor>,
        f: F,
    ) -> Vec<T>
    where
        F: for<'scope> FnOnce(&'scope Scope<'scope, 'env, T>),
        T: Send + 'static,
    {
        self.scope_inner(true, f)
    }

    #[expect(
        unsafe_code,
        reason = "Required to bind scoped jobs to the call lifetime."
    )]
    fn scope_inner<'env, F, T>(&self, run_spawn_jobs_on_rayon: bool, f: F) -> Vec<T>
    where
        F: for<'scope> FnOnce(&'scope Scope<'scope, 'env, T>),
        T: Send + 'static,
    {
        let results = ConcurrentQueue::<TaskResult<T>>::unbounded();
        let local_jobs = ConcurrentQueue::<LocalScopeJob<'env>>::unbounded();
        let pending_tasks = AtomicUsize::new(0);

        rayon::in_place_scope(|rayon_scope| {
            // SAFETY: all jobs spawned through this scope are driven to completion
            // before `rayon::scope` returns, so these references cannot outlive
            // the borrowed stack state.
            let results_ref: &'env ConcurrentQueue<TaskResult<T>> =
                unsafe { mem::transmute(&results) };
            let local_jobs_ref: &'env ConcurrentQueue<LocalScopeJob<'env>> =
                unsafe { mem::transmute(&local_jobs) };
            let pending_tasks_ref: &'env AtomicUsize = unsafe { mem::transmute(&pending_tasks) };
            let rayon_scope_ref: &'env rayon::Scope<'env> = unsafe { mem::transmute(rayon_scope) };

            let scope = Scope {
                rayon_scope: rayon_scope_ref,
                run_spawn_jobs_on_rayon,
                results: results_ref,
                local_jobs: local_jobs_ref,
                pending_tasks: pending_tasks_ref,
                scope: PhantomData,
                env: PhantomData,
            };

            // SAFETY: as above, all references are confined to this call and all
            // spawned work completes before the function returns.
            let scope_ref: &'env Scope<'_, 'env, T> = unsafe { mem::transmute(&scope) };

            f(scope_ref);

            Self::drain_scope_thread_jobs(local_jobs_ref, pending_tasks_ref);
        });

        let mut output = Vec::with_capacity(results.len());
        while let Ok(result) = results.pop() {
            match result {
                Ok(value) => output.push(value),
                Err(payload) => std::panic::resume_unwind(payload),
            }
        }
        output
    }

    fn drain_scope_thread_jobs<'scope>(
        local_jobs: &'scope ConcurrentQueue<LocalScopeJob<'scope>>,
        pending_tasks: &'scope AtomicUsize,
    ) {
        loop {
            let mut ran_job = false;
            while let Ok(job) = local_jobs.pop() {
                ran_job = true;
                job();
            }

            if pending_tasks.load(Ordering::Acquire) == 0 && local_jobs.is_empty() {
                break;
            }

            if !ran_job {
                rayon::yield_now();
                core::hint::spin_loop();
            }
        }
    }

    /// Spawns a static future on the JS event loop for wasm compatibility.
    ///
    /// Bevy's asset stack uses non-`Send` futures on wasm. Scoped scheduler jobs
    /// still use Rayon through [`Scope::spawn`].
    pub fn spawn<T>(&self, future: impl Future<Output = T> + 'static) -> Task<T>
    where
        T: 'static,
    {
        Task::wrap_future(future)
    }

    /// Spawns a static future on the JS event loop for the current worker.
    pub fn spawn_local<T>(&self, future: impl Future<Output = T> + 'static) -> Task<T>
    where
        T: 'static,
    {
        Task::wrap_future(future)
    }

    /// Runs a function with the thread-local executor.
    pub fn with_local_executor<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Executor) -> R,
    {
        LOCAL_EXECUTOR.with(f)
    }
}

impl Default for TaskPool {
    fn default() -> Self {
        Self::new()
    }
}

/// A [`TaskPool`] scope for running one or more non-`'static` futures.
#[derive(Debug)]
pub struct Scope<'scope, 'env: 'scope, T> {
    rayon_scope: &'scope rayon::Scope<'scope>,
    run_spawn_jobs_on_rayon: bool,
    results: &'scope ConcurrentQueue<TaskResult<T>>,
    local_jobs: &'scope ConcurrentQueue<LocalScopeJob<'scope>>,
    pending_tasks: &'scope AtomicUsize,
    scope: PhantomData<&'scope mut &'scope ()>,
    env: PhantomData<&'env mut &'env ()>,
}

impl<'scope, 'env, T: Send + 'scope> Scope<'scope, 'env, T> {
    /// Spawns a scoped future.
    ///
    /// Scheduler scopes run these jobs on Rayon. Generic task-pool scopes keep
    /// them on the Bevy worker thread to avoid moving wasm thread-affine
    /// browser/GPU wrappers to Rayon workers.
    pub fn spawn<Fut: Future<Output = T> + 'scope + Send>(&self, f: Fut) {
        if !self.run_spawn_jobs_on_rayon {
            self.spawn_local_job(f);
            return;
        }

        let pending_tasks = self.pending_tasks;
        let results = self.results;
        pending_tasks.fetch_add(1, Ordering::Release);
        self.rayon_scope.spawn(move |_| {
            let result = block_on(AssertUnwindSafe(f).catch_unwind());
            let _ = results.push(result);
            pending_tasks.fetch_sub(1, Ordering::Release);
        });
    }

    /// Queues a scoped future to run on the Bevy worker thread that owns this scope.
    pub fn spawn_on_scope<Fut: Future<Output = T> + 'scope + Send>(&self, f: Fut) {
        self.spawn_local_job(f);
    }

    /// Queues a scoped future to run on the Bevy worker thread that owns this scope.
    pub fn spawn_on_external<Fut: Future<Output = T> + 'scope + Send>(&self, f: Fut) {
        self.spawn_local_job(f);
    }

    fn spawn_local_job<Fut: Future<Output = T> + 'scope + Send>(&self, f: Fut) {
        let pending_tasks = self.pending_tasks;
        let results = self.results;
        pending_tasks.fetch_add(1, Ordering::Release);
        let job = Box::new(move || {
            let result = block_on(AssertUnwindSafe(f).catch_unwind());
            let _ = results.push(result);
            pending_tasks.fetch_sub(1, Ordering::Release);
        });
        let _ = self.local_jobs.push(job);
    }
}
