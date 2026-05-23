import type { ExpressionSpecification, LayerSpecification, Map as MapLibreMap } from 'maplibre-gl'
import {
  Armchair,
  AudioLines,
  Baby,
  BadgeDollarSign,
  Banknote,
  BatteryCharging,
  Bed,
  Beer,
  Bike,
  Binoculars,
  Bird,
  BookOpen,
  BriefcaseBusiness,
  Bug,
  Building2,
  Bus,
  Camera,
  Candy,
  Car,
  CarTaxiFront,
  Caravan,
  Castle,
  Church,
  Cigarette,
  Circle,
  CircleSlash,
  Code2,
  Coffee,
  Compass,
  Cross,
  DoorOpen,
  Droplets,
  Dumbbell,
  Ear,
  Factory,
  Film,
  Flag,
  Flame,
  FlameKindling,
  Flower2,
  Footprints,
  Fuel,
  Gamepad2,
  Gem,
  Gift,
  Glasses,
  GraduationCap,
  Hamburger,
  Hammer,
  HardHat,
  Heart,
  HeartHandshake,
  HeartPulse,
  Hospital,
  Hotel,
  House,
  IceCreamBowl,
  Images,
  Info,
  KeyRound,
  Lamp,
  Landmark,
  Languages,
  Lock,
  Mail,
  Mailbox,
  Map,
  MapPin,
  Megaphone,
  Microscope,
  Monitor,
  Mountain,
  MountainSnow,
  Music,
  Newspaper,
  NotebookPen,
  Package,
  Paintbrush,
  Palette,
  PawPrint,
  Phone,
  Pill,
  Plane,
  Plug,
  Printer,
  Puzzle,
  RadioTower,
  Recycle,
  RefreshCw,
  RollerCoaster,
  Ruler,
  Scale,
  School,
  Scissors,
  Search,
  Shield,
  ShieldCheck,
  Ship,
  Shirt,
  ShoppingBag,
  ShoppingBasket,
  ShoppingCart,
  Smartphone,
  Snowflake,
  Sparkles,
  SquareParking,
  Star,
  Stethoscope,
  Store,
  Target,
  Tent,
  Theater,
  Ticket,
  Toilet,
  TrainFront,
  TramFront,
  Trash2,
  TreePine,
  Trees,
  Trophy,
  Truck,
  Users,
  Utensils,
  Warehouse,
  WashingMachine,
  Watch,
  Waves,
  Wheat,
  Wine,
  Wrench,
  Zap,
} from 'lucide-static'
import { OMT_POI_SUBCLASS_METADATA } from '@/constants/omt-mapping.ts'
import { onScopeDisposeLifo } from '@/composables/helper.ts'
import { useLayer } from '@/composables/maplibre'
import { svgToImage } from '@/utils/svg-to-image.ts'

type SymbolLayerSpecification = Extract<LayerSpecification, { type: 'symbol' }>
type SymbolLayerLayout = NonNullable<SymbolLayerSpecification['layout']>
type SymbolLayerPaint = NonNullable<SymbolLayerSpecification['paint']>

type PoiMarkerOptions = {
  width?: number
  height?: number
  color?: string
  iconColor?: string
  headColor?: string
  outlineColor?: string
  shadowColor?: string
  scale?: number
  fontScale?: number
  iconScale?: number
  pixelRatio?: number
}

type UsePoiLayerMode = 'replace' | 'overlay'

export type UsePoiLayerOptions = {
  sourceLayer?: string
  layerIds?: string[]
  layerFilter?: (layer: SymbolLayerSpecification) => boolean
  mode?: UsePoiLayerMode
  beforeLayerId?: string
  marker?: PoiMarkerOptions
}

const DEFAULT_SOURCE_LAYER = 'poi'
const DEFAULT_POI_ICON = 'lucide:map-pin'
const POI_MARKER_LAYER_SUFFIX = '-poi-marker'
const POI_MARKER_IMAGE_VERSION = 'v3'

const DEFAULT_MARKER_OPTIONS: Required<PoiMarkerOptions> = {
  width: 32,
  height: 36,
  color: '#2563eb',
  iconColor: '#111827',
  headColor: '#ffffff',
  outlineColor: 'rgb(15 23 42 / 0.22)',
  shadowColor: 'rgb(15 23 42 / 0.26)',
  scale: 1.25,
  fontScale: 1.25,
  iconScale: 1.0,
  pixelRatio: 2,
}

