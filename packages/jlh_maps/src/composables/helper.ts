import {
  effectScope,
  type EffectScope,
  getCurrentScope,
  getCurrentWatcher,
  onScopeDispose,
  onWatcherCleanup,
  type ReactiveEffect,
  watch,
  type WatchSource,
} from 'vue'
import { isClient, tryOnScopeDispose } from '@vueuse/core'

export function watchDefinedOnce<T>(
  value: WatchSource<T | undefined>,
  callback: (value: T) => void,
) {
  const handle = watch(
    value,
    (val, prev) => {
      if (val !== undefined && prev === undefined) {
        callback(val)
        handle.pause()
      }
    },
    { immediate: true },
  )

  return { stop: handle.stop }
}

export function createInjectOrThrow<T>(useInjected: () => T, errorMessage: string) {
  return () => {
    const value = useInjected()

    if (value == null) {
      throw new Error(errorMessage)
    }

    return value as NonNullable<T>
  }
}

export function createKeyedSharedComposable<Params, Ret>(
  getKey: (params: Params) => string,
  composable: (params: Params) => Ret,
) {
  if (!isClient) return (params: Params) => composable(params)

  type SharedState = {
    subscribers: number
    state: Ret | undefined
    scope: EffectScope | undefined
  }

  const sharedStates = new Map<string, SharedState>()

  return (params: Params) => {
    const key = getKey(params)

    let sharedState = sharedStates.get(key)

    if (!sharedState) {
      sharedState = {
        subscribers: 0,
        state: undefined,
        scope: undefined,
      }
      sharedStates.set(key, sharedState)
    }

    sharedState.subscribers += 1

    const dispose = () => {
      if (!sharedState) return

      sharedState.subscribers -= 1

      if (sharedState.scope && sharedState.subscribers <= 0) {
        sharedState.scope.stop()
        sharedState.state = undefined
        sharedState.scope = undefined
        sharedStates.delete(key)
      }
    }

    if (!sharedState.scope) {
      sharedState.scope = effectScope(true)
      sharedState.state = sharedState.scope.run(() => composable(params))
    }

    tryOnScopeDispose(dispose)

    return sharedState.state!
  }
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
