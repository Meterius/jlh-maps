import lucideSpriteSvg from 'lucide-static/sprite.svg?raw'
import { escapeSvgAttribute, SVG_NAMESPACE } from '@/utils/svg.ts'

const LUCIDE_ICON_PREFIX = 'lucide:'

let lucideSpriteDocument: Document | undefined

const getLucideIconFileName = (iconName: string) =>
  iconName.startsWith(LUCIDE_ICON_PREFIX) ? iconName.slice(LUCIDE_ICON_PREFIX.length) : iconName

const getLucideSpriteDocument = () => {
  if (lucideSpriteDocument || typeof DOMParser === 'undefined') return lucideSpriteDocument

  lucideSpriteDocument = new DOMParser().parseFromString(lucideSpriteSvg, 'image/svg+xml')
  return lucideSpriteDocument
}

export const loadLucideIconSvg = (iconName: string) => {
  const symbol = getLucideSpriteDocument()?.getElementById(getLucideIconFileName(iconName))
  if (!symbol) return undefined

  return `
<svg xmlns="${SVG_NAMESPACE}" width="24" height="24" viewBox="${escapeSvgAttribute(symbol.getAttribute('viewBox') ?? '0 0 24 24')}" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
  ${symbol.innerHTML}
</svg>`.trim()
}

export const makeLucideIconSvgLoader = (iconName: string) => () => loadLucideIconSvg(iconName)