const LUCIDE_ICON_SVGS: Record<string, string> = {
  armchair: Armchair,
  'audio-lines': AudioLines,
  baby: Baby,
  'badge-dollar-sign': BadgeDollarSign,
  banknote: Banknote,
  'battery-charging': BatteryCharging,
  bed: Bed,
  beer: Beer,
  bike: Bike,
  binoculars: Binoculars,
  bird: Bird,
  'book-open': BookOpen,
  'briefcase-business': BriefcaseBusiness,
  bug: Bug,
  'building-2': Building2,
  bus: Bus,
  camera: Camera,
  candy: Candy,
  car: Car,
  'car-taxi-front': CarTaxiFront,
  caravan: Caravan,
  castle: Castle,
  church: Church,
  cigarette: Cigarette,
  circle: Circle,
  'circle-slash': CircleSlash,
  'code-2': Code2,
  coffee: Coffee,
  compass: Compass,
  cross: Cross,
  'door-open': DoorOpen,
  droplets: Droplets,
  dumbbell: Dumbbell,
  ear: Ear,
  factory: Factory,
  film: Film,
  flag: Flag,
  flame: Flame,
  'flame-kindling': FlameKindling,
  'flower-2': Flower2,
  footprints: Footprints,
  fuel: Fuel,
  'gamepad-2': Gamepad2,
  gem: Gem,
  gift: Gift,
  glasses: Glasses,
  'graduation-cap': GraduationCap,
  hamburger: Hamburger,
  hammer: Hammer,
  'hard-hat': HardHat,
  heart: Heart,
  'heart-handshake': HeartHandshake,
  'heart-pulse': HeartPulse,
  hospital: Hospital,
  hotel: Hotel,
  house: House,
  'ice-cream-bowl': IceCreamBowl,
  images: Images,
  info: Info,
  'key-round': KeyRound,
  lamp: Lamp,
  landmark: Landmark,
  languages: Languages,
  lock: Lock,
  mail: Mail,
  mailbox: Mailbox,
  map: Map,
  'map-pin': MapPin,
  megaphone: Megaphone,
  microscope: Microscope,
  monitor: Monitor,
  mountain: Mountain,
  'mountain-snow': MountainSnow,
  music: Music,
  newspaper: Newspaper,
  'notebook-pen': NotebookPen,
  package: Package,
  paintbrush: Paintbrush,
  palette: Palette,
  'paw-print': PawPrint,
  phone: Phone,
  pill: Pill,
  plane: Plane,
  plug: Plug,
  printer: Printer,
  puzzle: Puzzle,
  'radio-tower': RadioTower,
  recycle: Recycle,
  'refresh-cw': RefreshCw,
  'roller-coaster': RollerCoaster,
  ruler: Ruler,
  scale: Scale,
  school: School,
  scissors: Scissors,
  search: Search,
  shield: Shield,
  'shield-check': ShieldCheck,
  ship: Ship,
  shirt: Shirt,
  'shopping-bag': ShoppingBag,
  'shopping-basket': ShoppingBasket,
  'shopping-cart': ShoppingCart,
  smartphone: Smartphone,
  snowflake: Snowflake,
  sparkles: Sparkles,
  'square-parking': SquareParking,
  star: Star,
  stethoscope: Stethoscope,
  store: Store,
  target: Target,
  tent: Tent,
  theater: Theater,
  ticket: Ticket,
  toilet: Toilet,
  'train-front': TrainFront,
  'tram-front': TramFront,
  'trash-2': Trash2,
  'tree-pine': TreePine,
  trees: Trees,
  trophy: Trophy,
  truck: Truck,
  users: Users,
  utensils: Utensils,
  warehouse: Warehouse,
  'washing-machine': WashingMachine,
  watch: Watch,
  waves: Waves,
  wheat: Wheat,
  wine: Wine,
  wrench: Wrench,
  zap: Zap,
}

const escapeAttribute = (value: string) =>
  value
    .replaceAll('&', '&amp;')
    .replaceAll('"', '&quot;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')

const SVG_PRESENTATION_ATTRIBUTES = [
  'clip-rule',
  'fill',
  'fill-rule',
  'stroke',
  'stroke-linecap',
  'stroke-linejoin',
  'stroke-miterlimit',
  'stroke-width',
] as const

const sanitizeImageIdPart = (value: string) => value.replace(/[^a-zA-Z0-9_-]/g, '-')

const getMarkerStyleKey = (marker: Required<PoiMarkerOptions>) =>
  [
    marker.width,
    marker.height,
    marker.color,
    marker.iconColor,
    marker.headColor,
    marker.outlineColor,
    marker.shadowColor,
    marker.iconScale,
    marker.pixelRatio,
  ]
    .map((value) => sanitizeImageIdPart(String(value)))
    .join('-')

