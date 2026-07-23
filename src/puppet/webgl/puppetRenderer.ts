/**
 * puppetRenderer.ts — WebGL2 renderer for a multi-PART puppet: it draws an
 * ordered list of independent textured meshes (layers) back-to-front onto a
 * transparent canvas. This supersedes the single-mesh spike renderer; layer
 * independence is what lets the eye + eyelid close without touching the face.
 *
 * Keeps the properties the migration plan insists on: transparent clear (no
 * black box), straight-alpha compositing, deterministic resource cleanup, and
 * WebGL context-loss recovery (all parts rebuilt on restore).
 *
 * Lifecycle:
 *   const r = createPuppetRenderer(canvas)
 *   r.setPuppet(parts)               // parts: { mesh, image }[] in draw order
 *   r.draw(perPartVertices)          // one deformed vertex array per part
 *   r.resize(w, h, dpr); r.destroy()
 */
import type { Vec2 } from '../deform'
import { flattenVec2, type Mesh } from '../mesh'

export interface RenderPart {
  mesh:  Mesh
  image: TexImageSource
}

export interface PuppetGLRenderer {
  /** Upload each part's static uv/index buffers and texture, in draw order. */
  setPuppet(parts: readonly RenderPart[]): void
  /** Draw one frame: `perPartVertices[i]` are part i's deformed positions. */
  draw(perPartVertices: readonly (readonly Vec2[])[]): void
  resize(cssWidth: number, cssHeight: number, dpr?: number): void
  destroy(): void
  readonly lost: boolean
}

interface PartGL {
  vao:        WebGLVertexArrayObject | null
  posBuf:     WebGLBuffer | null
  uvBuf:      WebGLBuffer | null
  idxBuf:     WebGLBuffer | null
  texture:    WebGLTexture | null
  indexCount: number
}

const VERT_SRC = `#version 300 es
in vec2 a_pos;
in vec2 a_uv;
out vec2 v_uv;
uniform vec2 u_scale;
void main() { v_uv = a_uv; gl_Position = vec4(a_pos * u_scale, 0.0, 1.0); }`

const FRAG_SRC = `#version 300 es
precision mediump float;
in vec2 v_uv;
out vec4 outColor;
uniform sampler2D u_tex;
void main() { outColor = texture(u_tex, v_uv); }`

function compile(gl: WebGL2RenderingContext, type: number, src: string): WebGLShader {
  const sh = gl.createShader(type)
  if (!sh) throw new Error('createShader failed')
  gl.shaderSource(sh, src); gl.compileShader(sh)
  if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) {
    const log = gl.getShaderInfoLog(sh); gl.deleteShader(sh)
    throw new Error(`shader compile failed: ${log}`)
  }
  return sh
}

function link(gl: WebGL2RenderingContext): WebGLProgram {
  const vs = compile(gl, gl.VERTEX_SHADER, VERT_SRC)
  const fs = compile(gl, gl.FRAGMENT_SHADER, FRAG_SRC)
  const prog = gl.createProgram()
  if (!prog) throw new Error('createProgram failed')
  gl.attachShader(prog, vs); gl.attachShader(prog, fs); gl.linkProgram(prog)
  gl.detachShader(prog, vs); gl.deleteShader(vs)
  gl.detachShader(prog, fs); gl.deleteShader(fs)
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
    const log = gl.getProgramInfoLog(prog); gl.deleteProgram(prog)
    throw new Error(`program link failed: ${log}`)
  }
  return prog
}

