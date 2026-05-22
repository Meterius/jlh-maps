import {
  type EffectScope,
  getCurrentScope,
  getCurrentWatcher,
  onScopeDispose,
  onWatcherCleanup,
  type ReactiveEffect,
  watch,
  type WatchSource,
} from 'vue'

export function watchDefinedOnce<T>(
  value: WatchSource<T | undefined>,
  callback: (value: T) => void,
) {
  return watch(
    value,
    (val, prev) => {
      if (val !== undefined && prev === undefined) {
        callback(val)
      }
    },
    { immediate: true },
  )
}

// LIFO Semantics For Effect Scope And Watcher Effect Cleanup

const scopeCleanupQueues = new WeakMap<EffectScope, (() => void)[]>()
const watcherCleanupQueues = new WeakMap<ReactiveEffect, (() => void)[]>()

export function onScopeDisposeLifo(callback: () => void, failSilently = false) {
  const scope = getCurrentScope()

  if (!scope) {
    onScopeDispose(callback, failSilently)
    return
  }

  let cleanupQueue = scopeCleanupQueues.get(scope)

  if (!cleanupQueue) {
    const newCleanupQueue: (() => void)[] = []

    cleanupQueue = newCleanupQueue
    scopeCleanupQueues.set(scope, newCleanupQueue)
    onScopeDispose(() => {
      for (let i = newCleanupQueue.length - 1; i >= 0; i -= 1) {
        newCleanupQueue[i]!()
      }

      newCleanupQueue.length = 0
      scopeCleanupQueues.delete(scope)
    }, failSilently)
  }

  cleanupQueue.push(callback)
}

export function onWatcherCleanupLifo(callback: () => void, failSilently = false) {
  const watcher = getCurrentWatcher()

  if (!watcher) {
    onWatcherCleanup(callback, failSilently)
    return
  }

  let cleanupQueue = watcherCleanupQueues.get(watcher)

  if (!cleanupQueue) {
    const newCleanupQueue: (() => void)[] = []

    cleanupQueue = newCleanupQueue
    watcherCleanupQueues.set(watcher, newCleanupQueue)
    onWatcherCleanup(() => {
      for (let i = newCleanupQueue.length - 1; i >= 0; i -= 1) {
        newCleanupQueue[i]!()
      }

      newCleanupQueue.length = 0
      watcherCleanupQueues.delete(watcher)
    }, failSilently)
  }

  cleanupQueue.push(callback)
}
