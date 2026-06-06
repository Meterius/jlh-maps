import type { MapLibreMap } from 'maplibre-gl'
import { onScopeDispose } from 'vue'
import {
  MAP_VIEW_SCENARIOS,
  MapViewScenarioName,
  MapViewScenarioRuntimeStatus,
  getMapViewScenario,
  isMapViewScenarioName,
  type MapViewScenario,
  type MapViewScenarioStoreContext,
} from './scenarios.ts'

export {
  MAP_VIEW_SCENARIOS,
  MapViewScenarioName,
  MapViewScenarioRuntimeStatus,
  getMapViewScenario,
  isMapViewScenarioName,
  type MapViewScenario,
  type MapViewScenarioHookContext,
  type MapViewScenarioStoreContext,
} from './scenarios.ts'

export type MapViewScenarioRuntime = {
  name: MapViewScenarioName
  status: MapViewScenarioRuntimeStatus
  readyAtMs: number | null
  startedAtMs: number | null
  finishedAtMs: number | null
  error: string | null
  start: () => Promise<void>
}

declare global {
  interface Window {
    __jlhMapScenario?: MapViewScenarioRuntime
  }
}

export function useMapViewScenarioRuntime(
  scenarioName: MapViewScenarioName,
  map: MapLibreMap,
  context: MapViewScenarioStoreContext,
) {
  const scenario = MAP_VIEW_SCENARIOS[scenarioName]
  let disposed = false
  let readyResolved = false
  let runPromise: Promise<void> | null = null
  let resolveReady: () => void
  const readyPromise = new Promise<void>((resolve) => {
    resolveReady = resolve
  })

  const runtime: MapViewScenarioRuntime = {
    name: scenarioName,
    status: MapViewScenarioRuntimeStatus.Initializing,
    readyAtMs: null,
    startedAtMs: null,
    finishedAtMs: null,
    error: null,
    start: () => {
      if (runPromise) return runPromise

      runPromise = runScenario(scenario, map, readyPromise, runtime, context)
      return runPromise
    },
  }

  window.__jlhMapScenario = runtime

  Promise.resolve(scenario.setup?.(context)).then(
    () => {
      if (disposed) return

      readyResolved = true
      runtime.readyAtMs = performance.now()
      runtime.status = MapViewScenarioRuntimeStatus.Ready
      performance.mark(`jlh:scenario:${scenarioName}:ready`)
      resolveReady()
    },
    (error: unknown) => {
      if (disposed) return

      runtime.status = MapViewScenarioRuntimeStatus.Error
      runtime.error = formatError(error)
      resolveReady()
    },
  )

  onScopeDispose(() => {
    disposed = true
    if (!readyResolved) resolveReady()

    runtime.status = MapViewScenarioRuntimeStatus.Disposed
    if (window.__jlhMapScenario === runtime) {
      delete window.__jlhMapScenario
    }
  })

  return runtime
}

async function runScenario(
  scenario: MapViewScenario,
  map: MapLibreMap,
  readyPromise: Promise<void>,
  runtime: MapViewScenarioRuntime,
  context: MapViewScenarioStoreContext,
) {
  await readyPromise

  if (
    runtime.status === MapViewScenarioRuntimeStatus.Error ||
    runtime.status === MapViewScenarioRuntimeStatus.Disposed
  ) {
    return
  }

  runtime.status = MapViewScenarioRuntimeStatus.Running
  runtime.startedAtMs = performance.now()
  performance.mark(`jlh:scenario:${scenario.name}:start`)

  try {
    await (scenario.run?.({ ...context, map, scenario }) ??
      new Promise<void>((resolve) => requestAnimationFrame(() => resolve())))

    runtime.status = MapViewScenarioRuntimeStatus.Finished
    runtime.finishedAtMs = performance.now()
    performance.mark(`jlh:scenario:${scenario.name}:finish`)
  } catch (error) {
    runtime.status = MapViewScenarioRuntimeStatus.Error
    runtime.error = formatError(error)
    throw error
  }
}

function formatError(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}
