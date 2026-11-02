// Node check for the shared present workers. Not a demo page.
// Run after `cargo xtask build-present-workers`.

import { readFile } from "node:fs/promises";

const clear = [13, 18, 30, 255];

async function load(pkgDir, wasmName) {
  const jsUrl = new URL(`./${pkgDir}/${wasmName}.js`, import.meta.url);
  const wasmUrl = new URL(`./${pkgDir}/${wasmName}_bg.wasm`, import.meta.url);
  const module = await import(jsUrl);
  const bytes = await readFile(wasmUrl);
  await module.default({ module_or_path: bytes });
  return module;
}

function pixel(buffer, slot, width, x, y) {
  const offset = slot + (y * width + x) * 4;
  return [...new Uint8Array(buffer, offset, 4)];
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

const webgl = await load("pkg-webgl-worker", "glgpu_webgl_worker");
webgl.install(720, 480);
const webglMemory = webgl.shared_memory();
assert(
  webglMemory.buffer instanceof SharedArrayBuffer,
  "webgl worker memory is not shared",
);
const source = await readFile(
  new URL("../examples/webgl2-triangle/p0_vertex.essl", import.meta.url),
);
const encoded = new TextEncoder().encode(source);
const commandAddr = webgl.command_addr();
assert(commandAddr % 4 === 0, "webgl command address is not aligned");
new Uint8Array(webglMemory.buffer, commandAddr, encoded.length).set(encoded);
const atoms = new Int32Array(webglMemory.buffer);
Atomics.store(atoms, webgl.command_len_addr() / 4, encoded.length);
webgl.submit_essl();
assert(webgl.frame_index() === 1, "webgl worker did not publish a frame");
const readable = Atomics.load(atoms, webgl.readable_addr() / 4) >>> 0;
const slot = readable === 0 ? webgl.slot_ptr(0) : webgl.slot_ptr(1);
Atomics.store(atoms, webgl.held_addr() / 4, readable + 1);
const corner = pixel(webglMemory.buffer, slot, 720, 0, 0);
const interior = pixel(webglMemory.buffer, slot, 720, 360, 80);
Atomics.store(atoms, webgl.held_addr() / 4, 0);
assert(corner.join() === clear.join(), `webgl corner ${corner} is not the clear`);
assert(interior.join() !== clear.join(), "webgl interior matches the clear");

const webgpu = await load("pkg-webgpu-worker", "glgpu_webgpu_worker");
webgpu.install(8, 8);
const gpuMemory = webgpu.shared_memory();
assert(
  gpuMemory.buffer instanceof SharedArrayBuffer,
  "webgpu worker memory is not shared",
);
const coords = new Float32Array(21);
const raw = [
  [1, 1, 255, 0, 0],
  [7, 1, 255, 0, 0],
  [1, 7, 255, 0, 0],
];
for (let index = 0; index < raw.length; index += 1) {
  const [x, y, r, g, b] = raw[index];
  const base = index * 7;
  coords[base] = x;
  coords[base + 1] = y;
  coords[base + 2] = 0.5;
  coords[base + 3] = r;
  coords[base + 4] = g;
  coords[base + 5] = b;
  coords[base + 6] = 255;
}
const gpuCommand = webgpu.command_addr();
assert(gpuCommand % 4 === 0, "webgpu command address is not aligned");
new Float32Array(gpuMemory.buffer, gpuCommand, 21).set(coords);
const selected = new Uint32Array(gpuMemory.buffer, gpuCommand + 84, 2);
selected[0] = 2;
selected[1] = 2;
const gpuAtoms = new Int32Array(gpuMemory.buffer);
Atomics.store(gpuAtoms, webgpu.command_len_addr() / 4, 92);
webgpu.submit_triangle();
assert(webgpu.frame_index() === 1, "webgpu worker did not publish a frame");
const gpuReadable = Atomics.load(gpuAtoms, webgpu.readable_addr() / 4) >>> 0;
const gpuSlot = gpuReadable === 0 ? webgpu.slot_ptr(0) : webgpu.slot_ptr(1);
const covered = pixel(gpuMemory.buffer, gpuSlot, 8, 2, 2);
assert(covered.join() === "255,0,0,255", `webgpu pixel ${covered} is not red`);
assert(webgpu.last_history().includes("pixel (2, 2)"), "webgpu history missing the pixel");
assert(webgpu.last_hash().length === 16, "webgpu hash was not published");

console.log("present workers published front slots from shared memory");
