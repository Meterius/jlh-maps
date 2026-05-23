import {
  escapeSvgAttribute,
  getSvgPresentationAttributes,
  parseSvgElement,
  SVG_NAMESPACE,
} from '@/utils/svg.ts'

type ParsedMarkerIconSvg = {
  innerHtml: string
  presentationAttributes: string
  viewBox: string
}

export type MarkerIconOptions = {
  width?: number
  height?: number
  color?: string
  iconColor?: string
  headColor?: string
  outlineColor?: string
  shadowColor?: string
  iconScale?: number
  iconBaseSize?: number
  headRadius?: number
  headCenterX?: number
  headCenterY?: number
  markerPath?: string
  viewBox?: string
}

const DEFAULT_ICON_PRESENTATION =
  'fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"'

const DEFAULT_MARKER_PATH =
  'M16 34C15.25 34 14.55 33.68 13.95 33.05C11.3 30.3 4 22.65 4 15.8C4 8.85 9.37 3.5 16 3.5C22.63 3.5 28 8.85 28 15.8C28 22.65 20.7 30.3 18.05 33.05C17.45 33.68 16.75 34 16 34Z'

export const DEFAULT_MARKER_ICON_OPTIONS = {
  width: 32,
  height: 36,
  color: '#2563eb',
  iconColor: '#2563eb',
  headColor: '#ffffff',
  outlineColor: 'rgb(15 23 42 / 0.22)',
  shadowColor: 'rgb(15 23 42 / 0.26)',
  iconScale: 1,
  iconBaseSize: 14,
  headRadius: 9,
  headCenterX: 16,
  headCenterY: 16,
  markerPath: DEFAULT_MARKER_PATH,
  viewBox: '0 0 32 36',
} satisfies Required<MarkerIconOptions>

const parsedMarkerIconSvgCache = new Map<string, ParsedMarkerIconSvg>()

const parseMarkerIconSvg = (source: string | undefined): ParsedMarkerIconSvg => {
  if (!source) {
    return {
      innerHtml: '',
      presentationAttributes: DEFAULT_ICON_PRESENTATION,
      viewBox: '0 0 24 24',
    }
  }

  const cached = parsedMarkerIconSvgCache.get(source)
  if (cached) return cached

  const svg = parseSvgElement(source)
  const parsed = svg
    ? {
        innerHtml: svg.innerHTML,
        presentationAttributes: getSvgPresentationAttributes(svg),
        viewBox: svg.getAttribute('viewBox') ?? '0 0 24 24',
      }
    : {
        innerHtml: '',
        presentationAttributes: DEFAULT_ICON_PRESENTATION,
        viewBox: '0 0 24 24',
      }

  parsedMarkerIconSvgCache.set(source, parsed)
  return parsed
}

const formatSvgNumber = (value: number) => Number(value.toFixed(3)).toString()

const getMarkerIconBounds = ({
  iconBaseSize,
  iconScale,
  headCenterX,
  headCenterY,
}: Required<MarkerIconOptions>) => {
  const iconSize = iconBaseSize * iconScale

  return {
    size: formatSvgNumber(iconSize),
    x: formatSvgNumber(headCenterX - iconSize / 2),
    y: formatSvgNumber(headCenterY - iconSize / 2),
  }
}

export const makeMarkerIcon = (
  iconSvg: string | undefined,
  marker: Required<MarkerIconOptions>,
) => {
  const { innerHtml, presentationAttributes, viewBox } = parseMarkerIconSvg(iconSvg)
  const iconBounds = getMarkerIconBounds(marker)

  return `
<svg xmlns="${SVG_NAMESPACE}" width="${marker.width}" height="${marker.height}" viewBox="${escapeSvgAttribute(marker.viewBox)}">
  <ellipse cx="16" cy="34.5" rx="7" ry="1.5" fill="${escapeSvgAttribute(marker.shadowColor)}"/>
  <path d="${escapeSvgAttribute(marker.markerPath)}" fill="${escapeSvgAttribute(marker.color)}"/>
  <path d="${escapeSvgAttribute(marker.markerPath)}" fill="none" stroke="${escapeSvgAttribute(marker.outlineColor)}" stroke-width="1"/>
  <circle cx="${formatSvgNumber(marker.headCenterX)}" cy="${formatSvgNumber(marker.headCenterY)}" r="${formatSvgNumber(marker.headRadius)}" fill="${escapeSvgAttribute(marker.headColor)}"/>
  <svg x="${iconBounds.x}" y="${iconBounds.y}" width="${iconBounds.size}" height="${iconBounds.size}" viewBox="${escapeSvgAttribute(viewBox)}" color="${escapeSvgAttribute(marker.iconColor)}" ${presentationAttributes}>
    ${innerHtml}
  </svg>
</svg>`.trim()
}
