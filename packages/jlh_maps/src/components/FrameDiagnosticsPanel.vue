<!-- WARNING: AI-SLOP -->
<template>
  <UCard
    variant="subtle"
    class="diagnostics-panel pointer-events-none w-[33rem] max-w-[calc(100vw-1rem)] font-mono text-[11px] leading-tight shadow-lg backdrop-blur"
    :ui="{ body: 'p-2.5 grid gap-2' }"
  >
    <div class="flex items-center justify-between gap-3">
      <span class="text-highlighted font-semibold tracking-wide">Frame diagnostics</span>
      <span class="text-muted">MapLibre render</span>
    </div>

    <div class="grid grid-cols-[1fr_auto] items-baseline gap-x-3 gap-y-1">
      <span class="text-muted">Frame</span>
      <span class="diagnostic-value">{{ currentFrameLabel }}</span>

      <span class="text-muted">FPS, 1s avg</span>
      <span class="diagnostic-value">{{ fpsLabel }}</span>

      <span class="text-muted">Last frame</span>
      <span class="diagnostic-value">{{ currentFrameDurationLabel }}</span>
    </div>

    <div class="grid gap-1">
      <div class="flex items-center justify-between gap-3">
        <span class="text-muted">Frame time, trailing 10s</span>
        <span class="diagnostic-value">{{ timelineScaleLabel }}</span>
      </div>

      <canvas
        ref="timelineCanvas"
        class="timeline-chart"
        aria-label="Per-frame render duration timeline for the trailing 10 seconds"
        role="img"
      />
    </div>

    <div class="grid gap-1">
      <div class="grid grid-cols-[3.5rem_repeat(5,minmax(0,1fr))] gap-x-2 text-muted">
        <span>Win</span>
        <span class="diagnostic-table-value">N</span>
        <span class="diagnostic-table-value">Min</span>
        <span class="diagnostic-table-value">Avg</span>
        <span class="diagnostic-table-value">P90</span>
        <span class="diagnostic-table-value">Max</span>
      </div>

      <div
        v-for="stat in statsRows"
        :key="stat.label"
        class="grid grid-cols-[3.5rem_repeat(5,minmax(0,1fr))] gap-x-2 text-default"
      >
        <span class="text-muted">{{ stat.label }}</span>
        <span class="diagnostic-table-value">{{ stat.sampleCount }}</span>
        <span class="diagnostic-table-value">{{ stat.minMs }}</span>
        <span class="diagnostic-table-value">{{ stat.avgMs }}</span>
        <span class="diagnostic-table-value">{{ stat.p90Ms }}</span>
        <span class="diagnostic-table-value">{{ stat.maxMs }}</span>
      </div>
    </div>
  </UCard>
</template>

<script setup lang="ts">
import { tryOnScopeDispose, useResizeObserver } from '@vueuse/core'
import { computed, ref, watchPostEffect } from 'vue'
import type {
  FrameDiagnosticsSnapshot,
  UseFrameDiagnosticsReturn,
} from '@/composables/frame-diagnostics.ts'

const TIMELINE_WINDOW_MS = 10_000
const TIMELINE_RENDER_INTERVAL_MS = 250
const TIMELINE_AXIS_WIDTH = 50
const TIMELINE_PLOT_TOP = 8
const TIMELINE_BOTTOM_AXIS_HEIGHT = 24
const TIMELINE_AXIS_HEADROOM_RATIO = 1.16
const TIMELINE_FRAME_COLOR = '#38bdf8'
const TIMELINE_WARN_COLOR = '#facc15'
const TIMELINE_MIN_COLOR = '#22c55e'
const TIMELINE_MAX_COLOR = '#f97316'
const TIMELINE_GRID_COLOR = 'rgba(148, 163, 184, 0.24)'
const TIMELINE_AXIS_COLOR = 'rgba(148, 163, 184, 0.48)'
const TIMELINE_LABEL_COLOR = 'rgba(203, 213, 225, 0.72)'
const TIMELINE_PLOT_BACKGROUND = 'rgba(15, 23, 42, 0.18)'

const props = defineProps<{
  diagnostics: UseFrameDiagnosticsReturn
}>()

const timelineCanvas = ref<HTMLCanvasElement | null>(null)
let lastTimelineRenderAtMs = Number.NEGATIVE_INFINITY
let timelineResizeAnimationFrame: number | null = null

