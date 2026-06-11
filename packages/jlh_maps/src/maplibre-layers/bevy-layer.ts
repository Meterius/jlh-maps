import type { CustomLayerInterface, Map as MapLibreMap } from 'maplibre-gl'
import type { ShallowRef } from 'vue'
import { BEVY_MAPLIBRE_TEXTURE_ATLAS_VIEWPORT, MaplibreTextureAtlasKind } from '@/bevy'
import { assertNever } from '@/utils/helper.ts'

interface BevyLayerOptions {
  id?: string
}

const VERTEX_SHADER = `#version 300 es
in vec2 a_pos;
out vec2 v_uv;

void main() {
  v_uv = vec2(a_pos.x * 0.5 + 0.5, 0.5 - a_pos.y * 0.5);
  gl_Position = vec4(a_pos, 0.0, 1.0);
}
`

const FRAGMENT_SHADER = `#version 300 es
precision highp float;

in vec2 v_uv;
uniform sampler2D u_color_texture;
uniform vec4 u_texture_region;
uniform vec2 u_depth_range;
uniform bool u_terrain_composite;
out vec4 out_color;

void main() {
  vec2 atlas_uv = u_texture_region.xy + v_uv * u_texture_region.zw;
  vec4 color = texture(u_color_texture, atlas_uv);

  if (u_terrain_composite) {
    // The terrain region is a straight-alpha coverage texture. Convert it into
    // an opaque multiplier: uncovered pixels become white, covered pixels keep
    // Bevy's lit terrain color.
    float coverage = clamp(color.a, 0.0, 1.0);
    color = vec4(mix(vec3(1.0), color.rgb, coverage), 1.0);
  }

  gl_FragDepth = u_depth_range.x;
  out_color = color;
}
`

interface FrameTexture {
  handle?: WebGLTexture
  width: number
  height: number
}

export class BevyLayer implements CustomLayerInterface {
  id: string
  type = 'custom' as const
  renderingMode: '2d' | '3d' = '3d'
  compositeSeperator = true

  private map!: MapLibreMap
  private program: WebGLProgram | undefined

  private texture: FrameTexture = {
    handle: undefined,
    width: 0,
    height: 0,
  }

  private vertexBuffer: WebGLBuffer | undefined
  private vertexArray: WebGLVertexArrayObject | undefined
  private aPos = -1
  private uColorTexture: WebGLUniformLocation | null = null
  private uTextureRegion: WebGLUniformLocation | null = null
  private uDepthRange: WebGLUniformLocation | null = null
  private uTerrainComposite: WebGLUniformLocation | null = null

  constructor(
    private readonly frameBitmap: ShallowRef<ImageBitmap | null>,
    options: BevyLayerOptions = {},
  ) {
    this.id = options.id ?? 'bevy-texture'
  }

  onAdd(map: MapLibreMap, gl: WebGLRenderingContext | WebGL2RenderingContext): void {
    this.map = map

    this.program = createProgram(gl, VERTEX_SHADER, FRAGMENT_SHADER)
    this.aPos = gl.getAttribLocation(this.program, 'a_pos')
    this.uColorTexture = gl.getUniformLocation(this.program, 'u_color_texture')
    this.uTextureRegion = gl.getUniformLocation(this.program, 'u_texture_region')
    this.uDepthRange = gl.getUniformLocation(this.program, 'u_depth_range')
    this.uTerrainComposite = gl.getUniformLocation(this.program, 'u_terrain_composite')

    this.vertexBuffer = gl.createBuffer()!
    gl.bindBuffer(gl.ARRAY_BUFFER, this.vertexBuffer)
    gl.bufferData(
      gl.ARRAY_BUFFER,
      new Float32Array([-1, -1, 1, -1, -1, 1, -1, 1, 1, -1, 1, 1]),
      gl.STATIC_DRAW,
    )

    if (isWebGL2(gl)) {
      this.vertexArray = gl.createVertexArray() ?? undefined
      gl.bindVertexArray(this.vertexArray ?? null)
      gl.enableVertexAttribArray(this.aPos)
      gl.vertexAttribPointer(this.aPos, 2, gl.FLOAT, false, 0, 0)
      gl.bindVertexArray(null)
    }

    gl.bindBuffer(gl.ARRAY_BUFFER, null)
  }

  render(): void {}

