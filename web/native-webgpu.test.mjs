import assert from "node:assert/strict";
import test from "node:test";
import { COMPARE_VERTICES } from "./compare-scene.js";
import { triangleVertexData, webgpuShaderSource } from "./native-webgpu.js";

test("webgpu vertices come from the scene module", () => {
  const floats = Array.from(triangleVertexData());
  assert.equal(floats.length, 18);
  assert.equal(floats[0], COMPARE_VERTICES[0].ndcX);
  assert.equal(floats[1], COMPARE_VERTICES[0].ndcY);
  assert.equal(floats[2], COMPARE_VERTICES[0].r);
  assert.equal(floats[5], COMPARE_VERTICES[0].a);
  assert.equal(floats[7], Math.fround(COMPARE_VERTICES[1].ndcY));
  assert.equal(floats[14], Math.fround(COMPARE_VERTICES[2].r));
  assert.equal(floats[15], Math.fround(COMPARE_VERTICES[2].g));
});

test("webgpu shader uses scene clip z/w and does not use the editor z", () => {
  const source = webgpuShaderSource();
  assert.match(source, /@vertex/);
  assert.match(source, /fn vs_main/);
  assert.match(source, /fn fs_main/);
  assert.match(source, /@location\(0\) color: vec4f/);
  assert.equal(source.includes("0.5"), false);
  assert.equal(source.includes(String(COMPARE_VERTICES[0].clipZ)), true);
  assert.equal(source.includes(String(COMPARE_VERTICES[0].clipW)), true);
});