const getPoiMarkerImageId = (icon: string, marker: Required<PoiMarkerOptions>) =>
  `jlh-poi-marker-${POI_MARKER_IMAGE_VERSION}-${getMarkerStyleKey(marker)}-${sanitizeImageIdPart(icon)}`

const getFunctionArguments = (value: string, functionName: string) => {
  const match = value.match(new RegExp(`^${functionName}a?\\((.*)\\)$`, 'i'))

  return match?.[1]
}

const getColorNumber = (value: string | undefined) => {
  if (!value) return undefined

  const normalized = value.trim()

  return normalized.endsWith('%')
    ? Number.parseFloat(normalized.slice(0, -1))
    : Number.parseFloat(normalized)
}

const isZeroColorComponent = (value: string | undefined) => getColorNumber(value) === 0

const isBlackRgbColor = (value: string) => {
  const args = getFunctionArguments(value, 'rgb')
  if (!args) return false

  const [colorChannels] = args.split('/')
  const channels = colorChannels
    ?.trim()
    .split(/[,\s]+/)
    .filter(Boolean)

  return channels?.length === 3 && channels.every((channel) => isZeroColorComponent(channel))
}

const isBlackHslColor = (value: string) => {
  const args = getFunctionArguments(value, 'hsl')
  if (!args) return false

  const [colorChannels] = args.split('/')
  const channels = colorChannels
    ?.trim()
    .split(/[,\s]+/)
    .filter(Boolean)

  return channels !== undefined && isZeroColorComponent(channels[2])
}

const isBlackHexColor = (value: string) => {
  const color = value.slice(1)

  return /^0{3,4}$/i.test(color) || /^0{6}([0-9a-f]{2})?$/i.test(color)
}

const isBlackColor = (value: string) => {
  const normalized = value.trim().toLowerCase()

  return (
    normalized === 'black' ||
    (normalized.startsWith('#') && isBlackHexColor(normalized)) ||
    isBlackRgbColor(normalized) ||
    isBlackHslColor(normalized)
  )
}

const getConstantColor = (value: unknown) =>
  typeof value === 'string' && value.trim().length > 0 && !isBlackColor(value) ? value : undefined

const getOriginalLayerIconColor = (baseLayer: SymbolLayerSpecification) => {
  const paint = (baseLayer.paint ?? {}) as SymbolLayerPaint

  return getConstantColor(paint['icon-color']) ?? getConstantColor(paint['text-color'])
}

const makeLayerMarkerOptions = (
  baseLayer: SymbolLayerSpecification,
  marker: Required<PoiMarkerOptions>,
  overrideMarkerColor: string | undefined,
): Required<PoiMarkerOptions> => {
  const color =
    getConstantColor(overrideMarkerColor) ??
    getOriginalLayerIconColor(baseLayer) ??
    getConstantColor(marker.color) ??
    DEFAULT_MARKER_OPTIONS.color

  return {
    ...marker,
    color,
    iconColor: color,
  }
}

const getLucideIconName = (icon: string) =>
  icon.startsWith('lucide:') ? icon.slice('lucide:'.length) : undefined

const getLucideIconSvg = (icon: string) => {
  const iconName = getLucideIconName(icon)
  if (!iconName) return undefined

  return LUCIDE_ICON_SVGS[iconName]
}

const parseSvgContent = (source: string | undefined) => {
  if (!source) {
    return {
      innerHtml: '',
      presentationAttributes:
        'fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"',
      viewBox: '0 0 24 24',
    }
  }

  const svg = new DOMParser().parseFromString(source, 'image/svg+xml').documentElement

  if (svg.tagName.toLowerCase() !== 'svg') {
    return {
      innerHtml: '',
      presentationAttributes:
        'fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"',
      viewBox: '0 0 24 24',
    }
  }

  return {
    innerHtml: svg.innerHTML,
    presentationAttributes: SVG_PRESENTATION_ATTRIBUTES.flatMap((name) => {
      const value = svg.getAttribute(name)
      if (value === null) return []

      return [`${name}="${escapeAttribute(value)}"`]
    }).join(' '),
    viewBox: svg.getAttribute('viewBox') ?? '0 0 24 24',
  }
}

const formatSvgNumber = (value: number) => Number(value.toFixed(3)).toString()

const getMarkerIconBounds = (marker: Required<PoiMarkerOptions>) => {
  const iconSize = 14 * marker.iconScale
  const iconPosition = 16 - iconSize / 2

  return {
    size: formatSvgNumber(iconSize),
    x: formatSvgNumber(iconPosition),
    y: formatSvgNumber(iconPosition),
  }
}