  renderComposite(gl: WebGL2RenderingContext | WebGLRenderingContext): void {
    this.map.triggerRepaint()

    const frameBitmap = this.frameBitmap.value

    this.frameBitmap.value = null

    if (!frameBitmap || !this.program || !this.vertexBuffer || !this.texture) {
      frameBitmap?.close()
      return
    }

    if (!bindAndUploadTexture(gl, this.texture, frameBitmap)) {
      this.map.triggerRepaint()
      return
    }

    for (const kind of [MaplibreTextureAtlasKind.Terrain, MaplibreTextureAtlasKind.Overlay]) {
      gl.enable(gl.BLEND)

      switch (kind) {
        case MaplibreTextureAtlasKind.Terrain: {
          // Terrain is applied as a color multiplier over MapLibre terrain:
          // final color = Bevy terrain multiplier * existing framebuffer color.
          gl.blendFunc(gl.DST_COLOR, gl.ZERO)
          break
        }
        case MaplibreTextureAtlasKind.Overlay: {
          // The worker requests straight-alpha ImageBitmaps and upload keeps
          // them straight, so use ordinary source-alpha composition here.
          gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA)
          break
        }
        default:
          assertNever(kind)
          break
      }

      gl.enable(gl.DEPTH_TEST)
      gl.depthFunc(gl.LEQUAL)
      gl.depthMask(true)

      gl.useProgram(this.program)

      if (isWebGL2(gl) && this.vertexArray) {
        gl.bindVertexArray(this.vertexArray)
      } else {
        gl.bindBuffer(gl.ARRAY_BUFFER, this.vertexBuffer)
        gl.enableVertexAttribArray(this.aPos)
        gl.vertexAttribPointer(this.aPos, 2, gl.FLOAT, false, 0, 0)
      }

      gl.uniform1i(this.uColorTexture, 0)
      gl.uniform4fv(this.uTextureRegion, BEVY_MAPLIBRE_TEXTURE_ATLAS_VIEWPORT[kind])
      // Only the terrain atlas region needs conversion into a multiplier; the
      // overlay region keeps its sampled RGBA color unchanged.
      gl.uniform1i(this.uTerrainComposite, kind === MaplibreTextureAtlasKind.Terrain ? 1 : 0)
      gl.uniform2f(
        this.uDepthRange,
        this.map.painter.depthRangeFor3D[0],
        this.map.painter.depthRangeFor3D[1],
      )
      gl.drawArrays(gl.TRIANGLES, 0, 6)
    }
  }

  onRemove(_map: MapLibreMap, gl: WebGLRenderingContext | WebGL2RenderingContext): void {
    if (this.vertexArray && isWebGL2(gl)) {
      gl.deleteVertexArray(this.vertexArray)
    }
    if (this.vertexBuffer) {
      gl.deleteBuffer(this.vertexBuffer)
    }
    if (this.texture.handle) {
      gl.deleteTexture(this.texture.handle)
    }
    if (this.program) {
      gl.deleteProgram(this.program)
    }
  }
}

function isWebGL2(
  gl: WebGLRenderingContext | WebGL2RenderingContext,
): gl is WebGL2RenderingContext {
  return 'createVertexArray' in gl
}

function createTexture(
  gl: WebGLRenderingContext | WebGL2RenderingContext,
): WebGLTexture | undefined {
  const texture = gl.createTexture() ?? undefined
  gl.bindTexture(gl.TEXTURE_2D, texture ?? null)
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST)
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST)
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE)
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE)
  gl.bindTexture(gl.TEXTURE_2D, null)
  return texture
}

function bindAndUploadTexture(
  gl: WebGLRenderingContext | WebGL2RenderingContext,
  frameTexture: FrameTexture,
  frameBitmap: ImageBitmap,
): boolean {
  gl.activeTexture(gl.TEXTURE0)

  gl.bindTexture(gl.TEXTURE_2D, frameTexture.handle ?? null)

  gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false)
  gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, true)
  gl.pixelStorei(gl.UNPACK_COLORSPACE_CONVERSION_WEBGL, gl.NONE)

  // Recreate texture if dimensions have changed
  if (
    !frameTexture.handle ||
    frameBitmap.width !== frameTexture.width ||
    frameBitmap.height !== frameTexture.height
  ) {
    frameTexture.width = frameBitmap.width
    frameTexture.height = frameBitmap.height

    if (isWebGL2(gl)) {
      if (frameTexture.handle) {
        gl.deleteTexture(frameTexture.handle)
      }

      frameTexture.handle = createTexture(gl)

      if (!frameTexture.handle) {
        frameBitmap.close()
        return false
      }

      gl.bindTexture(gl.TEXTURE_2D, frameTexture.handle)
      gl.texStorage2D(gl.TEXTURE_2D, 1, gl.RGBA8, frameTexture.width, frameTexture.height)
    } else {
      gl.texImage2D(
        gl.TEXTURE_2D,
        0,
        gl.RGBA,
        frameTexture.width,
        frameTexture.height,
        0,
        gl.RGBA,
        gl.UNSIGNED_BYTE,
        null,
      )
    }
  }

  // On Chrome this happens < 1ms, likely because the current setup is handled
  // as a GPU-to-GPU copy. On Firefox this can take ~20-40ms and incurs a CPU copy.
  // TODO: investigate Firefox performance bottleneck of texture transfer.
  try {
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, gl.RGBA, gl.UNSIGNED_BYTE, frameBitmap)
  } finally {
    frameBitmap.close()
  }

  return true
}

function createProgram(
  gl: WebGLRenderingContext | WebGL2RenderingContext,
  vertexShaderSource: string,
  fragmentShaderSource: string,
) {
  const vertexShader = createShader(gl, gl.VERTEX_SHADER, vertexShaderSource)
  const fragmentShader = createShader(gl, gl.FRAGMENT_SHADER, fragmentShaderSource)
  const program = gl.createProgram()
  if (!program) throw new Error('Failed to create Bevy layer program')

  gl.attachShader(program, vertexShader)
  gl.attachShader(program, fragmentShader)
  gl.linkProgram(program)
  gl.deleteShader(vertexShader)
  gl.deleteShader(fragmentShader)

  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    const info = gl.getProgramInfoLog(program) || 'Unknown Bevy layer program error'
    gl.deleteProgram(program)
    throw new Error(info)
  }

  return program
}

function createShader(
  gl: WebGLRenderingContext | WebGL2RenderingContext,
  type: number,
  source: string,
) {
  const shader = gl.createShader(type)
  if (!shader) throw new Error('Failed to create Bevy layer shader')

  gl.shaderSource(shader, source)
  gl.compileShader(shader)

  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    const info = gl.getShaderInfoLog(shader) || 'Unknown Bevy layer shader error'
    gl.deleteShader(shader)
    throw new Error(info)
  }

  return shader
}
