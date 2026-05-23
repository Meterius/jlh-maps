import type { ExpressionSpecification } from 'maplibre-gl'

type LegacyStopsSpecification = {
  stops: [number, number][]
  [key: string]: unknown
}

const isLegacyStopsSpecification = (value: unknown): value is LegacyStopsSpecification => {
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

export const makeStringPropertyMatchExpression = (
  property: string,
  entries: Iterable<[string, string]>,
  fallback: string | ExpressionSpecification,
): ExpressionSpecification =>
  [
    'match',
    ['to-string', ['get', property]],
    ...Array.from(entries).flatMap(([value, result]) => [value, result]),
    fallback,
  ] as ExpressionSpecification

export const scaleStyleNumber = (value: unknown, scale: number, fallback: number) => {
  if (typeof value === 'number') return value * scale
  if (Array.isArray(value)) return ['*', value, scale] as unknown as ExpressionSpecification
  if (isLegacyStopsSpecification(value)) {
    return {
      ...value,
      stops: value.stops.map(([zoom, size]) => [zoom, size * scale]),
    }
  }

  return fallback * scale
}
