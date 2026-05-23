import { svgToBlob } from '@svg-fns/svg2img'

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
}

const loadImageBitmap = async (blob: Blob) => {
  if (typeof createImageBitmap !== 'function') {
    throw new Error('createImageBitmap is required to convert SVG output for MapLibre')
  }

  return createImageBitmap(blob)
}

export const svgToImage = async (
  svgSource: string,
  { width, height, pixelRatio = 1 }: SvgToImageOptions,
): Promise<ImageBitmap> => {
  const { blob } = await svgToBlob(svgSource, {
    format: 'png',
    width,
    height,
    scale: pixelRatio,
    fit: 'fill',
  })

  if (!blob) {
    throw new Error('SVG conversion did not produce an image blob')
  }

  return loadImageBitmap(blob)
}
