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

      <span class="text-muted">FPS, 5s avg</span>
      <span class="diagnostic-value">{{ fpsLabel }}</span>

      <span class="text-muted">Last frame</span>
      <span class="diagnostic-value">{{ currentFrameDurationLabel }}</span>
    </div>

    <div class="grid gap-1">
      <div class="flex items-center justify-between gap-3">
        <span class="text-muted">Frame time, trailing 10s</span>
        <span class="diagnostic-value">{{ timelineScaleLabel }}</span>
      </div>

      <svg
        class="timeline-chart"
        :viewBox="`0 0 ${TIMELINE_VIEW_WIDTH} ${TIMELINE_VIEW_HEIGHT}`"
        aria-label="Per-frame render duration timeline for the trailing 10 seconds"
        role="img"
      >
        <rect
          class="timeline-plot-background"
          :x="TIMELINE_AXIS_WIDTH"
          :y="TIMELINE_PLOT_TOP"
          :width="TIMELINE_PLOT_WIDTH"
          :height="TIMELINE_PLOT_HEIGHT"
          rx="2"
        />

        <g v-for="tick in timelineYAxisTicks" :key="tick.value">
          <line
            class="timeline-grid-line"
            :x1="TIMELINE_AXIS_WIDTH"
            :y1="tick.y"
            :x2="TIMELINE_VIEW_WIDTH"
            :y2="tick.y"
          />
          <text
            class="timeline-axis-label"
            :x="TIMELINE_AXIS_WIDTH - 5"
            :y="tick.y + 3"
            text-anchor="end"
          >
            {{ tick.label }}
          </text>
        </g>

        <g v-for="annotation in timelineAnnotations" :key="annotation.key">
          <line
            class="timeline-annotation-line"
            :x1="TIMELINE_AXIS_WIDTH"
            :y1="annotation.y"
            :x2="TIMELINE_VIEW_WIDTH"
            :y2="annotation.y"
            :stroke="annotation.color"
          />
          <text
            class="timeline-annotation-label"
            :x="TIMELINE_AXIS_WIDTH + 5"
            :y="annotation.y - 3"
            :fill="annotation.color"
          >
            {{ annotation.label }}
          </text>
        </g>

        <rect
          v-for="bar in timelineFrameBars"
          :key="bar.index"
          class="timeline-frame-bar"
          :x="bar.x"
          :y="bar.y"
          :width="bar.width"
          :height="bar.height"
          :fill="bar.color"
          :opacity="bar.opacity"
        />

        <line
          class="timeline-axis-line"
          :x1="TIMELINE_AXIS_WIDTH"
          :y1="TIMELINE_PLOT_BOTTOM"
          :x2="TIMELINE_VIEW_WIDTH"
          :y2="TIMELINE_PLOT_BOTTOM"
        />
        <text
          class="timeline-axis-label"
          :x="TIMELINE_AXIS_WIDTH"
          :y="TIMELINE_VIEW_HEIGHT - 4"
          text-anchor="start"
        >
          -10s
        </text>
        <text
          class="timeline-axis-label"
          :x="TIMELINE_VIEW_WIDTH"
          :y="TIMELINE_VIEW_HEIGHT - 4"
          text-anchor="end"
        >
          now
        </text>
      </svg>
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
import { computed } from 'vue'
import type { UseFrameDiagnosticsReturn } from '@/composables/frame-diagnostics.ts'

const TIMELINE_WINDOW_MS = 10_000
const TIMELINE_VIEW_WIDTH = 420
const TIMELINE_VIEW_HEIGHT = 136
const TIMELINE_AXIS_WIDTH = 50
const TIMELINE_PLOT_TOP = 8
const TIMELINE_PLOT_HEIGHT = 104
const TIMELINE_PLOT_BOTTOM = TIMELINE_PLOT_TOP + TIMELINE_PLOT_HEIGHT
const TIMELINE_PLOT_WIDTH = TIMELINE_VIEW_WIDTH - TIMELINE_AXIS_WIDTH
const TIMELINE_AXIS_HEADROOM_RATIO = 1.16
const TIMELINE_FRAME_COLOR = '#38bdf8'
const TIMELINE_WARN_COLOR = '#facc15'
const TIMELINE_MIN_COLOR = '#22c55e'
const TIMELINE_MAX_COLOR = '#f97316'

const props = defineProps<{
  diagnostics: UseFrameDiagnosticsReturn
}>()

const snapshot = computed(() => props.diagnostics.snapshot.value)
const currentFrame = computed(() => snapshot.value.currentFrame)

const currentFrameLabel = computed(() =>
  currentFrame.value ? padInt(currentFrame.value.index, 5) : '-----',
)
const currentFrameDurationLabel = computed(() => formatMs(currentFrame.value?.durationMs ?? null))
const fpsLabel = computed(() => formatFps(snapshot.value.fps))

