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

export enum MarkerShape {
  Pin = 'pin',
  Box = 'box',
}

export type MarkerOptions = {
  width: number
  height: number

  color: string
  backgroundColor: string
  outlineColor: string

  iconColor: string
  headIconRatio: number

  headPadding: number
} & (
  | {
      shape: MarkerShape.Pin
      shadowColor: string
    }
  | {
      shape: MarkerShape.Box
    }
)

const DEFAULT_ICON_PRESENTATION =
  'fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"'

export const DEFAULT_PIN_MARKER_ICON_OPTIONS = {
  shape: MarkerShape.Pin,
  width: 48,
  height: 56,
  color: '#2563eb',
  backgroundColor: '#ffffff',
  outlineColor: 'rgb(15 23 42 / 0.22)',
  iconColor: '#2563eb',
  headIconRatio: 0.75,
  headPadding: 4,
  shadowColor: 'rgb(15 23 42 / 0.26)',
} satisfies MarkerOptions

export const DEFAULT_BOX_MARKER_ICON_OPTIONS = {
  shape: MarkerShape.Box,
  width: 48,
  height: 48,
  color: '#2563eb',
  backgroundColor: '#ffffff',
  outlineColor: 'rgb(15 23 42 / 0.22)',
  iconColor: '#2563eb',
  headIconRatio: 1,
  headPadding: 4,
} satisfies MarkerOptions

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

type MarkerIconBounds = {
  size: string
  x: string
  y: string
}

type MarkerGeometry = {
  markerPath: string
  headPath: string
  iconBounds: MarkerIconBounds
}

const makeCirclePath = (centerX: number, centerY: number, radius: number) => {
  const safeRadius = Math.max(radius, 0.001)

  return [
    `M ${formatSvgNumber(centerX)} ${formatSvgNumber(centerY - safeRadius)}`,
    `A ${formatSvgNumber(safeRadius)} ${formatSvgNumber(safeRadius)} 0 1 1 ${formatSvgNumber(centerX)} ${formatSvgNumber(centerY + safeRadius)}`,
    `A ${formatSvgNumber(safeRadius)} ${formatSvgNumber(safeRadius)} 0 1 1 ${formatSvgNumber(centerX)} ${formatSvgNumber(centerY - safeRadius)}`,
    'Z',
  ].join(' ')
}

const makeRoundedRectPath = (
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number,
) => {
  const safeRadius = Math.max(0, Math.min(radius, width / 2, height / 2))
  const right = x + width
  const bottom = y + height

  return [
    `M ${formatSvgNumber(x + safeRadius)} ${formatSvgNumber(y)}`,
    `H ${formatSvgNumber(right - safeRadius)}`,
    `Q ${formatSvgNumber(right)} ${formatSvgNumber(y)} ${formatSvgNumber(right)} ${formatSvgNumber(y + safeRadius)}`,
    `V ${formatSvgNumber(bottom - safeRadius)}`,
    `Q ${formatSvgNumber(right)} ${formatSvgNumber(bottom)} ${formatSvgNumber(right - safeRadius)} ${formatSvgNumber(bottom)}`,
    `H ${formatSvgNumber(x + safeRadius)}`,
    `Q ${formatSvgNumber(x)} ${formatSvgNumber(bottom)} ${formatSvgNumber(x)} ${formatSvgNumber(bottom - safeRadius)}`,
    `V ${formatSvgNumber(y + safeRadius)}`,
    `Q ${formatSvgNumber(x)} ${formatSvgNumber(y)} ${formatSvgNumber(x + safeRadius)} ${formatSvgNumber(y)}`,
    'Z',
  ].join(' ')
}

