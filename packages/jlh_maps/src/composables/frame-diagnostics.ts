import { computed, type ComputedRef, shallowRef, type ShallowRef } from 'vue'

const DEFAULT_FPS_WINDOW_MS = 5_000
const DEFAULT_HISTORY_MS = 5 * 60_000
const DEFAULT_TIMELINE_MS = 10_000
const DEFAULT_WINDOWS = [
  { label: '10s', durationMs: DEFAULT_TIMELINE_MS },
  { label: '1m', durationMs: 60_000 },
  { label: '5m', durationMs: DEFAULT_HISTORY_MS },
] as const

export type FrameDiagnosticsFrame = {
  index: number
  durationMs: number
}

export type FrameDiagnosticsStats = {
  label: string
  sampleCount: number
  minMs: number | null
  avgMs: number | null
  p90Ms: number | null
  maxMs: number | null
}

export type FrameDiagnosticsTimelineFrame = {
  index: number
  startedAtMs: number
  endedAtMs: number
  durationMs: number
}

export type FrameDiagnosticsSnapshot = {
  sampledAtMs: number
  currentFrame: FrameDiagnosticsFrame | null
  fps: number | null
  stats: FrameDiagnosticsStats[]
  timelineFrames: FrameDiagnosticsTimelineFrame[]
}

export type FrameDiagnosticsSample = {
  index: number
  startedAtMs: number
  endedAtMs: number
  durationMs: number
}

export type PushFrameInput = {
  startedAtMs: number
  endedAtMs: number
}

export type UseFrameDiagnosticsReturn = {
  snapshot: ComputedRef<FrameDiagnosticsSnapshot>
  currentFrame: ShallowRef<FrameDiagnosticsFrame | null>
  pushFrame: (frame: PushFrameInput) => void
  reset: () => void
}

type FrameDiagnosticsWindow = {
  label: string
  durationMs: number
}

type UseFrameDiagnosticsOptions = {
  fpsWindowMs?: number
  historyMs?: number
  timelineMs?: number
  windows?: readonly FrameDiagnosticsWindow[]
}

export function useFrameDiagnostics(
  options: UseFrameDiagnosticsOptions = {},
): UseFrameDiagnosticsReturn {
  const fpsWindowMs = options.fpsWindowMs ?? DEFAULT_FPS_WINDOW_MS
  const historyMs = options.historyMs ?? DEFAULT_HISTORY_MS
  const timelineMs = options.timelineMs ?? DEFAULT_TIMELINE_MS
  const windows = options.windows ?? DEFAULT_WINDOWS

  const currentFrame = shallowRef<FrameDiagnosticsFrame | null>(null)
  const samples: FrameDiagnosticsSample[] = []
  const samplesVersion = shallowRef(0)

  let frameIndex = 0

  const snapshot = computed<FrameDiagnosticsSnapshot>(() => {
    const version = samplesVersion.value
    void version

    const now = performance.now()
    pruneSamples(samples, now, historyMs)

    return {
      sampledAtMs: now,
      currentFrame: currentFrame.value,
      fps: rollingFps(samples, now, fpsWindowMs),
      stats: windows.map(({ label, durationMs }) => frameStats(label, samples, now, durationMs)),
      timelineFrames: frameTimelineFrames(samples, now, timelineMs),
    }
  })

  const pushFrame = ({ startedAtMs, endedAtMs }: PushFrameInput) => {
    const durationMs = Math.max(0, endedAtMs - startedAtMs)
    const index = ++frameIndex

    currentFrame.value = {
      index,
      durationMs,
    }
    samples.push({
      index,
      startedAtMs,
      endedAtMs,
      durationMs,
    })
    pruneSamples(samples, endedAtMs, historyMs)
    samplesVersion.value += 1
  }

  const reset = () => {
    currentFrame.value = null
    samples.length = 0
    frameIndex = 0
    samplesVersion.value += 1
  }

  return {
    snapshot,
    currentFrame,
    pushFrame,
    reset,
  }
}

function pruneSamples(samples: FrameDiagnosticsSample[], now: number, historyMs: number) {
  const cutoff = now - historyMs
  let firstRetainedIndex = 0

  while (firstRetainedIndex < samples.length) {
    const sample = samples[firstRetainedIndex]
    if (!sample || sample.endedAtMs >= cutoff) break

    ++firstRetainedIndex
  }

  if (firstRetainedIndex > 0) {
    samples.splice(0, firstRetainedIndex)
  }
}

function rollingFps(samples: FrameDiagnosticsSample[], now: number, windowMs: number) {
  const cutoff = now - windowMs
  let count = 0
  let oldestTimestamp = now

  for (let index = samples.length - 1; index >= 0; --index) {
    const sample = samples[index]
    if (!sample || sample.endedAtMs < cutoff) break

    oldestTimestamp = sample.endedAtMs
    ++count
  }

  if (count === 0) return null

  const elapsedMs = Math.min(windowMs, Math.max(now - oldestTimestamp, 1_000))
  return count / (elapsedMs / 1_000)
}

function frameStats(
  label: string,
  samples: FrameDiagnosticsSample[],
  now: number,
  windowMs: number,
): FrameDiagnosticsStats {
  const cutoff = now - windowMs
  const values: number[] = []

  for (let index = samples.length - 1; index >= 0; --index) {
    const sample = samples[index]
    if (!sample || sample.endedAtMs < cutoff) break

    values.push(sample.durationMs)
  }

  if (values.length === 0) {
    return {
      label,
      sampleCount: 0,
      minMs: null,
      avgMs: null,
      p90Ms: null,
      maxMs: null,
    }
  }

  let minMs = Number.POSITIVE_INFINITY
  let maxMs = 0
  let totalMs = 0

  for (const value of values) {
    minMs = Math.min(minMs, value)
    maxMs = Math.max(maxMs, value)
    totalMs += value
  }

  values.sort((a, b) => a - b)
  const p90Index = Math.max(0, Math.ceil(values.length * 0.9) - 1)

  return {
    label,
    sampleCount: values.length,
    minMs,
    avgMs: totalMs / values.length,
    p90Ms: values[p90Index] ?? maxMs,
    maxMs,
  }
}

function frameTimelineFrames(
  samples: FrameDiagnosticsSample[],
  now: number,
  windowMs: number,
): FrameDiagnosticsTimelineFrame[] {
  const cutoff = now - windowMs
  const frames: FrameDiagnosticsTimelineFrame[] = []

  for (let index = samples.length - 1; index >= 0; --index) {
    const sample = samples[index]
    if (!sample || sample.endedAtMs < cutoff) break

    frames.push({
      index: sample.index,
      startedAtMs: sample.startedAtMs,
      endedAtMs: sample.endedAtMs,
      durationMs: sample.durationMs,
    })
  }

  return frames.reverse()
}
