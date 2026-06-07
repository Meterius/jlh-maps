// WARNING: AI-SLOP
import { shallowRef, type ShallowRef, triggerRef } from 'vue'

const DEFAULT_FPS_WINDOW_MS = 1_000
const DEFAULT_HISTORY_MS = 5 * 60_000
const DEFAULT_TIMELINE_MS = 10_000
const DEFAULT_MAX_TRACKED_FPS = 240
const HISTOGRAM_BUCKET_MS = 0.1
const HISTOGRAM_MAX_MS = 1_000
const HISTOGRAM_BUCKET_COUNT = Math.floor(HISTOGRAM_MAX_MS / HISTOGRAM_BUCKET_MS) + 1
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
  snapshot: ShallowRef<FrameDiagnosticsSnapshot>
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
  maxTrackedFps?: number
  windows?: readonly FrameDiagnosticsWindow[]
}

export function useFrameDiagnostics(
  options: UseFrameDiagnosticsOptions = {},
): UseFrameDiagnosticsReturn {
  const fpsWindowMs = options.fpsWindowMs ?? DEFAULT_FPS_WINDOW_MS
  const historyMs = options.historyMs ?? DEFAULT_HISTORY_MS
  const timelineMs = options.timelineMs ?? DEFAULT_TIMELINE_MS
  const maxTrackedFps = options.maxTrackedFps ?? DEFAULT_MAX_TRACKED_FPS
  const windows = options.windows ?? DEFAULT_WINDOWS

  const currentFrameValue: FrameDiagnosticsFrame = {
    index: 0,
    durationMs: 0,
  }
  const currentFrame = shallowRef<FrameDiagnosticsFrame | null>(null)
  const samples = new FrameSampleRing(frameCapacity(historyMs, maxTrackedFps))
  const fpsWindow = new RollingFpsWindow(fpsWindowMs, frameCapacity(fpsWindowMs, maxTrackedFps))
  const rollingWindows = windows.map(
    ({ label, durationMs }) =>
      new RollingStatsWindow(label, durationMs, frameCapacity(durationMs, maxTrackedFps)),
  )
  const stats = rollingWindows.map((window) => window.stats)
  const timelineFrames: FrameDiagnosticsTimelineFrame[] = []
  const snapshotValue: FrameDiagnosticsSnapshot = {
    sampledAtMs: performance.now(),
    currentFrame: null,
    fps: null,
    stats,
    timelineFrames,
  }
  const snapshotRef = shallowRef(snapshotValue)

  let frameIndex = 0
  let fps: number | null = null

  const refreshSnapshot = (now: number) => {
    samples.pruneEndedBefore(now - historyMs)
    snapshotValue.sampledAtMs = now
    snapshotValue.currentFrame = currentFrame.value
    snapshotValue.fps = fps
    fillFrameTimelineFrames(samples, now, timelineMs, timelineFrames)
    triggerRef(snapshotRef)
  }

  const pushFrame = ({ startedAtMs, endedAtMs }: PushFrameInput) => {
    const durationMs = Math.max(0, endedAtMs - startedAtMs)
    const index = ++frameIndex

    currentFrameValue.index = index
    currentFrameValue.durationMs = durationMs
    if (currentFrame.value) {
      triggerRef(currentFrame)
    } else {
      currentFrame.value = currentFrameValue
    }

    samples.pruneEndedBefore(endedAtMs - historyMs)
    samples.push(index, startedAtMs, endedAtMs, durationMs)
    fps = fpsWindow.push(endedAtMs)

    for (const window of rollingWindows) {
      window.push(index, endedAtMs, durationMs)
    }

    refreshSnapshot(endedAtMs)
  }

  const reset = () => {
    currentFrame.value = null
    samples.clear()
    fpsWindow.clear()
    rollingWindows.forEach((window) => window.clear())
    frameIndex = 0
    fps = null
    refreshSnapshot(performance.now())
  }

  return {
    snapshot: snapshotRef,
    currentFrame,
    pushFrame,
    reset,
  }
}

function frameCapacity(durationMs: number, maxTrackedFps: number) {
  return Math.max(1, Math.ceil((durationMs / 1_000) * maxTrackedFps) + 2)
}

function emptyStats(label: string): FrameDiagnosticsStats {
  return {
    label,
    sampleCount: 0,
    minMs: null,
    avgMs: null,
    p90Ms: null,
    maxMs: null,
  }
}

class FrameSampleRing {
  private head = 0
  private used = 0

