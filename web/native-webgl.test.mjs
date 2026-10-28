import assert from "node:assert/strict";
import test from "node:test";
import { COMPARE_VERTICES } from "./compare-scene.js";
import {
  WEBGL2_CONTEXT_ATTRIBUTES,
  triangleVertexData,
  webglShaderSource,
} from "./native-webgl.js";

test("webgl2 context attributes match the spec", () => {
  assert.deepEqual(WEBGL2_CONTEXT_ATTRIBUTES, {
    antialias: false,
    alpha: false,
    depth: false,
    stencil: false,
    premultipliedAlpha: false,
    preserveDrawingBuffer: true,
    desynchronized: false,
  });
});

test("webgl vertices and shader come from the scene module", () => {
  const floats = Array.from(triangleVertexData());
  assert.equal(floats.length, 24);
  assert.equal(floats[0], COMPARE_VERTICES[0].ndcX);
  assert.equal(floats[1], COMPARE_VERTICES[0].ndcY);
  assert.equal(floats[2], COMPARE_VERTICES[0].clipZ);
  assert.equal(floats[3], COMPARE_VERTICES[0].clipW);
  assert.equal(floats[4], COMPARE_VERTICES[0].r);
  const source = webglShaderSource();
  assert.match(source.vertex, /#version 300 es/);
  assert.match(source.vertex, /gl_Position/);
  assert.match(source.fragment, /out vec4 frag/);
  assert.equal(source.vertex.includes("0.5"), false);
  assert.equal(source.fragment.includes("0.5"), false);
});