const timelineWindowStats = computed(() => {
  if (snapshot.value.timelineFrames.length === 0) return null

  let minMs = Number.POSITIVE_INFINITY
  let maxMs = 0

  for (const frame of snapshot.value.timelineFrames) {
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

const timelineYAxisTicks = computed(() => {
  const maxMs = timelineMaxMs.value

  return [1, 0.75, 0.5, 0.25, 0].map((ratio) => {
    const value = maxMs * ratio

    return {
      value,
      y: timelineY(value),
      label: formatCompactMs(value),
    }
  })
})

const timelineAnnotations = computed(() => {
  const stats = timelineWindowStats.value
  if (!stats) return []

  return [
    stats.minMs === null
      ? null
      : {
          key: 'min',
          value: stats.minMs,
          y: timelineY(stats.minMs),
          color: TIMELINE_MIN_COLOR,
          label: `min ${formatCompactMs(stats.minMs)}`,
        },
    stats.maxMs === null
      ? null
      : {
          key: 'max',
          value: stats.maxMs,
          y: timelineY(stats.maxMs),
          color: TIMELINE_MAX_COLOR,
          label: `max ${formatCompactMs(stats.maxMs)}`,
        },
  ].filter((annotation) => annotation !== null)
})

const timelineFrameBars = computed(() => {
  const endMs = snapshot.value.sampledAtMs
  const startMs = endMs - TIMELINE_WINDOW_MS
  const maxMs = timelineMaxMs.value

  return snapshot.value.timelineFrames.flatMap((frame) => {
    const clippedStartMs = Math.max(frame.startedAtMs, startMs)
    const clippedEndMs = Math.min(frame.endedAtMs, endMs)
    const visibleDurationMs = clippedEndMs - clippedStartMs
    if (visibleDurationMs <= 0) return []

    const x = TIMELINE_AXIS_WIDTH + ((clippedStartMs - startMs) / TIMELINE_WINDOW_MS) * TIMELINE_PLOT_WIDTH
    const width = (visibleDurationMs / TIMELINE_WINDOW_MS) * TIMELINE_PLOT_WIDTH
    const height = Math.max(0.7, (frame.durationMs / maxMs) * TIMELINE_PLOT_HEIGHT)

    return [
      {
        index: frame.index,
        x,
        y: TIMELINE_PLOT_BOTTOM - height,
        width,
        height,
        opacity: Math.min(0.95, Math.max(0.28, frame.durationMs / maxMs)),
        color: frameDurationColor(frame.durationMs),
      },
    ]
  })
})

const statsRows = computed(() =>
  snapshot.value.stats.map((stat) => ({
    label: stat.label,
    sampleCount: padInt(stat.sampleCount, 5),
    minMs: formatMs(stat.minMs),
    avgMs: formatMs(stat.avgMs),
    p90Ms: formatMs(stat.p90Ms),
    maxMs: formatMs(stat.maxMs),
  })),
)

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

function timelineY(valueMs: number) {
  const clamped = Math.min(timelineMaxMs.value, Math.max(0, valueMs))
  return TIMELINE_PLOT_BOTTOM - (clamped / timelineMaxMs.value) * TIMELINE_PLOT_HEIGHT
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
  width: 100%;
  height: 8.5rem;
  overflow: hidden;
  border: 1px solid color-mix(in srgb, currentColor 18%, transparent);
  border-radius: 0.375rem;
  background: color-mix(in srgb, var(--ui-bg) 84%, transparent);
}

.timeline-plot-background {
  fill: color-mix(in srgb, var(--ui-bg) 90%, transparent);
}

.timeline-grid-line {
  stroke: color-mix(in srgb, currentColor 18%, transparent);
  stroke-dasharray: 3 3;
  stroke-width: 0.6;
  vector-effect: non-scaling-stroke;
}

.timeline-axis-line {
  stroke: color-mix(in srgb, currentColor 34%, transparent);
  stroke-width: 0.8;
  vector-effect: non-scaling-stroke;
}

.timeline-axis-label {
  fill: color-mix(in srgb, currentColor 58%, transparent);
  font-size: 10px;
  font-variant-numeric: tabular-nums;
}

.timeline-annotation-line {
  stroke-dasharray: 4 3;
  stroke-width: 1.2;
  vector-effect: non-scaling-stroke;
}

.timeline-annotation-label {
  font-size: 10px;
  font-weight: 700;
  paint-order: stroke;
  stroke: color-mix(in srgb, black 72%, transparent);
  stroke-width: 2.5;
  stroke-linejoin: round;
}

.timeline-frame-bar {
  shape-rendering: geometricprecision;
}

.timeline-chart text {
  font-family: monospace;
  font-variant-numeric: tabular-nums;
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
