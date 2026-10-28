import {
  COMPARE_CLEAR,
  COMPARE_ERROR_WEBGL,
  COMPARE_HEIGHT,
  COMPARE_STATUS_NATIVE,
  COMPARE_VERTICES,
  COMPARE_WIDTH,
} from "./compare-scene.js";
import { mountCanvas, setCompareError, setComparePending, setCompareReady } from "./compare-present.js";

export const WEBGL_FLIP_Y = false;

export const WEBGL2_CONTEXT_ATTRIBUTES = Object.freeze({
  antialias: false,
  alpha: false,
  depth: false,
  stencil: false,
  premultipliedAlpha: false,
  preserveDrawingBuffer: true,
  desynchronized: false,
});

export function triangleVertexData() {
  const data = new Float32Array(COMPARE_VERTICES.length * 8);
  COMPARE_VERTICES.forEach((vertex, index) => {
    const offset = index * 8;
    data[offset] = vertex.ndcX;
    data[offset + 1] = WEBGL_FLIP_Y ? -vertex.ndcY : vertex.ndcY;
    data[offset + 2] = vertex.clipZ;
    data[offset + 3] = vertex.clipW;
    data[offset + 4] = vertex.r;
    data[offset + 5] = vertex.g;
    data[offset + 6] = vertex.b;
    data[offset + 7] = vertex.a;
  });
  return data;
}

export function webglShaderSource() {
  return {
    vertex: `#version 300 es
precision highp float;
layout(location = 0) in vec4 a_clip;
layout(location = 1) in vec4 a_color;
out vec4 v_color;
void main() {
  gl_Position = a_clip;
  v_color = a_color;
}
`,
    fragment: `#version 300 es
precision highp float;
in vec4 v_color;
out vec4 frag;
void main() {
  frag = v_color;
}
`,
  };
}

function compile(gl, type, source) {
  const shader = gl.createShader(type);
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    const log = gl.getShaderInfoLog(shader);
    gl.deleteShader(shader);
    throw new Error(log || "shader compile failed");
  }
  return shader;
}

export function presentNativeWebgl(slot) {
  setComparePending(slot, "native");
  try {
    const canvas = mountCanvas(slot, COMPARE_WIDTH, COMPARE_HEIGHT);
    const gl = canvas.getContext("webgl2", WEBGL2_CONTEXT_ATTRIBUTES);
    if (!gl) {
      throw new Error(COMPARE_ERROR_WEBGL);
    }
    const source = webglShaderSource();
    const program = gl.createProgram();
    const vertex = compile(gl, gl.VERTEX_SHADER, source.vertex);
    const fragment = compile(gl, gl.FRAGMENT_SHADER, source.fragment);
    gl.attachShader(program, vertex);
    gl.attachShader(program, fragment);
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      throw new Error(gl.getProgramInfoLog(program) || "program link failed");
    }
    const vertices = triangleVertexData();
    const buffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, vertices, gl.STATIC_DRAW);
    gl.useProgram(program);
    gl.enableVertexAttribArray(0);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribPointer(0, 4, gl.FLOAT, false, 32, 0);
    gl.vertexAttribPointer(1, 4, gl.FLOAT, false, 32, 16);
    gl.viewport(0, 0, COMPARE_WIDTH, COMPARE_HEIGHT);
    gl.disable(gl.DITHER);
    gl.disable(gl.SCISSOR_TEST);
    gl.disable(gl.BLEND);
    gl.clearColor(
      COMPARE_CLEAR[0] / 255,
      COMPARE_CLEAR[1] / 255,
      COMPARE_CLEAR[2] / 255,
      COMPARE_CLEAR[3] / 255,
    );
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.finish();
    document.getElementById("compare-status").textContent = COMPARE_STATUS_NATIVE;
    document.getElementById("viewport-badge").textContent = "RGBA8 · 320×240";
    setCompareReady(slot, "native");
  } catch (error) {
    const message = String(error && error.message ? error.message : error);
    setCompareError(
      slot,
      "native",
      message.includes(COMPARE_ERROR_WEBGL) ? COMPARE_ERROR_WEBGL : message,
    );
  }
}
