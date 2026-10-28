export const COMPARE_WIDTH = 320;
export const COMPARE_HEIGHT = 240;
export const COMPARE_CLEAR = Object.freeze([13, 18, 30, 255]);

export const COMPARE_VERTICES = Object.freeze([
  Object.freeze({ ndcX: 0.0, ndcY: 0.75, clipZ: 0.0, clipW: 1.0, r: 1.0, g: 0.2, b: 0.2, a: 1.0 }),
  Object.freeze({ ndcX: -0.75, ndcY: -0.65, clipZ: 0.0, clipW: 1.0, r: 0.2, g: 1.0, b: 0.3, a: 1.0 }),
  Object.freeze({ ndcX: 0.75, ndcY: -0.65, clipZ: 0.0, clipW: 1.0, r: 0.2, g: 0.45, b: 1.0, a: 1.0 }),
]);

export const COMPARE_STATUS_REFERENCE = "正在显示：固定对照场景 · 参考实现";
export const COMPARE_STATUS_NATIVE = "正在显示：固定对照场景 · 原生";
export const COMPARE_STATUS_EDITOR = "正在显示：编辑器预览 · 不参与原生对照";
export const COMPARE_NOTE_WEBGPU =
  "原生对照只用于观看画面和本地像素比较。它不运行编辑器里的调试流程，也不改变 CpuGpuSubset — NOT WEBGPU CONFORMANT。";
export const COMPARE_NOTE_WEBGL =
  "原生对照只用于观看画面和本地像素比较。它不运行编辑器里的调试流程，也不改变 CpuWebGl2LearningTrack — NOT WEBGL CONFORMANT。";
export const COMPARE_ERROR_CHROME =
  "原生对照需要本机已安装的 Google Chrome。Playwright 的 chrome 通道启动失败，命令失败。";
export const COMPARE_ERROR_WEBGPU =
  "原生对照需要本机 Google Chrome 的 WebGPU（navigator.gpu）。当前环境没有可用的 WebGPU，命令失败。";
export const COMPARE_ERROR_WEBGL =
  "原生对照需要本机 Google Chrome 的 WebGL2。当前环境不能创建 WebGL2 上下文，命令失败。";

export function compareClearBytes() {
  return Uint8Array.from(COMPARE_CLEAR);
}

export function compareVertexFloats() {
  const floats = new Float32Array(24);
  COMPARE_VERTICES.forEach((vertex, index) => {
    const offset = index * 8;
    floats[offset] = vertex.ndcX;
    floats[offset + 1] = vertex.ndcY;
    floats[offset + 2] = vertex.clipZ;
    floats[offset + 3] = vertex.clipW;
    floats[offset + 4] = vertex.r;
    floats[offset + 5] = vertex.g;
    floats[offset + 6] = vertex.b;
    floats[offset + 7] = vertex.a;
  });
  return floats;
}