const makePinPath = (width: number, height: number) => {
  const center = width / 2
  const radius = width / 2
  const tailHeight = height - width

  if (tailHeight <= 0) return makeCirclePath(center, center, radius)

  const bottomRadius = (radius * tailHeight) / height
  const bottomCenterY = height - bottomRadius
  const centerDistance = bottomCenterY - center
  const tangentNormalY = (radius - bottomRadius) / centerDistance
  const tangentNormalX = Math.sqrt(1 - tangentNormalY ** 2)
  const rightHeadTangent = {
    x: center + radius * tangentNormalX,
    y: center + radius * tangentNormalY,
  }
  const leftHeadTangent = {
    x: center - radius * tangentNormalX,
    y: center + radius * tangentNormalY,
  }
  const rightTipTangent = {
    x: center + bottomRadius * tangentNormalX,
    y: bottomCenterY + bottomRadius * tangentNormalY,
  }
  const leftTipTangent = {
    x: center - bottomRadius * tangentNormalX,
    y: bottomCenterY + bottomRadius * tangentNormalY,
  }

  return [
    `M ${formatSvgNumber(rightHeadTangent.x)} ${formatSvgNumber(rightHeadTangent.y)}`,
    `A ${formatSvgNumber(radius)} ${formatSvgNumber(radius)} 0 1 0 ${formatSvgNumber(leftHeadTangent.x)} ${formatSvgNumber(leftHeadTangent.y)}`,
    `L ${formatSvgNumber(leftTipTangent.x)} ${formatSvgNumber(leftTipTangent.y)}`,
    `A ${formatSvgNumber(bottomRadius)} ${formatSvgNumber(bottomRadius)} 0 0 0 ${formatSvgNumber(rightTipTangent.x)} ${formatSvgNumber(rightTipTangent.y)}`,
    `L ${formatSvgNumber(rightHeadTangent.x)} ${formatSvgNumber(rightHeadTangent.y)}`,
    'Z',
  ].join(' ')
}

const makeCenteredIconBounds = (
  centerX: number,
  centerY: number,
  maxSize: number,
  ratio: number,
): MarkerIconBounds => {
  const iconSize = maxSize * Math.max(0, Math.min(ratio, 1))

  return {
    size: formatSvgNumber(iconSize),
    x: formatSvgNumber(centerX - iconSize / 2),
    y: formatSvgNumber(centerY - iconSize / 2),
  }
}

const makePinGeometry = (marker: Extract<MarkerOptions, { shape: MarkerShape.Pin }>) => {
  const center = marker.width / 2
  const headRadius = Math.max(0, marker.width / 2 - marker.headPadding)

  return {
    markerPath: makePinPath(marker.width, marker.height),
    headPath: makeCirclePath(center, center, headRadius),
    iconBounds: makeCenteredIconBounds(center, center, headRadius * 2, marker.headIconRatio),
  } satisfies MarkerGeometry
}

const makeBoxGeometry = (marker: Extract<MarkerOptions, { shape: MarkerShape.Box }>) => {
  const headWidth = Math.max(0, marker.width - marker.headPadding * 2)
  const headHeight = Math.max(0, marker.height - marker.headPadding * 2)
  const centerX = marker.width / 2
  const centerY = marker.height / 2
  const cornerRadius = Math.min(marker.width, marker.height) * 0.22
  const headCornerRadius = Math.max(0, cornerRadius - marker.headPadding)

  return {
    markerPath: makeRoundedRectPath(0, 0, marker.width, marker.height, cornerRadius),
    headPath: makeRoundedRectPath(
      marker.headPadding,
      marker.headPadding,
      headWidth,
      headHeight,
      headCornerRadius,
    ),
    iconBounds: makeCenteredIconBounds(
      centerX,
      centerY,
      Math.min(headWidth, headHeight),
      marker.headIconRatio,
    ),
  } satisfies MarkerGeometry
}

const makeMarkerGeometry = (marker: MarkerOptions): MarkerGeometry => {
  switch (marker.shape) {
    case MarkerShape.Pin:
      return makePinGeometry(marker)
    case MarkerShape.Box:
      return makeBoxGeometry(marker)
  }
}

export const makeMarkerIcon = (iconSvg: string | undefined, marker: MarkerOptions) => {
  const { innerHtml, presentationAttributes, viewBox } = parseMarkerIconSvg(iconSvg)
  const { markerPath, headPath, iconBounds } = makeMarkerGeometry(marker)

  return `
<svg xmlns="${SVG_NAMESPACE}" width="${marker.width}" height="${marker.height}" viewBox="-1 -1 ${marker.width + 2} ${marker.height + 2}">
  <path d="${escapeSvgAttribute(markerPath)}" fill="${escapeSvgAttribute(marker.color)}"/>
  <path d="${escapeSvgAttribute(headPath)}" fill="${escapeSvgAttribute(marker.backgroundColor)}"/>
  <svg x="${iconBounds.x}" y="${iconBounds.y}" width="${iconBounds.size}" height="${iconBounds.size}" viewBox="${escapeSvgAttribute(viewBox)}" color="${escapeSvgAttribute(marker.iconColor)}" ${presentationAttributes}>
    ${innerHtml}
  </svg>
</svg>`.trim()
}
