type SvgRasterizeRequest = {
  id: number
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

const workerSelf = self as unknown as {
  postMessage: (message: SvgRasterizeResponse, transfer?: Transferable[]) => void
  onmessage: ((event: MessageEvent<SvgRasterizeRequest>) => void) | null
}

const getTransferableBuffer = (data: Uint8ClampedArray) => {
  if (data.byteOffset === 0 && data.byteLength === data.buffer.byteLength) {
    return data.buffer as ArrayBuffer
  }

  return data.slice().buffer
}

const rasterizeSvg = async ({ svgSource, width, height }: SvgRasterizeRequest) => {
  const bitmap = await createImageBitmap(new Blob([svgSource], { type: 'image/svg+xml' }))
  const canvas = new OffscreenCanvas(width, height)
  const ctx = canvas.getContext('2d', { willReadFrequently: true })

  if (!ctx) {
    bitmap.close()
    return new ArrayBuffer(width * height * 4)
  }

  try {
    ctx.drawImage(bitmap, 0, 0, width, height)
    const imageData = ctx.getImageData(0, 0, width, height)

    return getTransferableBuffer(imageData.data)
  } finally {
    bitmap.close()
  }
}

workerSelf.onmessage = (event) => {
  rasterizeSvg(event.data).then(
    (buffer) => {
      workerSelf.postMessage(
        {
          id: event.data.id,
          width: event.data.width,
          height: event.data.height,
          buffer,
        },
        [buffer],
      )
    },
    (error: unknown) => {
      workerSelf.postMessage({
        id: event.data.id,
        error: error instanceof Error ? error.message : String(error),
      })
    },
  )
}

export {}