  private readonly indices: Float64Array
  private readonly startedAtMs: Float64Array
  private readonly endedAtMs: Float64Array
  private readonly durationMs: Float64Array

  constructor(private readonly capacity: number) {
    this.indices = new Float64Array(capacity)
    this.startedAtMs = new Float64Array(capacity)
    this.endedAtMs = new Float64Array(capacity)
    this.durationMs = new Float64Array(capacity)
  }

  get length() {
    return this.used
  }

  clear() {
    this.head = 0
    this.used = 0
  }

  push(index: number, startedAtMs: number, endedAtMs: number, durationMs: number) {
    if (this.used === this.capacity) {
      this.dropOldest()
    }

    const physicalIndex = this.physicalIndex(this.used)
    this.indices[physicalIndex] = index
    this.startedAtMs[physicalIndex] = startedAtMs
    this.endedAtMs[physicalIndex] = endedAtMs
    this.durationMs[physicalIndex] = durationMs
    ++this.used
  }

  pruneEndedBefore(cutoffMs: number) {
    while (this.used > 0 && this.endedAtMs[this.head]! < cutoffMs) {
      this.dropOldest()
    }
  }

  writeSampleAt(offset: number, sample: FrameDiagnosticsSample) {
    const physicalIndex = this.physicalIndex(offset)
    sample.index = this.indices[physicalIndex]!
    sample.startedAtMs = this.startedAtMs[physicalIndex]!
    sample.endedAtMs = this.endedAtMs[physicalIndex]!
    sample.durationMs = this.durationMs[physicalIndex]!
  }

  endedAt(offset: number) {
    return this.endedAtMs[this.physicalIndex(offset)]!
  }

  private dropOldest() {
    this.head = (this.head + 1) % this.capacity
    --this.used
  }

  private physicalIndex(offset: number) {
    return (this.head + offset) % this.capacity
  }
}

class RollingFpsWindow {
  private head = 0
  private used = 0

  private readonly endedAtMs: Float64Array

  constructor(
    private readonly windowMs: number,
    private readonly capacity: number,
  ) {
    this.endedAtMs = new Float64Array(capacity)
  }

  clear() {
    this.head = 0
    this.used = 0
  }

  push(endedAtMs: number) {
    this.prune(endedAtMs)

    if (this.used === this.capacity) {
      this.dropOldest()
    }

    this.endedAtMs[this.physicalIndex(this.used)] = endedAtMs
    ++this.used

    return this.fps(endedAtMs)
  }

  private prune(nowMs: number) {
    const cutoffMs = nowMs - this.windowMs
    while (this.used > 0 && this.endedAtMs[this.head]! < cutoffMs) {
      this.dropOldest()
    }
  }

  private fps(nowMs: number) {
    if (this.used === 0) return null

    const oldestTimestamp = this.endedAtMs[this.head]!
    const elapsedMs = Math.min(this.windowMs, Math.max(nowMs - oldestTimestamp, 1_000))
    return this.used / (elapsedMs / 1_000)
  }

  private dropOldest() {
    this.head = (this.head + 1) % this.capacity
    --this.used
  }

  private physicalIndex(offset: number) {
    return (this.head + offset) % this.capacity
  }
}

class RollingStatsWindow {
  readonly stats: FrameDiagnosticsStats

  private head = 0
  private used = 0
  private totalMs = 0

  private readonly indices: Float64Array
  private readonly endedAtMs: Float64Array
  private readonly durationMs: Float64Array
  private readonly histogram: Uint32Array
  private readonly minDeque: MonotonicDurationDeque
  private readonly maxDeque: MonotonicDurationDeque

  constructor(
    label: string,
    private readonly windowMs: number,
    private readonly capacity: number,
  ) {
    this.stats = emptyStats(label)
    this.indices = new Float64Array(capacity)
    this.endedAtMs = new Float64Array(capacity)
    this.durationMs = new Float64Array(capacity)
    this.histogram = new Uint32Array(HISTOGRAM_BUCKET_COUNT)
    this.minDeque = new MonotonicDurationDeque(capacity, 'min')
    this.maxDeque = new MonotonicDurationDeque(capacity, 'max')
  }

  clear() {
    this.head = 0
    this.used = 0
    this.totalMs = 0
    this.histogram.fill(0)
    this.minDeque.clear()
    this.maxDeque.clear()
    this.updateStats()
  }