const currentFrameLabel = computed(() => {
  const currentFrame = props.diagnostics.snapshot.value.currentFrame
  return currentFrame ? padInt(currentFrame.index, 5) : '-----'
})
const currentFrameDurationLabel = computed(() =>
  formatMs(props.diagnostics.snapshot.value.currentFrame?.durationMs ?? null),
)
const fpsLabel = computed(() => formatFps(props.diagnostics.snapshot.value.fps))

const timelineWindowStats = computed(() => {
  const snapshot = props.diagnostics.snapshot.value
  if (snapshot.timelineFrames.length === 0) return null

  let minMs = Number.POSITIVE_INFINITY
  let maxMs = 0

  for (const frame of snapshot.timelineFrames) {
    minMs = Math.min(minMs, frame.durationMs)
    maxMs = Math.max(maxMs, frame.durationMs)
  }

  return {
    minMs,
    maxMs,
  }
})

const timelineMaxMs = computed(() => {
  const stats = timelineWindowStats.value
  if (!stats) return 16.7

  return Math.max(0.1, stats.maxMs * TIMELINE_AXIS_HEADROOM_RATIO)
})

const timelineScaleLabel = computed(() => `0-${formatMs(timelineMaxMs.value)}`)

const statsRows = computed(() =>
  props.diagnostics.snapshot.value.stats.map((stat) => ({
    label: stat.label,
    sampleCount: padInt(stat.sampleCount, 5),
    minMs: formatMs(stat.minMs),
    avgMs: formatMs(stat.avgMs),
    p90Ms: formatMs(stat.p90Ms),
    maxMs: formatMs(stat.maxMs),
  })),
)

watchPostEffect(() => {
  renderTimeline()
})

useResizeObserver(timelineCanvas, () => {
  if (timelineResizeAnimationFrame !== null) return

  timelineResizeAnimationFrame = requestAnimationFrame(() => {
    timelineResizeAnimationFrame = null
    renderTimeline(true)
  })
})

tryOnScopeDispose(() => {
  if (timelineResizeAnimationFrame !== null) {
    cancelAnimationFrame(timelineResizeAnimationFrame)
    timelineResizeAnimationFrame = null
  }
})

function padInt(value: number, width: number) {
  return Math.trunc(value).toString().padStart(width, ' ')
}

function formatFps(value: number | null) {
  if (value === null) return '--.-'
  return value.toFixed(1).padStart(4, ' ')
}

function formatMs(value: number | null) {
  if (value === null) return '---.-ms'
  return `${value.toFixed(1).padStart(5, ' ')}ms`
}

function formatCompactMs(value: number) {
  return `${value.toFixed(1)}ms`
}

function renderTimeline(force = false) {
  const snapshot = props.diagnostics.snapshot.value
  if (!force && snapshot.sampledAtMs - lastTimelineRenderAtMs < TIMELINE_RENDER_INTERVAL_MS) {
    return
  }

  lastTimelineRenderAtMs = snapshot.sampledAtMs
  drawTimeline(
    timelineCanvas.value,
    snapshot,
    timelineMaxMs.value,
    timelineWindowStats.value,
  )
}

type TimelineBounds = {
  width: number
  height: number
  axisWidth: number
  plotTop: number
  plotBottom: number
  plotHeight: number
  plotWidth: number
}

type TimelineStats = {
  minMs: number
  maxMs: number
} | null

function drawTimeline(
  canvas: HTMLCanvasElement | null,
  snapshot: FrameDiagnosticsSnapshot,
  maxMs: number,
  stats: TimelineStats,
) {
  if (!canvas) return

  const width = Math.max(1, Math.floor(canvas.clientWidth))
  const height = Math.max(1, Math.floor(canvas.clientHeight))
  const pixelRatio = globalThis.devicePixelRatio || 1
  const pixelWidth = Math.max(1, Math.floor(width * pixelRatio))
  const pixelHeight = Math.max(1, Math.floor(height * pixelRatio))

  if (canvas.width !== pixelWidth || canvas.height !== pixelHeight) {
    canvas.width = pixelWidth
    canvas.height = pixelHeight
  }

  const ctx = canvas.getContext('2d')
  if (!ctx) return

  ctx.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0)
  ctx.clearRect(0, 0, width, height)

  const bounds = timelineBounds(width, height)
  drawTimelineBackground(ctx, bounds)
  drawTimelineGrid(ctx, bounds, maxMs)
  drawTimelineAnnotations(ctx, bounds, maxMs, stats)
  drawTimelineBars(ctx, bounds, snapshot, maxMs)
  drawTimelineAxis(ctx, bounds)
}