export function createPuppetRenderer(canvas: HTMLCanvasElement): PuppetGLRenderer {
  const gl = canvas.getContext('webgl2', {
    alpha: true, premultipliedAlpha: false, antialias: true, depth: false,
  })
  if (!gl) throw new Error('WebGL2 not available in this environment')

  let program: WebGLProgram | null = null
  let aPos = -1, aUv = -1
  let uScaleLoc: WebGLUniformLocation | null = null
  let uTexLoc:   WebGLUniformLocation | null = null
  let partsGL: PartGL[] = []

  let lastParts: readonly RenderPart[] = []
  let scaleX = 1, scaleY = 1
  let lost = false
  let destroyed = false

  function initProgram(): void {
    program = link(gl!)
    aPos = gl!.getAttribLocation(program, 'a_pos')
    aUv  = gl!.getAttribLocation(program, 'a_uv')
    uScaleLoc = gl!.getUniformLocation(program, 'u_scale')
    uTexLoc   = gl!.getUniformLocation(program, 'u_tex')
    gl!.clearColor(0, 0, 0, 0)
    gl!.enable(gl!.BLEND)
    gl!.blendFuncSeparate(gl!.SRC_ALPHA, gl!.ONE_MINUS_SRC_ALPHA, gl!.ONE, gl!.ONE_MINUS_SRC_ALPHA)
  }

  function buildPart(part: RenderPart): PartGL {
    const vao = gl!.createVertexArray()
    const posBuf = gl!.createBuffer(), uvBuf = gl!.createBuffer(), idxBuf = gl!.createBuffer()
    const texture = gl!.createTexture()

    gl!.bindVertexArray(vao)
    gl!.bindBuffer(gl!.ARRAY_BUFFER, posBuf)
    gl!.bufferData(gl!.ARRAY_BUFFER, flattenVec2(part.mesh.vertices), gl!.DYNAMIC_DRAW)
    gl!.enableVertexAttribArray(aPos)
    gl!.vertexAttribPointer(aPos, 2, gl!.FLOAT, false, 0, 0)

    gl!.bindBuffer(gl!.ARRAY_BUFFER, uvBuf)
    gl!.bufferData(gl!.ARRAY_BUFFER, flattenVec2(part.mesh.uvs), gl!.STATIC_DRAW)
    gl!.enableVertexAttribArray(aUv)
    gl!.vertexAttribPointer(aUv, 2, gl!.FLOAT, false, 0, 0)

    gl!.bindBuffer(gl!.ELEMENT_ARRAY_BUFFER, idxBuf)
    gl!.bufferData(gl!.ELEMENT_ARRAY_BUFFER, new Uint16Array(part.mesh.indices), gl!.STATIC_DRAW)
    gl!.bindVertexArray(null)

    gl!.bindTexture(gl!.TEXTURE_2D, texture)
    gl!.pixelStorei(gl!.UNPACK_FLIP_Y_WEBGL, false)
    gl!.pixelStorei(gl!.UNPACK_PREMULTIPLY_ALPHA_WEBGL, false)
    gl!.texImage2D(gl!.TEXTURE_2D, 0, gl!.RGBA, gl!.RGBA, gl!.UNSIGNED_BYTE, part.image)
    gl!.texParameteri(gl!.TEXTURE_2D, gl!.TEXTURE_MIN_FILTER, gl!.LINEAR)
    gl!.texParameteri(gl!.TEXTURE_2D, gl!.TEXTURE_MAG_FILTER, gl!.LINEAR)
    gl!.texParameteri(gl!.TEXTURE_2D, gl!.TEXTURE_WRAP_S, gl!.CLAMP_TO_EDGE)
    gl!.texParameteri(gl!.TEXTURE_2D, gl!.TEXTURE_WRAP_T, gl!.CLAMP_TO_EDGE)
    gl!.bindTexture(gl!.TEXTURE_2D, null)

    return { vao, posBuf, uvBuf, idxBuf, texture, indexCount: part.mesh.indices.length }
  }

  function disposeParts(): void {
    for (const p of partsGL) {
      gl!.deleteBuffer(p.posBuf); gl!.deleteBuffer(p.uvBuf); gl!.deleteBuffer(p.idxBuf)
      gl!.deleteTexture(p.texture); gl!.deleteVertexArray(p.vao)
    }
    partsGL = []
  }

  function onLost(e: Event): void { e.preventDefault(); lost = true }
  function onRestored(): void {
    if (destroyed) return
    initProgram()
    partsGL = lastParts.map(buildPart)
    lost = false
  }
  canvas.addEventListener('webglcontextlost', onLost as EventListener, false)
  canvas.addEventListener('webglcontextrestored', onRestored, false)

  initProgram()

  return {
    get lost() { return lost },

    setPuppet(parts) {
      lastParts = parts
      if (lost) return
      disposeParts()
      partsGL = parts.map(buildPart)
    },

    draw(perPartVertices) {
      if (lost || destroyed || partsGL.length === 0) return
      gl.viewport(0, 0, canvas.width, canvas.height)
      gl.clear(gl.COLOR_BUFFER_BIT)
      gl.useProgram(program)
      gl.uniform2f(uScaleLoc, scaleX, scaleY)
      gl.activeTexture(gl.TEXTURE0)
      gl.uniform1i(uTexLoc, 0)

      for (let i = 0; i < partsGL.length; i++) {
        const p = partsGL[i]
        const verts = perPartVertices[i]
        if (!verts || p.indexCount === 0) continue
        gl.bindVertexArray(p.vao)
        gl.bindBuffer(gl.ARRAY_BUFFER, p.posBuf)
        gl.bufferSubData(gl.ARRAY_BUFFER, 0, flattenVec2(verts))
        gl.bindTexture(gl.TEXTURE_2D, p.texture)
        gl.drawElements(gl.TRIANGLES, p.indexCount, gl.UNSIGNED_SHORT, 0)
      }
      gl.bindVertexArray(null)
    },

    resize(cssWidth, cssHeight, dpr = 1) {
      canvas.width  = Math.max(1, Math.round(cssWidth * dpr))
      canvas.height = Math.max(1, Math.round(cssHeight * dpr))
      const aspect = canvas.width / canvas.height
      if (aspect >= 1) { scaleX = 1 / aspect; scaleY = 1 } else { scaleX = 1; scaleY = aspect }
    },

    destroy() {
      if (destroyed) return
      destroyed = true
      canvas.removeEventListener('webglcontextlost', onLost as EventListener, false)
      canvas.removeEventListener('webglcontextrestored', onRestored, false)
      disposeParts()
      gl.deleteProgram(program); program = null
    },
  }
}
