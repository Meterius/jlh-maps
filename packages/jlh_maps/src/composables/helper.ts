import {
  computed,
  effectScope,
  type EffectScope,
  getCurrentScope,
  getCurrentWatcher,
  type MaybeRefOrGetter,
  onScopeDispose,
  onWatcherCleanup,
  type ReactiveEffect,
  shallowRef,
  toValue,
  watch,
  type WatchSource,
} from 'vue'
import { isClient, tryOnScopeDispose } from '@vueuse/core'
import { isEqual, cloneDeep } from 'lodash'

export function watchDefinedOnce<T>(
  value: WatchSource<T | undefined>,
  callback: (value: T) => void,
) {
  const initialValue = toValue(value)
  if (initialValue !== undefined) {
    callback(initialValue)
    return { stop: () => {} }
  }

  const handle = watch(value, (val, prev) => {
    if (val !== undefined && prev === undefined) {
      callback(val)
      handle.pause()
    }
  })

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

export function createToggledComposable<Ret>(
  enabled: MaybeRefOrGetter<boolean>,
  composable: () => NonNullable<Ret>,
) {
  const instance = shallowRef<{
    scope: EffectScope
    data: Ret
  } | null>(null)

  watch(
    () => toValue(enabled),
    (value) => {
      if (value && !instance.value) {
        const scope = effectScope(true)

        let data: Ret | null = null
        scope.run(() => {
          data = composable()
        })

        if (data !== null) {
          instance.value = { scope, data }
        }
      } else if (!value && instance.value) {
        instance.value.scope.stop()
        instance.value = null
      }
    },
    { immediate: true },
  )

  onScopeDispose(() => {
    instance.value?.scope.stop()
    instance.value = null
  })

  return computed(() => instance.value?.data ?? null)
}

export function createDynamicComposable<Params, Ret>(
  params: MaybeRefOrGetter<Params>,
  composable: (params: Params) => Ret,
) {
  const instance = shallowRef<{
    params: Params
    scope: EffectScope
    data: Ret
  } | null>(null)

  watch(
    () => toValue(params),
    (value) => {
      if (!instance.value || !isEqual(value, instance.value?.params)) {
        if (instance.value) {
          instance.value.scope.stop()
        }

        const updatedParams = cloneDeep(value)

        const scope = effectScope(true)

        let data: Ret | null = null
        scope.run(() => {
          data = composable(updatedParams)
        })

        instance.value = { scope, data: data!, params: updatedParams }
      }
    },
    { immediate: true },
  )

  onScopeDispose(() => {
    instance.value?.scope.stop()
    instance.value = null
  })

  return computed(() => instance.value!.data)
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
