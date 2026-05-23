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

const getColorFunctionChannels = (args: string) =>
  args
    .split('/')[0]
    ?.trim()
    .split(/[,\s]+/)
    .filter(Boolean)

const isBlackRgbColor = (value: string) => {
  const args = getFunctionArguments(value, 'rgb')
  if (!args) return false

  const channels = getColorFunctionChannels(args)

  return (
    channels !== undefined &&
    channels.length >= 3 &&
    channels.slice(0, 3).every(isZeroColorComponent)
  )
}

const isBlackHslColor = (value: string) => {
  const args = getFunctionArguments(value, 'hsl')
  if (!args) return false

  const channels = getColorFunctionChannels(args)

  return channels !== undefined && isZeroColorComponent(channels[2])
}

const isBlackHexColor = (value: string) => {
  const color = value.slice(1)

  return /^0{3,4}$/i.test(color) || /^0{6}([0-9a-f]{2})?$/i.test(color)
}

export const isBlackCssColor = (value: string) => {
  const normalized = value.trim().toLowerCase()

  return (
    normalized === 'black' ||
    (normalized.startsWith('#') && isBlackHexColor(normalized)) ||
    isBlackRgbColor(normalized) ||
    isBlackHslColor(normalized)
  )
}

export const getUsableCssColor = (value: unknown) =>
  typeof value === 'string' && value.trim().length > 0 && !isBlackCssColor(value)
    ? value
    : undefined