  push(index: number, endedAtMs: number, durationMs: number) {
    this.prune(endedAtMs)

    if (this.used === this.capacity) {
      this.dropOldest()
    }

    const physicalIndex = this.physicalIndex(this.used)
    this.indices[physicalIndex] = index
    this.endedAtMs[physicalIndex] = endedAtMs
    this.durationMs[physicalIndex] = durationMs
    ++this.used

    this.totalMs += durationMs
    const bucket = durationBucket(durationMs)
    this.histogram[bucket] = (this.histogram[bucket] ?? 0) + 1
    this.minDeque.push(index, durationMs)
    this.maxDeque.push(index, durationMs)
    this.updateStats()
  }

  private prune(nowMs: number) {
    const cutoffMs = nowMs - this.windowMs
    while (this.used > 0 && this.endedAtMs[this.head]! < cutoffMs) {
      this.dropOldest()
    }
  }

  private dropOldest() {
    const index = this.indices[this.head]!
    const durationMs = this.durationMs[this.head]!

    this.totalMs -= durationMs
    const bucket = durationBucket(durationMs)
    this.histogram[bucket] = Math.max(0, (this.histogram[bucket] ?? 0) - 1)
    this.minDeque.expireThrough(index)
    this.maxDeque.expireThrough(index)
    this.head = (this.head + 1) % this.capacity
    --this.used

    if (this.used === 0) {
      this.totalMs = 0
    }
  }

  private updateStats() {
    const stats = this.stats
    stats.sampleCount = this.used

    if (this.used === 0) {
      stats.minMs = null
      stats.avgMs = null
      stats.p90Ms = null
      stats.maxMs = null
      return
    }

    stats.minMs = this.minDeque.frontValue()
    stats.avgMs = this.totalMs / this.used
    stats.p90Ms = histogramPercentile(this.histogram, this.used, 0.9)
    stats.maxMs = this.maxDeque.frontValue()
  }

  private physicalIndex(offset: number) {
    return (this.head + offset) % this.capacity
  }
}

class MonotonicDurationDeque {
  private head = 0
  private used = 0

  private readonly indices: Float64Array
  private readonly values: Float64Array

  constructor(
    private readonly capacity: number,
    private readonly mode: 'min' | 'max',
  ) {
    this.indices = new Float64Array(capacity)
    this.values = new Float64Array(capacity)
  }

  clear() {
    this.head = 0
    this.used = 0
  }

  push(index: number, value: number) {
    while (this.used > 0 && this.shouldDiscardTail(value)) {
      --this.used
    }

    const physicalIndex = this.physicalIndex(this.used)
    this.indices[physicalIndex] = index
    this.values[physicalIndex] = value
    ++this.used
  }

  expireThrough(index: number) {
    while (this.used > 0 && this.indices[this.head]! <= index) {
      this.head = (this.head + 1) % this.capacity
      --this.used
    }
  }

  frontValue() {
    return this.used === 0 ? null : this.values[this.head]!
  }

  private shouldDiscardTail(value: number) {
    const tailValue = this.values[this.physicalIndex(this.used - 1)]!
    return this.mode === 'min' ? tailValue >= value : tailValue <= value
  }

  private physicalIndex(offset: number) {
    return (this.head + offset) % this.capacity
  }
}

function durationBucket(value: number) {
  const bucket = Math.round(value / HISTOGRAM_BUCKET_MS)
  return Math.max(0, Math.min(HISTOGRAM_BUCKET_COUNT - 1, bucket))
}

function histogramPercentile(histogram: Uint32Array, count: number, percentile: number) {
  const target = Math.max(1, Math.ceil(count * percentile))
  let seen = 0

  for (let index = 0; index < histogram.length; ++index) {
    seen += histogram[index]!
    if (seen >= target) {
      return index * HISTOGRAM_BUCKET_MS
    }
  }

  return HISTOGRAM_MAX_MS
}

function fillFrameTimelineFrames(
  samples: FrameSampleRing,
  now: number,
  windowMs: number,
  frames: FrameDiagnosticsTimelineFrame[],
) {
  const cutoff = now - windowMs
  let firstRetainedOffset = samples.length

  for (let offset = samples.length - 1; offset >= 0; --offset) {
    if (samples.endedAt(offset) < cutoff) break

    firstRetainedOffset = offset
  }

  let writeIndex = 0
  for (let offset = firstRetainedOffset; offset < samples.length; ++offset) {
    const frame = frames[writeIndex] ?? {
      index: 0,
      startedAtMs: 0,
      endedAtMs: 0,
      durationMs: 0,
    }

    samples.writeSampleAt(offset, frame)
    frames[writeIndex] = frame
    ++writeIndex
  }

  frames.length = writeIndex
}
