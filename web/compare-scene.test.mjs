import assert from "node:assert/strict";
import test from "node:test";
import {
  COMPARE_CLEAR,
  COMPARE_ERROR_CHROME,
  COMPARE_ERROR_WEBGL,
  COMPARE_ERROR_WEBGPU,
  COMPARE_HEIGHT,
  COMPARE_NOTE_WEBGL,
  COMPARE_NOTE_WEBGPU,
  COMPARE_STATUS_EDITOR,
  COMPARE_STATUS_NATIVE,
  COMPARE_STATUS_REFERENCE,
  COMPARE_VERTICES,
  COMPARE_WIDTH,
  compareClearBytes,
  compareVertexFloats,
} from "./compare-scene.js";

test("compare scene matches the locked 320×240 P0 triangle", () => {
  assert.equal(COMPARE_WIDTH, 320);
  assert.equal(COMPARE_HEIGHT, 240);
  assert.deepEqual(COMPARE_CLEAR, [13, 18, 30, 255]);
  assert.deepEqual(
    COMPARE_VERTICES.map((vertex) => [vertex.ndcX, vertex.ndcY, vertex.clipZ, vertex.clipW]),
    [
      [0.0, 0.75, 0.0, 1.0],
      [-0.75, -0.65, 0.0, 1.0],
      [0.75, -0.65, 0.0, 1.0],
    ],
  );
  assert.deepEqual(
    COMPARE_VERTICES.map((vertex) => [vertex.r, vertex.g, vertex.b, vertex.a]),
    [
      [1.0, 0.2, 0.2, 1.0],
      [0.2, 1.0, 0.3, 1.0],
      [0.2, 0.45, 1.0, 1.0],
    ],
  );
  assert.deepEqual(Array.from(compareClearBytes()), [13, 18, 30, 255]);
  assert.ok(compareVertexFloats() instanceof Float32Array);
  assert.deepEqual(Array.from(compareVertexFloats()), Array.from(new Float32Array([
    0.0, 0.75, 0.0, 1.0, 1.0, 0.2, 0.2, 1.0,
    -0.75, -0.65, 0.0, 1.0, 0.2, 1.0, 0.3, 1.0,
    0.75, -0.65, 0.0, 1.0, 0.2, 0.45, 1.0, 1.0,
  ])));
});

test("compare copy matches the spec sentences", () => {
  assert.equal(COMPARE_STATUS_REFERENCE, "正在显示：固定对照场景 · 参考实现");
  assert.equal(COMPARE_STATUS_NATIVE, "正在显示：固定对照场景 · 原生");
  assert.equal(COMPARE_STATUS_EDITOR, "正在显示：编辑器预览 · 不参与原生对照");
  assert.equal(
    COMPARE_NOTE_WEBGPU,
    "原生对照只用于观看画面和本地像素比较。它不运行编辑器里的调试流程，也不改变 CpuGpuSubset — NOT WEBGPU CONFORMANT。",
  );
  assert.equal(
    COMPARE_NOTE_WEBGL,
    "原生对照只用于观看画面和本地像素比较。它不运行编辑器里的调试流程，也不改变 CpuWebGl2LearningTrack — NOT WEBGL CONFORMANT。",
  );
  assert.equal(
    COMPARE_ERROR_CHROME,
    "原生对照需要本机已安装的 Google Chrome。Playwright 的 chrome 通道启动失败，命令失败。",
  );
  assert.equal(
    COMPARE_ERROR_WEBGPU,
    "原生对照需要本机 Google Chrome 的 WebGPU（navigator.gpu）。当前环境没有可用的 WebGPU，命令失败。",
  );
  assert.equal(
    COMPARE_ERROR_WEBGL,
    "原生对照需要本机 Google Chrome 的 WebGL2。当前环境不能创建 WebGL2 上下文，命令失败。",
  );
});