const buildPoiMarkerSvg = (iconSvg: string | undefined, marker: Required<PoiMarkerOptions>) => {
  const { innerHtml, presentationAttributes, viewBox } = parseSvgContent(iconSvg)
  const iconBounds = getMarkerIconBounds(marker)
  const markerPath =
    'M16 34C15.25 34 14.55 33.68 13.95 33.05C11.3 30.3 4 22.65 4 15.8C4 8.85 9.37 3.5 16 3.5C22.63 3.5 28 8.85 28 15.8C28 22.65 20.7 30.3 18.05 33.05C17.45 33.68 16.75 34 16 34Z'

  return `
<svg xmlns="http://www.w3.org/2000/svg" width="${marker.width}" height="${marker.height}" viewBox="0 0 32 36">
  <ellipse cx="16" cy="34.5" rx="7" ry="1.5" fill="${escapeAttribute(marker.shadowColor)}"/>
  <path d="${markerPath}" fill="${escapeAttribute(marker.color)}"/>
  <path d="${markerPath}" fill="none" stroke="${escapeAttribute(marker.outlineColor)}" stroke-width="1"/>
  <circle cx="16" cy="16" r="9" fill="${escapeAttribute(marker.headColor)}"/>
  <svg x="${iconBounds.x}" y="${iconBounds.y}" width="${iconBounds.size}" height="${iconBounds.size}" viewBox="${escapeAttribute(viewBox)}" color="${escapeAttribute(marker.iconColor)}" ${presentationAttributes}>
    ${innerHtml}
  </svg>
</svg>`.trim()
}

const loadPoiMarkerImage = async (icon: string, marker: Required<PoiMarkerOptions>) => {
  const iconSvg = getLucideIconSvg(icon) ?? getLucideIconSvg(DEFAULT_POI_ICON)
  const markerSvg = buildPoiMarkerSvg(iconSvg, marker)

  return svgToImage(markerSvg, {
    width: marker.width,
    height: marker.height,
    pixelRatio: marker.pixelRatio,
    color: marker.color,
  })
}

const getPoiIconIds = () => [
  ...new Set([
    DEFAULT_POI_ICON,
    ...Object.values(OMT_POI_SUBCLASS_METADATA).map((item) => item.icon),
  ]),
]

const makePropertyIconMatchExpression = (
  property: 'class' | 'subclass',
  marker: Required<PoiMarkerOptions>,
  fallback: string | ExpressionSpecification,
): ExpressionSpecification =>
  [
    'match',
    ['to-string', ['get', property]],
    ...Object.entries(OMT_POI_SUBCLASS_METADATA).flatMap(([subclass, metadata]) => [
      subclass,
      getPoiMarkerImageId(metadata.icon, marker),
    ]),
    fallback,
  ] as ExpressionSpecification

const makePoiIconImageExpression = (marker: Required<PoiMarkerOptions>): ExpressionSpecification =>
  makePropertyIconMatchExpression(
    'subclass',
    marker,
    makePropertyIconMatchExpression('class', marker, getPoiMarkerImageId(DEFAULT_POI_ICON, marker)),
  )

type LegacyTextSizeSpecification = {
  stops: [number, number][]
  [key: string]: unknown
}

const isLegacyTextSizeSpecification = (value: unknown): value is LegacyTextSizeSpecification => {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return false

  const stops = (value as { stops?: unknown }).stops

  return (
    Array.isArray(stops) &&
    stops.every(
      (stop): stop is [number, number] =>
        Array.isArray(stop) &&
        stop.length === 2 &&
        typeof stop[0] === 'number' &&
        typeof stop[1] === 'number',
    )
  )
}

const scaleTextSize = (value: unknown, scale: number): SymbolLayerLayout['text-size'] => {
  if (typeof value === 'number') return value * scale
  if (Array.isArray(value)) return ['*', value, scale] as unknown as ExpressionSpecification
  if (isLegacyTextSizeSpecification(value)) {
    return {
      ...value,
      stops: value.stops.map(([zoom, size]) => [zoom, size * scale]),
    } as unknown as SymbolLayerLayout['text-size']
  }

  return 16 * scale
}

const isPoiSymbolLayer = (
  layer: LayerSpecification,
  sourceLayer: string,
): layer is SymbolLayerSpecification =>
  layer.type === 'symbol' && layer['source-layer'] === sourceLayer

