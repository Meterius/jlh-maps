import type { ExpressionSpecification, Map as MapLibreMap } from 'maplibre-gl'
import { createKeyedSharedComposable } from '@/composables/helper.ts'
import {
  getMapHashKey,
  type MapLibreMapImageData,
  useOnDemandImageProvider,
  type UseImageOptions,
} from '@/composables/maplibre'
import { loadLucideIconSvg } from '@/utils/lucide-icon-svg.ts'
import {
  escapeSvgAttribute,
  getSvgPresentationAttributes,
  parseSvgElement,
  parseSvgViewBox,
  SVG_NAMESPACE,
} from '@/utils/svg.ts'
import { svgToImage } from '@/utils/svg-to-image.ts'

const LUCIDE_PROVIDER_IMAGE_ID_PREFIX = 'lucide-provider:'
const LUCIDE_ICON_SIZE = 24

type UseLucideIconImageSourceProviderParams = {
  map: MapLibreMap
  pixelRatio: number
}

type LucideProviderImageParams = {
  iconName: string
  color: string
}

const makeEmptyLucideIconImage = (pixelRatio: number): MapLibreMapImageData => {
  const width = Math.round(LUCIDE_ICON_SIZE * pixelRatio)
  const height = Math.round(LUCIDE_ICON_SIZE * pixelRatio)

  return {
    width,
    height,
    data: new Uint8ClampedArray(width * height * 4),
  }
}

const makeLucideProviderImageId = (iconName: string, color: string) =>
  `${LUCIDE_PROVIDER_IMAGE_ID_PREFIX}${iconName}:${color}`

const parseLucideProviderImageId = (imageId: string): LucideProviderImageParams | null => {
  if (!imageId.startsWith(LUCIDE_PROVIDER_IMAGE_ID_PREFIX)) return null

  const imageKey = imageId.slice(LUCIDE_PROVIDER_IMAGE_ID_PREFIX.length)
  const colorSeparatorIndex = imageKey.lastIndexOf(':')
  if (colorSeparatorIndex <= 0 || colorSeparatorIndex === imageKey.length - 1) return null

  return {
    iconName: imageKey.slice(0, colorSeparatorIndex),
    color: imageKey.slice(colorSeparatorIndex + 1),
  }
}

export const makeLucideIcon = (iconSvg: string | undefined, color: string) => {
  const svg = iconSvg ? parseSvgElement(iconSvg) : undefined
  const viewBox = svg ? parseSvgViewBox(svg) : '0 0 24 24'
  const presentationAttributes = svg
    ? getSvgPresentationAttributes(svg)
    : 'fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"'

  return `
<svg xmlns="${SVG_NAMESPACE}" width="${LUCIDE_ICON_SIZE}" height="${LUCIDE_ICON_SIZE}" viewBox="${escapeSvgAttribute(viewBox)}" color="${escapeSvgAttribute(color)}" ${presentationAttributes}>
  ${svg?.innerHTML ?? ''}
</svg>`.trim()
}

export const useLucideIconImageSourceProvider = createKeyedSharedComposable(
  ({ map, pixelRatio }: UseLucideIconImageSourceProviderParams) =>
    [getMapHashKey(map), pixelRatio].join(':'),
  ({ map, pixelRatio }: UseLucideIconImageSourceProviderParams) => {
    const imageProviderOptions: UseImageOptions = {
      options: {
        pixelRatio,
      },
      onImageAdded: (image) => {
        if (image instanceof ImageBitmap) image.close()
      },
    }

    useOnDemandImageProvider(map, {
      getParamsForImageId: parseLucideProviderImageId,
      getInitialImage: () => ({
        image: makeEmptyLucideIconImage(pixelRatio),
        options: imageProviderOptions.options,
      }),
      fetchImage: async ({ iconName, color }) => ({
        image: await svgToImage(makeLucideIcon(loadLucideIconSvg(iconName), color), {
          width: LUCIDE_ICON_SIZE,
          height: LUCIDE_ICON_SIZE,
          pixelRatio,
        }),
      }),
      onImageAdded: imageProviderOptions.onImageAdded,
    })

    return {
      makeImageIdFromIconNameExpression: (
        iconNameExpression: ExpressionSpecification,
        colorExpression: string,
      ): ExpressionSpecification =>
        [
          'concat',
          LUCIDE_PROVIDER_IMAGE_ID_PREFIX,
          ['to-string', iconNameExpression],
          ':',
          ['to-string', colorExpression],
        ] as ExpressionSpecification,
      makeImageId: makeLucideProviderImageId,
    }
  },
)

export type UseLucideIconImageSourceProviderReturn = ReturnType<
  typeof useLucideIconImageSourceProvider
>
