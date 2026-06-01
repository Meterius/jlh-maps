import { addProtocol, type AddProtocolAction, type GetResourceResponse, type RequestParameters } from 'maplibre-gl'
import { Protocol } from 'pmtiles'

const DEMO_ASSETS_PROTOCOL = 'demo-assets'
const DEMO_GLYPHS_PROTOCOL = 'demo-glyphs'
const DEMO_PMTILES_PROTOCOL = 'demo-pmtiles'
const DEMO_ROOT = 'demo'

const appBaseUrl = new URL(import.meta.env.BASE_URL, window.location.href)
const pmtilesProtocol = new Protocol({ metadata: true })

let protocolsRegistered = false

export function registerMaplibreProtocols() {
  if (protocolsRegistered) return

  protocolsRegistered = true
  addProtocol('pmtiles', pmtilesProtocol.tile)
  addProtocol(DEMO_ASSETS_PROTOCOL, loadDemoAsset)
  addProtocol(DEMO_GLYPHS_PROTOCOL, loadDemoGlyphs)
  addProtocol(DEMO_PMTILES_PROTOCOL, loadDemoPmtiles)
}

const loadDemoAsset: AddProtocolAction = async (requestParameters, abortController) =>
  fetchDemoResource(getDemoPath(requestParameters.url, DEMO_ASSETS_PROTOCOL), requestParameters, abortController)

const loadDemoGlyphs: AddProtocolAction = async (requestParameters, abortController) => {
  const pathSegments = getDemoPath(requestParameters.url, DEMO_GLYPHS_PROTOCOL)
    .split('/')
    .filter(Boolean)
    .map((segment) => decodeURIComponent(segment))
  const fontstackIndex = pathSegments.length - 2

  if (fontstackIndex < 0) {
    throw new Error(`Invalid ${DEMO_GLYPHS_PROTOCOL} URL: ${requestParameters.url}`)
  }

  const fontstack = pathSegments[fontstackIndex]

  if (fontstack === undefined) {
    throw new Error(`Invalid ${DEMO_GLYPHS_PROTOCOL} URL: ${requestParameters.url}`)
  }

  pathSegments[fontstackIndex] = selectFirstFont(fontstack)

  return fetchDemoResource(pathSegments.map(encodeURIComponent).join('/'), requestParameters, abortController)
}

const loadDemoPmtiles: AddProtocolAction = async (requestParameters, abortController) => {
  const logicalPath = getDemoPath(requestParameters.url, DEMO_PMTILES_PROTOCOL)
  const { archivePath, tilePath } = splitPmtilesPath(logicalPath)
  const pmtilesUrl = `pmtiles://${resolveDemoUrl(archivePath).toString()}${tilePath}`
  const response = (await pmtilesProtocol.tile(
    {
      ...requestParameters,
      url: pmtilesUrl,
    },
    abortController,
  )) as GetResourceResponse<unknown>

  if (requestParameters.type === 'json' && isRecord(response.data)) {
    return {
      ...response,
      data: {
        ...response.data,
        tiles: [`${DEMO_PMTILES_PROTOCOL}://${archivePath}/{z}/{x}/{y}`],
      },
    }
  }

  return response
}

function getDemoPath(url: string, protocol: string): string {
  const prefix = `${protocol}://`

  if (!url.startsWith(prefix)) {
    throw new Error(`Invalid ${protocol} URL: ${url}`)
  }

  return url.slice(prefix.length).replace(/^\/+/, '')
}

function splitPmtilesPath(logicalPath: string): { archivePath: string; tilePath: string } {
  const result = logicalPath.match(/^(.*\.pmtiles)(\/\d+\/\d+\/\d+)?(?:\.[a-z0-9]+)?$/i)

  if (!result?.[1]) {
    throw new Error(`Invalid ${DEMO_PMTILES_PROTOCOL} URL path: ${logicalPath}`)
  }

  return {
    archivePath: result[1],
    tilePath: result[2] ?? '',
  }
}

function selectFirstFont(fontstack: string): string {
  return fontstack.split(',')[0]?.trim() ?? fontstack
}

async function fetchDemoResource(
  logicalPath: string,
  requestParameters: RequestParameters,
  abortController: AbortController,
): Promise<GetResourceResponse<unknown>> {
  const response = await fetch(resolveDemoUrl(logicalPath), {
    method: requestParameters.method ?? 'GET',
    body: requestParameters.body,
    credentials: requestParameters.credentials,
    headers: requestParameters.headers,
    cache: requestParameters.cache,
    referrerPolicy: requestParameters.referrerPolicy,
    signal: abortController.signal,
  })

  if (!response.ok) {
    throw new Error(`Failed to load ${requestParameters.url}: ${response.status} ${response.statusText}`)
  }

  const expiry = {
    cacheControl: response.headers.get('Cache-Control'),
    expires: response.headers.get('Expires'),
  }

  if (requestParameters.type === 'json') {
    return {
      ...expiry,
      data: await response.json(),
    }
  }

  if (requestParameters.type === 'arrayBuffer' || requestParameters.type === 'image') {
    return {
      ...expiry,
      data: await response.arrayBuffer(),
    }
  }

  return {
    ...expiry,
    data: await response.text(),
  }
}

function resolveDemoUrl(logicalPath: string): URL {
  const encodedPath = logicalPath
    .split('/')
    .filter(Boolean)
    .map(encodeDemoPathSegment)
    .join('/')

  return new URL(`${DEMO_ROOT}/${encodedPath}`, appBaseUrl)
}

function encodeDemoPathSegment(segment: string): string {
  return encodeURIComponent(decodeURIComponent(segment)).replace(/%40/gi, '@')
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