function timelineBounds(width: number, height: number): TimelineBounds {
  const axisWidth = Math.min(TIMELINE_AXIS_WIDTH, Math.max(32, width * 0.25))
  const plotTop = TIMELINE_PLOT_TOP
  const plotBottom = Math.max(plotTop + 1, height - TIMELINE_BOTTOM_AXIS_HEIGHT)

  return {
    width,
    height,
    axisWidth,
    plotTop,
    plotBottom,
    plotHeight: plotBottom - plotTop,
    plotWidth: Math.max(1, width - axisWidth),
  }
}

function drawTimelineBackground(ctx: CanvasRenderingContext2D, bounds: TimelineBounds) {
  ctx.fillStyle = TIMELINE_PLOT_BACKGROUND
  ctx.fillRect(bounds.axisWidth, bounds.plotTop, bounds.plotWidth, bounds.plotHeight)
}

function drawTimelineGrid(ctx: CanvasRenderingContext2D, bounds: TimelineBounds, maxMs: number) {
  ctx.save()
  ctx.font = '10px monospace'
  ctx.textBaseline = 'middle'
  ctx.textAlign = 'right'
  ctx.strokeStyle = TIMELINE_GRID_COLOR
  ctx.fillStyle = TIMELINE_LABEL_COLOR
  ctx.lineWidth = 0.75
  ctx.setLineDash([3, 3])

  for (const ratio of [1, 0.75, 0.5, 0.25, 0]) {
    const value = maxMs * ratio
    const y = timelineY(value, maxMs, bounds)

    ctx.beginPath()
    ctx.moveTo(bounds.axisWidth, y)
    ctx.lineTo(bounds.width, y)
    ctx.stroke()
    ctx.fillText(formatCompactMs(value), bounds.axisWidth - 5, y)
  }

  ctx.restore()
}

function drawTimelineAnnotations(
  ctx: CanvasRenderingContext2D,
  bounds: TimelineBounds,
  maxMs: number,
  stats: TimelineStats,
) {
  if (!stats) return

  drawTimelineAnnotation(
    ctx,
    bounds,
    timelineY(stats.minMs, maxMs, bounds),
    TIMELINE_MIN_COLOR,
    `min ${formatCompactMs(stats.minMs)}`,
  )
  drawTimelineAnnotation(
    ctx,
    bounds,
    timelineY(stats.maxMs, maxMs, bounds),
    TIMELINE_MAX_COLOR,
    `max ${formatCompactMs(stats.maxMs)}`,
  )
}

function drawTimelineAnnotation(
  ctx: CanvasRenderingContext2D,
  bounds: TimelineBounds,
  y: number,
  color: string,
  label: string,
) {
  ctx.save()
  ctx.strokeStyle = color
  ctx.fillStyle = color
  ctx.lineWidth = 1.2
  ctx.setLineDash([4, 3])

  ctx.beginPath()
  ctx.moveTo(bounds.axisWidth, y)
  ctx.lineTo(bounds.width, y)
  ctx.stroke()

  ctx.font = '700 10px monospace'
  ctx.textAlign = 'left'
  ctx.textBaseline = 'middle'

  const paddingX = 5
  const labelHeight = 16
  const labelWidth = Math.ceil(ctx.measureText(label).width) + paddingX * 2
  const labelX = bounds.axisWidth + 5
  const labelY = Math.max(
    bounds.plotTop + 1,
    Math.min(bounds.plotBottom - labelHeight - 1, y - labelHeight - 4),
  )

  drawRoundedRect(ctx, labelX, labelY, labelWidth, labelHeight, 4)
  ctx.fillStyle = 'rgba(2, 6, 23, 0.88)'
  ctx.fill()
  ctx.strokeStyle = color
  ctx.lineWidth = 1
  ctx.stroke()

  ctx.fillStyle = color
  ctx.fillRect(labelX, labelY, 3, labelHeight)
  ctx.fillStyle = '#f8fafc'
  ctx.fillText(label, labelX + paddingX + 2, labelY + labelHeight / 2 + 0.5)
  ctx.restore()
}