const pickPoiSymbolLayers = (
  map: MapLibreMap,
  { sourceLayer = DEFAULT_SOURCE_LAYER, layerIds, layerFilter }: UsePoiLayerOptions,
) => {
  const layerIdSet = layerIds ? new Set(layerIds) : undefined

  return (map.getStyle().layers ?? []).filter(
    (layer): layer is SymbolLayerSpecification =>
      isPoiSymbolLayer(layer, sourceLayer) &&
      (layerIdSet?.has(layer.id) ?? true) &&
      (layerFilter?.(layer) ?? true),
  )
}

const makePoiMarkerLayer = (
  baseLayer: SymbolLayerSpecification,
  marker: Required<PoiMarkerOptions>,
): SymbolLayerSpecification => {
  const layout = (baseLayer.layout ?? {}) as SymbolLayerLayout
  const paint = (baseLayer.paint ?? {}) as SymbolLayerPaint

  return {
    ...baseLayer,
    id: `${baseLayer.id}${POI_MARKER_LAYER_SUFFIX}`,
    layout: {
      ...layout,
      'icon-image': makePoiIconImageExpression(marker),
      'icon-size': marker.scale,
      'icon-anchor': 'bottom',
      'icon-offset': [0, 0],
      'icon-allow-overlap': layout['icon-allow-overlap'] ?? false,
      'icon-ignore-placement': layout['icon-ignore-placement'] ?? false,
      'text-field': layout['text-field'],
      'text-anchor': 'top',
      'text-offset': [0, 0.55],
      'text-size': scaleTextSize(layout['text-size'], marker.fontScale),
      'text-optional': false,
      'icon-optional': false,
      'symbol-sort-key': layout['symbol-sort-key'] ?? ['to-number', ['get', 'rank']],
    },
    paint: {
      ...paint,
      'text-color': paint['text-color'] ?? '#1f2937',
      'text-halo-color': paint['text-halo-color'] ?? '#ffffff',
      'text-halo-width': paint['text-halo-width'] ?? 1.5,
    },
  }
}

const registerPoiMarkerImages = (map: MapLibreMap, marker: Required<PoiMarkerOptions>) => {
  const addedImageIds = new Set<string>()
  let disposed = false

  onScopeDisposeLifo(() => {
    disposed = true

    addedImageIds.forEach((imageId) => {
      if (map.hasImage(imageId)) {
        map.removeImage(imageId)
      }
    })
  })

  getPoiIconIds().forEach((icon) => {
    const imageId = getPoiMarkerImageId(icon, marker)

    if (map.hasImage(imageId)) return

    loadPoiMarkerImage(icon, marker).then(({ image }) => {
      if (disposed || map.hasImage(imageId)) return

      map.addImage(imageId, image, {
        pixelRatio: marker.pixelRatio,
      })
      addedImageIds.add(imageId)
    }, console.error)
  })
}

export function usePoiLayer(
  map: MapLibreMap,
  optionsOrBeforeLayerId: UsePoiLayerOptions | string = {},
) {
  const options =
    typeof optionsOrBeforeLayerId === 'string'
      ? { beforeLayerId: optionsOrBeforeLayerId }
      : optionsOrBeforeLayerId
  const marker = {
    ...DEFAULT_MARKER_OPTIONS,
    ...options.marker,
  }
  const mode = options.mode ?? 'replace'

  const baseLayers = pickPoiSymbolLayers(map, options)
  const addedLayerIds = baseLayers.map((layer) => `${layer.id}${POI_MARKER_LAYER_SUFFIX}`)
  const registeredMarkerKeys = new Set<string>()

  baseLayers.forEach((baseLayer) => {
    const previousVisibility = map.getLayoutProperty(baseLayer.id, 'visibility')
    const layerMarker = makeLayerMarkerOptions(baseLayer, marker, options.marker?.color)
    const layerMarkerKey = getMarkerStyleKey(layerMarker)

    if (!registeredMarkerKeys.has(layerMarkerKey)) {
      registeredMarkerKeys.add(layerMarkerKey)
      registerPoiMarkerImages(map, layerMarker)
    }

    useLayer(map, makePoiMarkerLayer(baseLayer, layerMarker), {
      beforeId: options.beforeLayerId ?? baseLayer.id,
    })

    if (mode === 'replace') {
      map.setLayoutProperty(baseLayer.id, 'visibility', 'none')

      onScopeDisposeLifo(() => {
        if (map.getLayer(baseLayer.id)) {
          map.setLayoutProperty(baseLayer.id, 'visibility', previousVisibility ?? 'visible')
        }
      })
    }
  })

  return {
    layerIds: addedLayerIds,
    baseLayerIds: baseLayers.map((layer) => layer.id),
  }
}
