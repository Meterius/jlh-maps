import {
  escapeSvgAttribute,
  getSvgPresentationAttributes,
  parseSvgElementOrThrow,
  parseSvgViewBox,
  SVG_NAMESPACE,
} from '@/utils/svg.ts'

export type SvgImageBoundingBox = {
  x: number
  y: number
  width: number
  height: number
}

export type SvgToImageOptions = {
  width: number
  height?: number
  pixelRatio?: number
  color?: string
  sourceIsRenderable?: boolean
}

export type SvgRasterImage = {
  width: number
  height: number
  data: Uint8Array | Uint8ClampedArray
}

export type SvgToImageResult = {
  image: SvgRasterImage
}

type SvgRasterizeRequest = {
  svgSource: string
  width: number
  height: number
}

type SvgRasterizeResponse =
  | {
      id: number
      width: number
      height: number
      buffer: ArrayBuffer
    }
  | {
      id: number
      error: string
    }

type WorkerRequestCallbacks = {
  resolve: (image: SvgRasterImage) => void
  reject: (error: Error) => void
}

type SvgRasterizerWorker = {
  worker: Worker
  requests: Map<number, WorkerRequestCallbacks>
}

const MAX_RASTERIZE_CONCURRENCY = 3
const MAX_WORKER_COUNT = 3

let nextRasterizeRequestId = 1
let nextWorkerIndex = 0
let workerRasterizerUnavailable = false
let workerPool: SvgRasterizerWorker[] | undefined
let activeRasterizeTasks = 0
const rasterizeTaskQueue: Array<() => void> = []

const buildRenderableSvg = (
  svg: SVGSVGElement,
  { width, height, color }: { width: number; height: number; color: string },
) =>
  `
<svg xmlns="${SVG_NAMESPACE}" width="${width}" height="${height}" viewBox="${parseSvgViewBox(svg)}" color="${escapeSvgAttribute(color)}">
  <g ${getSvgPresentationAttributes(svg)}>${svg.innerHTML}</g>
</svg>`.trim()

const loadSvgImage = async (svgSource: string) => {
  const url = URL.createObjectURL(new Blob([svgSource], { type: 'image/svg+xml' }))
  const image = new Image()

  image.src = url
  await image.decode()

  return {
    image,
    revoke: () => URL.revokeObjectURL(url),
  }
}

const getWorkerCount = () =>
  Math.max(1, Math.min(MAX_WORKER_COUNT, globalThis.navigator?.hardwareConcurrency ?? 1))

const rejectWorkerRequests = (rasterizer: SvgRasterizerWorker, error: Error) => {
  rasterizer.requests.forEach(({ reject }) => reject(error))
  rasterizer.requests.clear()
}

const disposeWorkerPool = (error?: Error) => {
  workerPool?.forEach((rasterizer) => {
    if (error) {
      rejectWorkerRequests(rasterizer, error)
    }

    rasterizer.worker.terminate()
  })
  workerPool = undefined
}

const makeWorkerRasterizerUnavailable = (error?: Error) => {
  workerRasterizerUnavailable = true
  disposeWorkerPool(error)
}

const createWorkerRasterizer = () => {
  const rasterizer: SvgRasterizerWorker = {
    worker: new Worker(new URL('./svg-to-image.worker.ts', import.meta.url), { type: 'module' }),
    requests: new Map(),
  }

  rasterizer.worker.onmessage = (event: MessageEvent<SvgRasterizeResponse>) => {
    const response = event.data
    const callbacks = rasterizer.requests.get(response.id)
    if (!callbacks) return

    rasterizer.requests.delete(response.id)

    if ('error' in response) {
      callbacks.reject(new Error(response.error))
      return
    }

    callbacks.resolve({
      width: response.width,
      height: response.height,
      data: new Uint8ClampedArray(response.buffer),
    })
  }

  rasterizer.worker.onerror = (event) => {
    makeWorkerRasterizerUnavailable(
      new Error(event.message || 'SVG rasterizer worker failed unexpectedly'),
    )
  }

  return rasterizer
}

const getWorkerPool = () => {
  if (workerRasterizerUnavailable || typeof Worker === 'undefined') return undefined

  try {
    workerPool ??= Array.from({ length: getWorkerCount() }, createWorkerRasterizer)
    return workerPool
  } catch (error) {
    makeWorkerRasterizerUnavailable(
      error instanceof Error ? error : new Error('Failed to create SVG rasterizer worker'),
    )
    return undefined
  }
}

const rasterizeSvgInWorker = (request: SvgRasterizeRequest) =>
  new Promise<SvgRasterImage>((resolve, reject) => {
    const pool = getWorkerPool()
    if (!pool?.length) {
      reject(new Error('SVG rasterizer worker is unavailable'))
      return
    }

    const rasterizer = pool[nextWorkerIndex % pool.length]
    nextWorkerIndex += 1

    if (!rasterizer) {
      reject(new Error('SVG rasterizer worker is unavailable'))
      return
    }

    const id = nextRasterizeRequestId++
    rasterizer.requests.set(id, { resolve, reject })
    rasterizer.worker.postMessage({ id, ...request })
  })

const rasterizeSvgOnMainThread = async ({
  svgSource,
  width,
  height,
}: SvgRasterizeRequest): Promise<SvgRasterImage> => {
  const loadedImage = await loadSvgImage(svgSource)
  const canvas = document.createElement('canvas')
  canvas.width = width
  canvas.height = height

  const ctx = canvas.getContext('2d')
  if (!ctx) {
    loadedImage.revoke()

    return {
      width,
      height,
      data: new Uint8ClampedArray(width * height * 4),
    }
  }

  try {
    ctx.drawImage(loadedImage.image, 0, 0, width, height)
    const image = ctx.getImageData(0, 0, width, height)

    return {
      width,
      height,
      data: image.data,
    }
  } finally {
    loadedImage.revoke()
  }
}

const rasterizeSvg = async (request: SvgRasterizeRequest) => {
  if (!workerRasterizerUnavailable) {
    try {
      return await rasterizeSvgInWorker(request)
    } catch (error) {
      makeWorkerRasterizerUnavailable(
        error instanceof Error ? error : new Error('SVG rasterizer worker failed'),
      )
    }
  }

  return rasterizeSvgOnMainThread(request)
}

const drainRasterizeTaskQueue = () => {
  while (activeRasterizeTasks < MAX_RASTERIZE_CONCURRENCY) {
    const task = rasterizeTaskQueue.shift()
    if (!task) return

    activeRasterizeTasks += 1
    task()
  }
}

const enqueueRasterizeTask = (request: SvgRasterizeRequest) =>
  new Promise<SvgRasterImage>((resolve, reject) => {
    rasterizeTaskQueue.push(() => {
      rasterizeSvg(request)
        .then(resolve, reject)
        .finally(() => {
          activeRasterizeTasks -= 1
          drainRasterizeTaskQueue()
        })
    })

    drainRasterizeTaskQueue()
  })

export const svgToImage = async (
  svgSource: string,
  {
    width,
    height = width,
    pixelRatio = 1,
    color = 'currentColor',
    sourceIsRenderable = false,
  }: SvgToImageOptions,
): Promise<SvgToImageResult> => {
  const image = await enqueueRasterizeTask({
    svgSource: sourceIsRenderable
      ? svgSource
      : buildRenderableSvg(parseSvgElementOrThrow(svgSource), { width, height, color }),
    width: Math.ceil(width * pixelRatio),
    height: Math.ceil(height * pixelRatio),
  })

  return {
    image,
  }
}