function drawTimelineBars(
  ctx: CanvasRenderingContext2D,
  bounds: TimelineBounds,
  snapshot: FrameDiagnosticsSnapshot,
  maxMs: number,
) {
  const endMs = snapshot.sampledAtMs
  const startMs = endMs - TIMELINE_WINDOW_MS

  for (const frame of snapshot.timelineFrames) {
    const clippedStartMs = Math.max(frame.startedAtMs, startMs)
    const clippedEndMs = Math.min(frame.endedAtMs, endMs)
    const visibleDurationMs = clippedEndMs - clippedStartMs
    if (visibleDurationMs <= 0) continue

    const x =
      bounds.axisWidth + ((clippedStartMs - startMs) / TIMELINE_WINDOW_MS) * bounds.plotWidth
    const width = (visibleDurationMs / TIMELINE_WINDOW_MS) * bounds.plotWidth
    const height = Math.max(0.7, (frame.durationMs / maxMs) * bounds.plotHeight)
    const y = bounds.plotBottom - height

    ctx.globalAlpha = Math.min(0.95, Math.max(0.28, frame.durationMs / maxMs))
    ctx.fillStyle = frameDurationColor(frame.durationMs)
    ctx.fillRect(x, y, width, height)
  }

  ctx.globalAlpha = 1
}

function drawTimelineAxis(ctx: CanvasRenderingContext2D, bounds: TimelineBounds) {
  ctx.save()
  ctx.strokeStyle = TIMELINE_AXIS_COLOR
  ctx.fillStyle = TIMELINE_LABEL_COLOR
  ctx.lineWidth = 0.8
  ctx.setLineDash([])

  ctx.beginPath()
  ctx.moveTo(bounds.axisWidth, bounds.plotBottom)
  ctx.lineTo(bounds.width, bounds.plotBottom)
  ctx.stroke()

  ctx.font = '10px monospace'
  ctx.textBaseline = 'alphabetic'
  ctx.textAlign = 'left'
  ctx.fillText('-10s', bounds.axisWidth, bounds.height - 4)
  ctx.textAlign = 'right'
  ctx.fillText('now', bounds.width, bounds.height - 4)
  ctx.restore()
}

function timelineY(valueMs: number, maxMs: number, bounds: TimelineBounds) {
  const clamped = Math.min(maxMs, Math.max(0, valueMs))
  return bounds.plotBottom - (clamped / maxMs) * bounds.plotHeight
}

function drawRoundedRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number,
) {
  const clampedRadius = Math.min(radius, width / 2, height / 2)

  ctx.beginPath()
  ctx.moveTo(x + clampedRadius, y)
  ctx.lineTo(x + width - clampedRadius, y)
  ctx.quadraticCurveTo(x + width, y, x + width, y + clampedRadius)
  ctx.lineTo(x + width, y + height - clampedRadius)
  ctx.quadraticCurveTo(x + width, y + height, x + width - clampedRadius, y + height)
  ctx.lineTo(x + clampedRadius, y + height)
  ctx.quadraticCurveTo(x, y + height, x, y + height - clampedRadius)
  ctx.lineTo(x, y + clampedRadius)
  ctx.quadraticCurveTo(x, y, x + clampedRadius, y)
  ctx.closePath()
}

function frameDurationColor(valueMs: number) {
  if (valueMs >= 33.3) return TIMELINE_MAX_COLOR
  if (valueMs >= 16.7) return TIMELINE_WARN_COLOR
  return TIMELINE_FRAME_COLOR
}
</script>

<style scoped>
.diagnostics-panel {
  background: color-mix(in srgb, var(--ui-bg) 60%, transparent);
  border-color: color-mix(in srgb, currentColor 28%, transparent);
}

.timeline-chart {
  display: block;
  width: 100%;
  height: 8.5rem;
  overflow: hidden;
  border: 1px solid color-mix(in srgb, currentColor 18%, transparent);
  border-radius: 0.375rem;
  background: color-mix(in srgb, var(--ui-bg) 84%, transparent);
}

.diagnostic-value {
  min-width: 7ch;
  text-align: right;
  font-variant-numeric: tabular-nums;
  white-space: pre;
}

.diagnostic-table-value {
  min-width: 7ch;
  text-align: right;
  font-variant-numeric: tabular-nums;
  white-space: pre;
}
</style>
