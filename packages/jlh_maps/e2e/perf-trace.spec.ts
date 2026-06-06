import { gzipSync } from 'node:zlib'
import { mkdirSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { test, expect, type Page } from '@playwright/test'
import type { CDPSession } from '@playwright/test'
import {
  MapViewScenarioName,
  MapViewScenarioRuntimeStatus,
} from '../src/views/map-view/map-view-scenario/scenarios'

const DEFAULT_SCENARIOS = [
  MapViewScenarioName.Static,
  MapViewScenarioName.Movement,
] as const

const TRACE_CATEGORIES = [
  'toplevel',
  'devtools.timeline',
  'disabled-by-default-devtools.timeline',
  'disabled-by-default-devtools.timeline.frame',
  'blink',
  'blink.user_timing',
  'cc',
  'gpu',
  'loading',
  'v8',
  'disabled-by-default-v8.cpu_profiler',
  'disabled-by-default-v8.cpu_profiler.hires',
]

const selectedScenarios =
  process.env.JLH_MAPS_TRACE_SCENARIOS?.split(',')
    .map((scenario) => scenario.trim())
    .filter(Boolean) ?? [...DEFAULT_SCENARIOS]

test.setTimeout(90_000)

test.describe('map performance traces', () => {
  for (const scenarioName of selectedScenarios) {
    test(`captures ${scenarioName}`, async ({ page, browserName }, testInfo) => {
      // eslint-disable-next-line playwright/no-skipped-test
      test.skip(browserName !== 'chromium', 'Chrome tracing is collected through the Chromium CDP')

      await page.goto(`/scenario/${scenarioName}`)
      await waitForScenarioReady(page, scenarioName)

      const cdp = await page.context().newCDPSession(page)
      const stopTracing = await startChromeTracing(cdp)

      let runError: unknown = null
      try {
        await mark(page, `jlh:trace:${scenarioName}:start`)
        await page.evaluate(async () => {
          await window.__jlhMapScenario?.start()
        })
        await mark(page, `jlh:trace:${scenarioName}:finish`)
      } catch (error) {
        runError = error
      }

      const traceEvents = await stopTracing()
      const tracePath = writeTraceFile(testInfo.config.rootDir, scenarioName, traceEvents)
      await testInfo.attach('chrome-trace', {
        path: tracePath,
        contentType: 'application/gzip',
      })

      // eslint-disable-next-line playwright/no-conditional-in-test
      if (runError) throw runError

      const runtimeStatus = await page.evaluate(() => ({
        status: window.__jlhMapScenario?.status,
        error: window.__jlhMapScenario?.error,
      }))

      expect(runtimeStatus.error).toBeNull()
      expect([
        MapViewScenarioRuntimeStatus.Ready,
        MapViewScenarioRuntimeStatus.Finished,
      ]).toContain(runtimeStatus.status)
    })
  }
})

async function waitForScenarioReady(page: Page, scenarioName: string) {
  await page.waitForFunction(
    ({ name, readyStatus }) => {
      const scenario = window.__jlhMapScenario
      return scenario?.name === name && scenario.status === readyStatus
    },
    {
      name: scenarioName,
      readyStatus: MapViewScenarioRuntimeStatus.Ready,
    },
    { timeout: 45_000 },
  )
}

async function mark(page: Page, name: string) {
  await page.evaluate((markName) => {
    performance.mark(markName)
  }, name)
}

async function startChromeTracing(cdp: CDPSession) {
  const traceEvents: unknown[] = []
  const tracingComplete = new Promise<void>((resolveTracingComplete) => {
    cdp.once('Tracing.tracingComplete', () => resolveTracingComplete())
  })

  cdp.on('Tracing.dataCollected', (event: { value?: unknown[] }) => {
    if (event.value) traceEvents.push(...event.value)
  })

  await cdp.send('Tracing.start', {
    transferMode: 'ReportEvents',
    traceConfig: {
      includedCategories: TRACE_CATEGORIES,
      recordMode: 'recordContinuously',
    },
  })

  return async () => {
    await cdp.send('Tracing.end')
    await tracingComplete
    await cdp.detach()
    return traceEvents
  }
}

function writeTraceFile(rootDir: string, scenarioName: string, traceEvents: unknown[]) {
  const outputDir = resolve(rootDir, '..', 'test-results', 'perf-traces')
  const timestamp = new Date().toISOString().replaceAll(':', '').replaceAll('-', '')
  const tracePath = resolve(outputDir, `${timestamp}-${scenarioName}.json.gz`)

  mkdirSync(outputDir, { recursive: true })
  writeFileSync(
    tracePath,
    gzipSync(
      JSON.stringify({
        traceEvents,
        metadata: {
          scenarioName,
          capturedAt: timestamp,
          categories: TRACE_CATEGORIES,
        },
      }),
    ),
  )

  return tracePath
}
