export const SVG_NAMESPACE = 'http://www.w3.org/2000/svg'

export const escapeSvgAttribute = (value: string) =>
  value
    .replaceAll('&', '&amp;')
    .replaceAll('"', '&quot;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')

export const DEFAULT_SVG_PRESENTATION_ATTRIBUTES = [
  'clip-rule',
  'fill',
  'fill-rule',
  'stroke',
  'stroke-linecap',
  'stroke-linejoin',
  'stroke-miterlimit',
  'stroke-width',
  'style',
] as const

export const parseSvgElement = (svgSource: string) => {
  const svg = new DOMParser().parseFromString(svgSource, 'image/svg+xml').documentElement

  if (svg.tagName.toLowerCase() !== 'svg') return undefined

  return svg as unknown as SVGSVGElement
}

export const parseSvgElementOrThrow = (svgSource: string) => {
  const svg = parseSvgElement(svgSource)
  if (!svg) throw new Error('Expected SVG source to parse into an <svg> element')

  return svg
}

export const getSvgPresentationAttributes = (
  svg: SVGSVGElement,
  attributes: readonly string[] = DEFAULT_SVG_PRESENTATION_ATTRIBUTES,
) =>
  attributes
    .flatMap((name) => {
      const value = svg.getAttribute(name)
      if (value === null) return []

      return [`${name}="${escapeSvgAttribute(value)}"`]
    })
    .join(' ')

export const parseSvgViewBox = (svg: SVGSVGElement) => {
  const values = svg
    .getAttribute('viewBox')
    ?.trim()
    .split(/\s+/)
    .map((value) => Number(value))

  if (values?.length === 4 && values.every((value) => Number.isFinite(value))) {
    const [x, y, width, height] = values as [number, number, number, number]
    return `${x} ${y} ${width} ${height}`
  }

  return `0 0 ${Number(svg.getAttribute('width') ?? 16)} ${Number(svg.getAttribute('height') ?? 16)}`
}
