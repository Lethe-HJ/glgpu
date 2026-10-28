# Native Compare E2E Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show one fixed 320×240 scene on both debug pages, toggle it between the CPU reference and a native WebGPU or WebGL2 frame in the same viewport slot, and add `cargo xtask e2e-compare` to require exact RGBA equality locally.

**Architecture:** `web/compare-scene.js` is the only runtime copy of the scene numbers. Each Wasm package exports `compare_frame_rgba8`, which rasterizes those numbers with the existing CPU path for that page. Page JS puts the reference bytes on a Canvas 2D canvas inside `#compare-slot`, or replaces that canvas with one native frame. Playwright reads both canvases through `drawImage` + `getImageData` and fails on any byte difference.

**Tech Stack:** Rust 1.93 / existing `glgpu-web` and `glgpu-webgl-web` Wasm shells, Canvas 2D `putImageData`, browser WebGPU and WebGL2 from `web/*.js` only, `xtask`, Node 22 built-in test runner, Playwright 1.64.1, pngjs 7.0.0.

## Global Constraints

- Exact compare size is 320×240, `320 × 240 × 4 = 307200` bytes. Any different byte fails. No tolerance, no per-channel epsilon, no ignored edge pixels, no soft skip.
- Clear color is RGBA8 `(13, 18, 30, 255)`. Vertices are NDC `(0.0, 0.75)`, `(-0.75, -0.65)`, `(0.75, -0.65)` with clip z `0`, clip w `1`, and colors `(1.0, 0.2, 0.2, 1.0)`, `(0.2, 1.0, 0.3, 1.0)`, `(0.2, 0.45, 1.0, 1.0)`.
- Those literals live in `web/compare-scene.js` at runtime. `compare_frame.rs` must not contain `0.75` or `13, 18, 30`.
- Continuous storage anchors at 320×240 are `(160, 30)`, `(40, 198)`, `(280, 198)`. Vertex 0 is toward the top of the image.
- WebGPU page native path is WebGPU with no MSAA. WebGL page native path is WebGL2 with `antialias: false`, `alpha: false`, `depth: false`, `stencil: false`, `premultipliedAlpha: false`, `preserveDrawingBuffer: true`, `desynchronized: false`, plus `gl.disable(gl.DITHER)`.
- WebGPU canvas format is `navigator.gpu.getPreferredCanvasFormat()` and must be `rgba8unorm` or `bgra8unorm`. `alphaMode` is `"opaque"`. No `rgba16float`, no MSAA texture, no `resolveTarget`.
- One viewport slot, not two side-by-side canvases. Default mode is `reference`. Editor 「运行」 uses the existing 720×480 preview and does not call `compare_frame_rgba8`.
- Missing Google Chrome, WebGPU, or WebGL2 is a hard failure. The process output must contain the matching sentence verbatim:
  - `原生对照需要本机已安装的 Google Chrome。Playwright 的 chrome 通道启动失败，命令失败。`
  - `原生对照需要本机 Google Chrome 的 WebGPU（navigator.gpu）。当前环境没有可用的 WebGPU，命令失败。`
  - `原生对照需要本机 Google Chrome 的 WebGL2。当前环境不能创建 WebGL2 上下文，命令失败。`
- Command is `cargo xtask e2e-compare`, default bind `127.0.0.1:8765`. It does not build MT and does not open `?threads=1`.
- `cargo test -p xtask` must not launch Chrome, bind 8765, or require npm. Do not edit `.github/workflows/pages.yml`.
- Do not add `WebGl2RenderingContext`, `Gpu`, `GpuDevice`, or `GpuCanvasContext` to any Rust crate. Do not change `glgpu-raster` or `glgpu-webgl` coverage rules. Do not weaken `stable_hash64` tests.
- Permanent banners stay `CpuGpuSubset — NOT WEBGPU CONFORMANT` and `CpuWebGl2LearningTrack — NOT WEBGL CONFORMANT`.
- If a local GPU differs by one pixel, follow spec section 4.2 in order. Do not mark Task 7 done while any byte differs, and do not change the comparator.

---

## File Structure

- `web/compare-scene.js` — scene numbers, status copy, and the three failure sentences. Create.
- `web/compare-scene.test.mjs` — Node lock that the exports equal the spec table. Create.
- `web/compare-present.js` — slot canvas mount, reference `putImageData`, editor `putImageData`, ready/error attributes. Create.
- `web/native-webgpu.js` — one WebGPU frame for `index.html`. Create.
- `web/native-webgl.js` — one WebGL2 frame for `webgl.html`. Create.
- `web/index.html` — import the WebGPU native module and install `window.glgpuPresentCompare`. Modify.
- `web/webgl.html` — import the WebGL2 native module and install the same window names. Modify.
- `crates/glgpu-web/src/compare_frame.rs` — CPU WebGPU compare raster. Create.
- `crates/glgpu-web/src/lib.rs` — export `compare_frame_rgba8`; slot, toggle, and editor present. Modify.
- `crates/glgpu-web/Cargo.toml` — add web-sys feature `ImageData` only if a later edit still needs it. This plan reads pixels from JS, so do not add it.
- `crates/glgpu-webgl-web/src/compare_frame.rs` — CPU WebGL compare raster. Create.
- `crates/glgpu-webgl-web/src/lib.rs` — same page behavior for the WebGL shell. Modify.
- `e2e/package.json`, `e2e/package-lock.json`, `e2e/playwright.config.mjs`, `e2e/compare.spec.mjs` — local browser compare. Create.
- `xtask/src/main.rs` — `e2e-compare` parser and runner. Modify.
- `.gitignore` — ignore `e2e/node_modules/`. Modify.
- `docs/testing-and-acceptance.zh-CN.md` — new section and the first-frame hash wording. Modify.

Window functions installed by the HTML modules:

- `glgpuPresentCompare(mode)` where `mode` is `"reference"` or `"native"`.
- `glgpuPresentEditorFrame(width, height, pixels)` for the 720×480 editor preview.
- `glgpuReadCanvasPixel(x, y)` returns a `Uint8Array` of four bytes from the current 2D canvas.

Wasm export on both packages:

```text
compare_frame_rgba8(width: u32, height: u32, clear: &[u8], vertices: &[f32]) -> Result<Vec<u8>, JsValue>
```

Rust function on both packages:

```text
compare_frame::compare_frame_rgba8(width: u32, height: u32, clear: &[u8], vertices: &[f32]) -> Result<Vec<u8>, CompareFrameError>
```

`vertices` is 24 finite `f32`s: `ndc_x, ndc_y, clip_z, clip_w, r, g, b, a` for three vertices. `clear` is exactly 4 bytes. `width` and `height` must be in `1..=4096`.

## Task 1: Scene constants

**Files:**
- Create: `web/compare-scene.js`
- Create: `web/compare-scene.test.mjs`
- Test: `web/compare-scene.test.mjs`

**Interfaces:**
- Consumes: the numeric table in spec section 5.2 and the sentences in spec sections 5.1 and 7.4.
- Produces: `COMPARE_WIDTH`, `COMPARE_HEIGHT`, `COMPARE_CLEAR`, `COMPARE_VERTICES`, `compareClearBytes()`, `compareVertexFloats()`, `COMPARE_STATUS_REFERENCE`, `COMPARE_STATUS_NATIVE`, `COMPARE_STATUS_EDITOR`, `COMPARE_NOTE_WEBGPU`, `COMPARE_NOTE_WEBGL`, `COMPARE_ERROR_CHROME`, `COMPARE_ERROR_WEBGPU`, `COMPARE_ERROR_WEBGL`.

- [ ] **Step 1: Write the failing test**

Create `web/compare-scene.test.mjs`:

```javascript
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
  assert.deepEqual(Array.from(compareVertexFloats()), [
    0.0, 0.75, 0.0, 1.0, 1.0, 0.2, 0.2, 1.0,
    -0.75, -0.65, 0.0, 1.0, 0.2, 1.0, 0.3, 1.0,
    0.75, -0.65, 0.0, 1.0, 0.2, 0.45, 1.0, 1.0,
  ]);
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
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `node --test web/compare-scene.test.mjs`

Expected: FAIL with `ERR_MODULE_NOT_FOUND` for `./compare-scene.js`.

- [ ] **Step 3: Write the module**

Create `web/compare-scene.js`:

```javascript
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
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `node --test web/compare-scene.test.mjs`

Expected: PASS, 2 tests, 0 failed.

- [ ] **Step 5: Commit**

```bash
git add web/compare-scene.js web/compare-scene.test.mjs
git commit -m "Add the locked scene constants for native compare."
```

## Task 2: WebGPU `compare_frame_rgba8`

**Files:**
- Create: `crates/glgpu-web/src/compare_frame.rs`
- Modify: `crates/glgpu-web/src/lib.rs` (add `mod compare_frame;` next to the other module declarations, and the wasm export next to `boot`)
- Test: `crates/glgpu-web/src/compare_frame.rs`

**Interfaces:**
- Consumes: `CpuGpuSubset::new().request_adapter().request_device(DeviceDescriptor::new(extent))`, `CommandEncoder::clear`, `CommandEncoder::draw_triangle`, `ImmediateMainOwnerQueue::submit`, `readback_rgba8`. Screen mapping is `(ndc_x * 0.5 + 0.5) * width` and `(0.5 - ndc_y * 0.5) * height`. Color quantization is `(value.clamp(0.0, 1.0) * 255.0).round() as u8`.
- Produces: `compare_frame::compare_frame_rgba8(width: u32, height: u32, clear: &[u8], vertices: &[f32]) -> Result<Vec<u8>, CompareFrameError>` and the wasm export `compare_frame_rgba8`.

- [ ] **Step 1: Write the failing test**

Add `mod compare_frame;` to `crates/glgpu-web/src/lib.rs` immediately after `#![forbid(unsafe_code)]`.

Create `crates/glgpu-web/src/compare_frame.rs` with only this test module:

```rust
#[cfg(test)]
mod tests {
    use super::compare_frame_rgba8;

    fn locked_vertices() -> [f32; 24] {
        [
            0.0, 0.75, 0.0, 1.0, 1.0, 0.2, 0.2, 1.0, -0.75, -0.65, 0.0, 1.0, 0.2, 1.0, 0.3, 1.0,
            0.75, -0.65, 0.0, 1.0, 0.2, 0.45, 1.0, 1.0,
        ]
    }

    fn pixel(frame: &[u8], x: u32, y: u32) -> [u8; 4] {
        let index = ((y * 320 + x) * 4) as usize;
        frame[index..index + 4].try_into().unwrap()
    }

    #[test]
    fn locked_scene_is_320_by_240_and_leaves_the_corner_clear() {
        let frame = compare_frame_rgba8(320, 240, &[13, 18, 30, 255], &locked_vertices()).unwrap();
        assert_eq!(frame.len(), 307_200);
        assert_eq!(pixel(&frame, 0, 0), [13, 18, 30, 255]);
        assert_ne!(pixel(&frame, 160, 142), [13, 18, 30, 255]);
    }

    #[test]
    fn storage_anchors_match_the_spec() {
        assert_eq!(super::storage_position(0.0, 0.75, 320, 240), (160.0, 30.0));
        assert_eq!(super::storage_position(-0.75, -0.65, 320, 240), (40.0, 198.0));
        assert_eq!(super::storage_position(0.75, -0.65, 320, 240), (280.0, 198.0));
        assert_eq!(super::quantize_unorm8(0.2), 51);
        assert_eq!(super::quantize_unorm8(0.3), 77);
        assert_eq!(super::quantize_unorm8(0.45), 115);
        assert_eq!(super::quantize_unorm8(1.0), 255);
    }

    #[test]
    fn rejects_bad_extent_clear_and_vertices() {
        let vertices = locked_vertices();
        let error = compare_frame_rgba8(0, 240, &[13, 18, 30, 255], &vertices).unwrap_err();
        assert!(error.to_string().contains("outside 1..=4096"));
        let error = compare_frame_rgba8(4097, 240, &[13, 18, 30, 255], &vertices).unwrap_err();
        assert!(error.to_string().contains("outside 1..=4096"));
        let error = compare_frame_rgba8(320, 240, &[13, 18, 30], &vertices).unwrap_err();
        assert!(error.to_string().contains("4 bytes"));
        let error = compare_frame_rgba8(320, 240, &[13, 18, 30, 255], &vertices[..23]).unwrap_err();
        assert!(error.to_string().contains("24 floats"));
        let mut non_finite = vertices;
        non_finite[1] = f32::NAN;
        let error = compare_frame_rgba8(320, 240, &[13, 18, 30, 255], &non_finite).unwrap_err();
        assert!(error.to_string().contains("not finite"));
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p glgpu-web compare_frame -- --test-threads=1`

Expected: FAIL to compile, `cannot find function compare_frame_rgba8` or `cannot find function storage_position`.

- [ ] **Step 3: Implement the raster**

Replace `crates/glgpu-web/src/compare_frame.rs` with the test module from Step 1 at the bottom, and this implementation above it:

```rust
use std::fmt;

use glgpu_core::{Extent2d, Rgba8, ScreenVertex, Triangle};
use glgpu_webgpu::{CpuGpuSubset, DeviceDescriptor};

#[derive(Debug, PartialEq)]
pub enum CompareFrameError {
    ExtentOutOfRange { width: u32, height: u32 },
    ClearLength { actual: usize },
    VertexLength { actual: usize },
    NonFiniteVertex { index: usize },
    Raster(String),
}

impl fmt::Display for CompareFrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExtentOutOfRange { width, height } => {
                write!(f, "compare extent {width}x{height} is outside 1..=4096")
            }
            Self::ClearLength { actual } => {
                write!(f, "compare clear must be 4 bytes, got {actual}")
            }
            Self::VertexLength { actual } => {
                write!(f, "compare vertices must be 24 floats, got {actual}")
            }
            Self::NonFiniteVertex { index } => {
                write!(f, "compare vertex float {index} is not finite")
            }
            Self::Raster(message) => write!(f, "compare raster failed: {message}"),
        }
    }
}

pub(crate) fn storage_position(ndc_x: f32, ndc_y: f32, width: u32, height: u32) -> (f32, f32) {
    (
        (ndc_x * 0.5 + 0.5) * width as f32,
        (0.5 - ndc_y * 0.5) * height as f32,
    )
}

pub(crate) fn quantize_unorm8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

pub fn compare_frame_rgba8(
    width: u32,
    height: u32,
    clear: &[u8],
    vertices: &[f32],
) -> Result<Vec<u8>, CompareFrameError> {
    if !(1..=4096).contains(&width) || !(1..=4096).contains(&height) {
        return Err(CompareFrameError::ExtentOutOfRange { width, height });
    }
    if clear.len() != 4 {
        return Err(CompareFrameError::ClearLength { actual: clear.len() });
    }
    if vertices.len() != 24 {
        return Err(CompareFrameError::VertexLength { actual: vertices.len() });
    }
    if let Some(index) = vertices.iter().position(|value| !value.is_finite()) {
        return Err(CompareFrameError::NonFiniteVertex { index });
    }

    let screen = [0, 1, 2].map(|index| {
        let offset = index * 8;
        let (x, y) = storage_position(vertices[offset], vertices[offset + 1], width, height);
        ScreenVertex::new(
            x,
            y,
            vertices[offset + 2],
            Rgba8::new(
                quantize_unorm8(vertices[offset + 4]),
                quantize_unorm8(vertices[offset + 5]),
                quantize_unorm8(vertices[offset + 6]),
                quantize_unorm8(vertices[offset + 7]),
            ),
        )
    });

    let extent = Extent2d::new(width, height);
    let mut device = CpuGpuSubset::new()
        .request_adapter()
        .request_device(DeviceDescriptor::new(extent))
        .map_err(|error| CompareFrameError::Raster(error.to_string()))?;
    let mut encoder = device.create_command_encoder();
    encoder
        .clear(Rgba8::new(clear[0], clear[1], clear[2], clear[3]), 1.0)
        .map_err(|error| CompareFrameError::Raster(error.to_string()))?;
    encoder
        .draw_triangle(Triangle::from_vertices(1, screen[0], screen[1], screen[2]))
        .map_err(|error| CompareFrameError::Raster(error.to_string()))?;
    let command_buffer = encoder
        .finish()
        .map_err(|error| CompareFrameError::Raster(error.to_string()))?;
    device
        .queue_mut()
        .submit(command_buffer)
        .map_err(|error| CompareFrameError::Raster(error.to_string()))?;
    device
        .queue()
        .readback_rgba8()
        .map_err(|error| CompareFrameError::Raster(error.to_string()))
}
```

Clear depth stays `1.0`. Vertex depth is `clip_z` (`vertices[offset + 2]`), which is `0` for the locked scene, so the triangle wins the existing strict-less depth test. Do not call `select_pixel`. Do not execute WGSL.

In `crates/glgpu-web/src/lib.rs`, immediately above the existing wasm `boot` export, add:

```rust
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn compare_frame_rgba8(
    width: u32,
    height: u32,
    clear: &[u8],
    vertices: &[f32],
) -> Result<Vec<u8>, wasm_bindgen::JsValue> {
    compare_frame::compare_frame_rgba8(width, height, clear, vertices)
        .map_err(|error| wasm_bindgen::JsValue::from_str(&error.to_string()))
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p glgpu-web compare_frame -- --test-threads=1`

Expected: PASS, 3 tests.

Run: `rg -n "0\\.75|13, 18, 30" crates/glgpu-web/src/compare_frame.rs`

Expected: matches only inside `mod tests`, none in the implementation functions.

Run: `cargo test -p glgpu-raster -p glgpu-webgpu`

Expected: PASS. Existing hash and framebuffer equality tests stay exact.

- [ ] **Step 5: Commit**

```bash
git add crates/glgpu-web/src/compare_frame.rs crates/glgpu-web/src/lib.rs
git commit -m "Export the WebGPU CPU compare frame."
```

## Task 3: WebGL `compare_frame_rgba8`

**Files:**
- Create: `crates/glgpu-webgl-web/src/compare_frame.rs`
- Modify: `crates/glgpu-webgl-web/src/lib.rs` (add `mod compare_frame;` after `#![forbid(unsafe_code)]`, and the wasm export next to `boot`)
- Test: `crates/glgpu-webgl-web/src/compare_frame.rs`

**Interfaces:**
- Consumes: `WebGl2LearningTrack::new().create_context(width, height)`, `LearningContext::clear`, `LearningContext::draw_clip_space_triangle`, `LearningContext::to_rgba8_bytes`, `ClipVertex::new`.
- Produces: the same `compare_frame::compare_frame_rgba8` signature and wasm export name as Task 2. `CompareFrameError` uses the same `Display` text.

- [ ] **Step 1: Write the failing test**

Add `mod compare_frame;` to `crates/glgpu-webgl-web/src/lib.rs` after `#![forbid(unsafe_code)]`.

Create `crates/glgpu-webgl-web/src/compare_frame.rs` containing the same three tests as Task 2 Step 1. Keep `locked_vertices`, `pixel`, `locked_scene_is_320_by_240_and_leaves_the_corner_clear`, `storage_anchors_match_the_spec`, and `rejects_bad_extent_clear_and_vertices` unchanged, including the calls to `super::storage_position` and `super::quantize_unorm8`.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p glgpu-webgl-web compare_frame -- --test-threads=1`

Expected: FAIL to compile, `cannot find function compare_frame_rgba8`.

- [ ] **Step 3: Implement the WebGL CPU frame**

Put this implementation above the tests:

```rust
use std::fmt;

use glgpu_webgl::{ClipVertex, LearningContext, Rgba8, WebGl2LearningTrack};

#[derive(Debug, PartialEq)]
pub enum CompareFrameError {
    ExtentOutOfRange { width: u32, height: u32 },
    ClearLength { actual: usize },
    VertexLength { actual: usize },
    NonFiniteVertex { index: usize },
    Raster(String),
}

impl fmt::Display for CompareFrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExtentOutOfRange { width, height } => {
                write!(f, "compare extent {width}x{height} is outside 1..=4096")
            }
            Self::ClearLength { actual } => {
                write!(f, "compare clear must be 4 bytes, got {actual}")
            }
            Self::VertexLength { actual } => {
                write!(f, "compare vertices must be 24 floats, got {actual}")
            }
            Self::NonFiniteVertex { index } => {
                write!(f, "compare vertex float {index} is not finite")
            }
            Self::Raster(message) => write!(f, "compare raster failed: {message}"),
        }
    }
}

pub(crate) fn storage_position(ndc_x: f32, ndc_y: f32, width: u32, height: u32) -> (f32, f32) {
    let window_x = (ndc_x + 1.0) * 0.5 * width as f32;
    let window_y = (ndc_y + 1.0) * 0.5 * height as f32;
    (window_x, height as f32 - window_y)
}

pub(crate) fn quantize_unorm8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn validate(
    width: u32,
    height: u32,
    clear: &[u8],
    vertices: &[f32],
) -> Result<(), CompareFrameError> {
    if !(1..=4096).contains(&width) || !(1..=4096).contains(&height) {
        return Err(CompareFrameError::ExtentOutOfRange { width, height });
    }
    if clear.len() != 4 {
        return Err(CompareFrameError::ClearLength { actual: clear.len() });
    }
    if vertices.len() != 24 {
        return Err(CompareFrameError::VertexLength { actual: vertices.len() });
    }
    if let Some(index) = vertices.iter().position(|value| !value.is_finite()) {
        return Err(CompareFrameError::NonFiniteVertex { index });
    }
    Ok(())
}

pub fn compare_frame_rgba8(
    width: u32,
    height: u32,
    clear: &[u8],
    vertices: &[f32],
) -> Result<Vec<u8>, CompareFrameError> {
    validate(width, height, clear, vertices)?;
    let mut context: LearningContext = WebGl2LearningTrack::new()
        .create_context(width, height)
        .map_err(|error| CompareFrameError::Raster(error.to_string()))?;
    context.clear(Rgba8::new(clear[0], clear[1], clear[2], clear[3]));
    let clips = [0, 1, 2].map(|index| {
        let offset = index * 8;
        ClipVertex::new(
            vertices[offset],
            vertices[offset + 1],
            vertices[offset + 2],
            vertices[offset + 3],
            [
                vertices[offset + 4],
                vertices[offset + 5],
                vertices[offset + 6],
                vertices[offset + 7],
            ],
        )
    });
    context
        .draw_clip_space_triangle(clips)
        .map_err(|error| CompareFrameError::Raster(error.to_string()))?;
    Ok(context.to_rgba8_bytes())
}
```

`storage_position` uses the WebGL window projection and the storage flip `height - window_y`. For the locked `w = 1` vertices it produces the same anchors as Task 2. Do not call browser WebGL. Do not run the scalar ESSL subset.

Add the same wasm export as Task 2 to `crates/glgpu-webgl-web/src/lib.rs`, calling `compare_frame::compare_frame_rgba8`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p glgpu-webgl-web compare_frame -- --test-threads=1`

Expected: PASS, 3 tests.

Run: `rg -n "0\\.75|13, 18, 30" crates/glgpu-webgl-web/src/compare_frame.rs`

Expected: matches only inside `mod tests`.

Run: `cargo test -p glgpu-webgl`

Expected: PASS. Coverage tests are unchanged.

- [ ] **Step 5: Commit**

```bash
git add crates/glgpu-webgl-web/src/compare_frame.rs crates/glgpu-webgl-web/src/lib.rs
git commit -m "Export the WebGL CPU compare frame."
```

## Task 4: Reference and editor slot presentation

**Files:**
- Create: `web/compare-present.js`
- Modify: `web/index.html`
- Modify: `web/webgl.html`
- Modify: `crates/glgpu-web/src/lib.rs`
- Modify: `crates/glgpu-webgl-web/src/lib.rs`
- Modify: `crates/glgpu-webgl-web/src/debug.css`
- Test: `web/compare-scene.test.mjs` still passes; Rust tests below

**Interfaces:**
- Consumes: `compare_frame_rgba8` from the Wasm module that the page already imports, plus Task 1 exports.
- Produces: `presentReference(slot, compareFrameRgba8)`, `presentEditorFrame(slot, width, height, pixels)`, `readCanvasPixel(x, y)`, `mountCanvas(slot, width, height)`, `setComparePending(slot, state)`, `setCompareReady(slot, state)`, `setCompareError(slot, state, message)`. DOM ids `compare-slot`, `compare-status`, `compare-note`, `viewport-badge`.

- [ ] **Step 1: Write the failing page test**

Append this test to `web/compare-scene.test.mjs` and add the import:

```javascript
import { mountCanvasSource } from "./compare-present.js";

test("present helper forces buffer css size and does not scale", () => {
  assert.match(mountCanvasSource(), /canvas\.width = width/);
  assert.match(mountCanvasSource(), /canvas\.height = height/);
  assert.match(mountCanvasSource(), /style\.width = `\$\{width\}px`/);
  assert.match(mountCanvasSource(), /style\.transform = "none"/);
  assert.match(mountCanvasSource(), /style\.zoom = "1"/);
  assert.doesNotMatch(mountCanvasSource(), /devicePixelRatio/);
  assert.doesNotMatch(mountCanvasSource(), /requestAnimationFrame/);
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `node --test web/compare-scene.test.mjs`

Expected: FAIL with `ERR_MODULE_NOT_FOUND` for `./compare-present.js`.

- [ ] **Step 3: Add the presenter**

Create `web/compare-present.js`:

```javascript
import {
  COMPARE_HEIGHT,
  COMPARE_STATUS_EDITOR,
  COMPARE_STATUS_REFERENCE,
  COMPARE_WIDTH,
  compareClearBytes,
  compareVertexFloats,
} from "./compare-scene.js";

export function mountCanvasSource() {
  return mountCanvas.toString();
}

export function setComparePending(slot, state) {
  slot.dataset.compareState = state;
  delete slot.dataset.compareReady;
  delete slot.dataset.compareError;
}

export function setCompareReady(slot, state) {
  slot.dataset.compareState = state;
  slot.dataset.compareReady = "true";
  delete slot.dataset.compareError;
}

export function setCompareError(slot, state, message) {
  slot.dataset.compareState = state;
  slot.dataset.compareReady = "error";
  slot.dataset.compareError = message;
}

export function mountCanvas(slot, width, height) {
  slot.replaceChildren();
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  canvas.className = "viewport";
  canvas.style.width = `${width}px`;
  canvas.style.height = `${height}px`;
  canvas.style.transform = "none";
  canvas.style.zoom = "1";
  canvas.setAttribute("aria-label", "GLGPU compare framebuffer");
  slot.append(canvas);
  return canvas;
}

function paint(slot, width, height, pixels, state, status) {
  const canvas = mountCanvas(slot, width, height);
  const context = canvas.getContext("2d", { alpha: false, willReadFrequently: true });
  context.putImageData(new ImageData(new Uint8ClampedArray(pixels), width, height), 0, 0);
  document.getElementById("compare-status").textContent = status;
  document.getElementById("viewport-badge").textContent = `RGBA8 · ${width}×${height}`;
  setCompareReady(slot, state);
}

export function presentReference(slot, compareFrameRgba8) {
  setComparePending(slot, "reference");
  try {
    const pixels = compareFrameRgba8(
      COMPARE_WIDTH,
      COMPARE_HEIGHT,
      compareClearBytes(),
      compareVertexFloats(),
    );
    paint(slot, COMPARE_WIDTH, COMPARE_HEIGHT, pixels, "reference", COMPARE_STATUS_REFERENCE);
  } catch (error) {
    setCompareError(slot, "reference", String(error));
    throw error;
  }
}

export function presentEditorFrame(slot, width, height, pixels) {
  paint(slot, width, height, pixels, "editor", COMPARE_STATUS_EDITOR);
}

export function readCanvasPixel(x, y) {
  const canvas = document.querySelector("#compare-slot canvas");
  const context = canvas.getContext("2d");
  return new Uint8Array(context.getImageData(x, y, 1, 1).data);
}
```

`mountCanvas` replaces every child, so the slot never holds two canvases. `presentReference` calls the Wasm export with scene-module bytes. `presentEditorFrame` does not call `compare_frame_rgba8`.

- [ ] **Step 4: Run the node test**

Run: `node --test web/compare-scene.test.mjs`

Expected: PASS, 3 tests.

- [ ] **Step 5: Wire the WebGPU shell to the reference slot**

In `crates/glgpu-web/src/lib.rs` inside `mod app`:

1. Add fields to `DemoState`: `compare_slot: HtmlElement`, `compare_status: HtmlElement`, `viewport_badge: HtmlElement`, `compile_button: HtmlButtonElement`, `run_button: HtmlButtonElement`, `reference_button: HtmlButtonElement`, `native_button: HtmlButtonElement`.
2. Give the existing badge `viewport_badge.set_id("viewport-badge")`.
3. Replace the canvas that is appended directly to `viewport_panel` with an empty slot:

```rust
let compare_slot = element(&document, "div", "compare-slot")?;
compare_slot.set_id("compare-slot");
let compare_status = element(&document, "p", "compare-status")?;
compare_status.set_id("compare-status");
compare_status.set_text_content(Some("正在准备固定对照场景"));
let compare_note = element(&document, "p", "compare-note")?;
compare_note.set_id("compare-note");
compare_note.set_text_content(Some(
    "原生对照只用于观看画面和本地像素比较。它不运行编辑器里的调试流程，也不改变 CpuGpuSubset — NOT WEBGPU CONFORMANT。",
));
let toggle = element(&document, "div", "compare-toggle")?;
toggle.set_attribute("role", "group")?;
toggle.set_attribute("aria-label", "对照后端")?;
let reference_button = button(&document, "button button-secondary", "参考实现")?;
reference_button.set_attribute("type", "button")?;
reference_button.set_attribute("aria-pressed", "true")?;
let native_button = button(&document, "button button-secondary", "原生")?;
native_button.set_attribute("type", "button")?;
native_button.set_attribute("aria-pressed", "false")?;
append(&toggle, &reference_button)?;
append(&toggle, &native_button)?;
append(&viewport_panel, &viewport_header)?;
append(&viewport_panel, &toggle)?;
append(&viewport_panel, &compare_note)?;
append(&viewport_panel, &compare_slot)?;
append(&viewport_panel, &compare_status)?;
append(&viewport_panel, &metrics)?;
```

Do not call `getContext("2d")` in `boot`. Delete the old canvas creation block that did.

4. Add this CSS next to the existing `.viewport` rule:

```rust
#compare-slot { width: 320px; height: 240px; }
#compare-slot[data-compare-state="editor"] { width: 720px; height: 480px; }
#compare-slot .viewport { width: 320px; height: 240px; max-width: none; }
#compare-slot[data-compare-state="editor"] .viewport { width: 720px; height: 480px; }
```

5. Add these methods on `DemoState`:

```rust
fn call_window1(&self, name: &str, arg: &JsValue) -> Result<JsValue, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("window is unavailable"))?;
    let function = js_sys::Reflect::get(&window, &JsValue::from_str(name))?;
    let function: js_sys::Function = function.dyn_into()?;
    function.call1(&JsValue::UNDEFINED, arg)
}

fn mark_pending(&self, state: &str) -> Result<(), JsValue> {
    self.compare_slot.set_attribute("data-compare-state", state)?;
    self.compare_slot.remove_attribute("data-compare-ready")?;
    self.compare_slot.remove_attribute("data-compare-error")?;
    Ok(())
}

fn set_pressed(&self, native: bool) -> Result<(), JsValue> {
    self.reference_button
        .set_attribute("aria-pressed", if native { "false" } else { "true" })?;
    self.native_button
        .set_attribute("aria-pressed", if native { "true" } else { "false" })?;
    self.editor.set_read_only(native);
    self.compile_button.set_disabled(native);
    self.run_button.set_disabled(native);
    Ok(())
}

fn show_reference_compare(&self) -> Result<(), JsValue> {
    self.set_pressed(false)?;
    self.mark_pending("reference")?;
    self.call_window1("glgpuPresentCompare", &JsValue::from_str("reference"))?;
    self.metrics
        .set_text_content(Some("固定对照场景 · RGBA8 · 320×240"));
    Ok(())
}

fn show_native_compare(&self) -> Result<(), JsValue> {
    self.set_pressed(true)?;
    self.mark_pending("native")?;
    self.call_window1("glgpuPresentCompare", &JsValue::from_str("native"))?;
    Ok(())
}

fn present_editor_pixels(&self, pixels: &[u8]) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("window is unavailable"))?;
    let function: js_sys::Function =
        js_sys::Reflect::get(&window, &JsValue::from_str("glgpuPresentEditorFrame"))?.dyn_into()?;
    let bytes = js_sys::Uint8Array::from(pixels);
    function.call3(
        &JsValue::UNDEFINED,
        &JsValue::from(WIDTH),
        &JsValue::from(HEIGHT),
        &bytes,
    )?;
    self.set_pressed(false)?;
    Ok(())
}
```

6. Change `render` so it no longer calls `present_rgba8` on a stored 2D context. After `readback_rgba8`, call `self.present_editor_pixels(&pixels)?`. Keep the metrics hash line for this editor submit. This is the only present path for 「运行」. Do not call `compare_frame_rgba8` here. Delete `use glgpu_canvas2d::present_rgba8;` once nothing in the crate references it.
7. At the start of `compile_editor`, add:

```rust
if self.editor.read_only() {
    return Ok(false);
}
```

8. Replace `state.borrow_mut().compile_editor(true)?;` in `boot` with:

```rust
state.borrow_mut().compile_editor(false)?;
state.borrow_mut().show_reference_compare()?;
```

9. Attach click listeners to `reference_button` and `native_button` that call `show_reference_compare` and `show_native_compare`. Attach the canvas click listener to `compare_slot`, not to a canvas. In that listener:

- If `data-compare-state` is `native`, return without calling `render`.
- If it is `editor`, keep the current `render(PixelCoord)` behavior.
- If it is `reference`, call `glgpuReadCanvasPixel` and write `pixel (x, y)\nrgba(r, g, b, a)\n固定对照场景没有 primitive history` into `pixel_history`. Do not call `render`.

Map `offset_x` / `offset_y` with the clicked canvas's `width()` and `height()`, not the constant `WIDTH`/`HEIGHT`, and only when the event target is an `HtmlCanvasElement`.

10. Remove `context: CanvasRenderingContext2d` from `DemoState` if nothing reads it after the present change. Keep the `CpuDevice` at `Extent2d::new(WIDTH, HEIGHT)` for the editor preview.

In `web/index.html`, inside the module script, add the imports and assign the window functions after the Wasm namespace exists and before `wasm.boot(...)`:

```javascript
import {
  presentEditorFrame,
  presentReference,
  readCanvasPixel,
} from "./compare-present.js";
```

The import must sit with the other module imports at the top of the module script. `compare-scene.js` is imported by `compare-present.js`, and the page module must also import it directly:

```javascript
import "./compare-scene.js";
```

After `await wasm.default()` and before every `wasm.boot(...)` call:

```javascript
window.glgpuPresentCompare = (mode) => {
  const slot = document.getElementById("compare-slot");
  if (mode === "reference") {
    presentReference(slot, wasm.compare_frame_rgba8);
    return;
  }
  throw new Error(`glgpuPresentCompare cannot present ${mode} yet`);
};
window.glgpuPresentEditorFrame = (width, height, pixels) => {
  presentEditorFrame(document.getElementById("compare-slot"), width, height, pixels);
};
window.glgpuReadCanvasPixel = (x, y) => readCanvasPixel(x, y);
```

Apply that assignment in both the ST and MT boot paths before `boot`. The native branch is completed in Task 5. Until then, clicking 「原生」 throws; the reference load path does not use that branch.

- [ ] **Step 6: Wire the WebGL shell the same way**

Repeat the `DemoState` field, slot, toggle, note, CSS, `show_reference_compare`, `show_native_compare`, `present_editor_pixels`, `read_only` guard, and click behavior in `crates/glgpu-webgl-web/src/lib.rs`. Differences:

- The note text is `COMPARE_NOTE_WEBGL` from Task 1, ending in `CpuWebGl2LearningTrack — NOT WEBGL CONFORMANT`.
- Editor present still uses `PREVIEW_WIDTH` and `PREVIEW_HEIGHT` (720 and 480) and still updates the metrics line from `self.context.stable_hash64()` inside `present`. Change `present` to pass `self.context.to_rgba8_bytes()` to `glgpuPresentEditorFrame` instead of `present_rgba8` on `self.canvas_context`. Delete the local `fn present_rgba8` once nothing calls it.
- Boot currently calls `compile_editor(true)`, which would paint the 720×480 preview. Change it to `compile_editor(false)` and then `show_reference_compare()`. If compile returns `Ok(false)`, still call `show_reference_compare()`.
- Editor-state clicks keep `pixel_coord_from_click` and `show_selected_pixel` against the `LearningContext`. Reference-state clicks use `glgpuReadCanvasPixel` and must not read the 720×480 context. Native-state clicks return immediately.
- Add the same `#compare-slot` rules to `crates/glgpu-webgl-web/src/debug.css`.

In `web/webgl.html`, add the same imports and the same three `window` assignments before `wasm.boot()`. The page imports `./compare-scene.js` directly. Its `glgpuPresentCompare("native")` branch throws the same `cannot present native yet` error until Task 6.

- [ ] **Step 7: Run the host tests**

Run: `cargo test -p glgpu-web -p glgpu-webgl-web -- --test-threads=1`

Expected: PASS. The new compare-frame tests still pass. No Wasm browser test is required in this task.

Run: `node --test web/compare-scene.test.mjs`

Expected: PASS, 3 tests.

- [ ] **Step 8: Commit**

```bash
git add web/compare-present.js web/compare-scene.test.mjs web/index.html web/webgl.html crates/glgpu-web/src/lib.rs crates/glgpu-webgl-web/src/lib.rs crates/glgpu-webgl-web/src/debug.css
git commit -m "Show the CPU compare scene in the shared viewport slot."
```

## Task 5: Native WebGPU presenter

**Files:**
- Create: `web/native-webgpu.js`
- Create: `web/native-webgpu.test.mjs`
- Modify: `web/index.html`
- Modify: `xtask/src/main.rs` (`pages_loader_stays_relative_and_serve_headers_stay_real`)
- Test: `web/native-webgpu.test.mjs`

**Interfaces:**
- Consumes: `COMPARE_VERTICES`, `COMPARE_CLEAR`, `COMPARE_WIDTH`, `COMPARE_HEIGHT`, `COMPARE_ERROR_WEBGPU`, `COMPARE_STATUS_NATIVE`, `mountCanvas`, `setComparePending`, `setCompareReady`, `setCompareError`.
- Produces: `triangleVertexData()`, `webgpuShaderSource()`, `presentNativeWebgpu(slot)`.

- [ ] **Step 1: Write the failing test**

Create `web/native-webgpu.test.mjs`:

```javascript
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
  assert.equal(floats[7], COMPARE_VERTICES[1].ndcY);
  assert.equal(floats[14], COMPARE_VERTICES[2].g);
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
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `node --test web/native-webgpu.test.mjs`

Expected: FAIL with `ERR_MODULE_NOT_FOUND`.

- [ ] **Step 3: Write the WebGPU module**

Create `web/native-webgpu.js`:

```javascript
import {
  COMPARE_CLEAR,
  COMPARE_ERROR_WEBGPU,
  COMPARE_HEIGHT,
  COMPARE_STATUS_NATIVE,
  COMPARE_VERTICES,
  COMPARE_WIDTH,
} from "./compare-scene.js";
import { mountCanvas, setCompareError, setComparePending, setCompareReady } from "./compare-present.js";

export const WEBGPU_FLIP_Y = false;

export function triangleVertexData() {
  const data = new Float32Array(COMPARE_VERTICES.length * 6);
  COMPARE_VERTICES.forEach((vertex, index) => {
    const offset = index * 6;
    data[offset] = vertex.ndcX;
    data[offset + 1] = WEBGPU_FLIP_Y ? -vertex.ndcY : vertex.ndcY;
    data[offset + 2] = vertex.r;
    data[offset + 3] = vertex.g;
    data[offset + 4] = vertex.b;
    data[offset + 5] = vertex.a;
  });
  return data;
}

export function webgpuShaderSource() {
  const clipZ = COMPARE_VERTICES[0].clipZ;
  const clipW = COMPARE_VERTICES[0].clipW;
  return `
struct VertexOutput {
  @builtin(position) position: vec4f,
  @location(0) color: vec4f,
};
@vertex
fn vs_main(@location(0) ndc: vec2f, @location(1) color: vec4f) -> VertexOutput {
  var output: VertexOutput;
  output.position = vec4f(ndc.x, ndc.y, ${clipZ}, ${clipW});
  output.color = color;
  return output;
}
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {
  return input.color;
}
`;
}

export async function presentNativeWebgpu(slot) {
  setComparePending(slot, "native");
  try {
    if (!navigator.gpu) {
      throw new Error(COMPARE_ERROR_WEBGPU);
    }
    const adapter = await navigator.gpu.requestAdapter({ powerPreference: "high-performance" });
    if (!adapter) {
      throw new Error(COMPARE_ERROR_WEBGPU);
    }
    const device = await adapter.requestDevice();
    const format = navigator.gpu.getPreferredCanvasFormat();
    if (format !== "rgba8unorm" && format !== "bgra8unorm") {
      throw new Error(COMPARE_ERROR_WEBGPU);
    }
    const canvas = mountCanvas(slot, COMPARE_WIDTH, COMPARE_HEIGHT);
    const context = canvas.getContext("webgpu");
    if (!context) {
      throw new Error(COMPARE_ERROR_WEBGPU);
    }
    context.configure({ device, format, alphaMode: "opaque" });
    const shader = device.createShaderModule({ code: webgpuShaderSource() });
    const pipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: {
        module: shader,
        entryPoint: "vs_main",
        buffers: [
          {
            arrayStride: 24,
            attributes: [
              { shaderLocation: 0, offset: 0, format: "float32x2" },
              { shaderLocation: 1, offset: 8, format: "float32x4" },
            ],
          },
        ],
      },
      fragment: {
        module: shader,
        entryPoint: "fs_main",
        targets: [{ format }],
      },
      primitive: { topology: "triangle-list", cullMode: "none" },
      multisample: { count: 1 },
    });
    const vertices = triangleVertexData();
    const buffer = device.createBuffer({
      size: vertices.byteLength,
      usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
    });
    device.queue.writeBuffer(buffer, 0, vertices);
    const encoder = device.createCommandEncoder();
    const pass = encoder.beginRenderPass({
      colorAttachments: [
        {
          view: context.getCurrentTexture().createView(),
          clearValue: {
            r: COMPARE_CLEAR[0] / 255,
            g: COMPARE_CLEAR[1] / 255,
            b: COMPARE_CLEAR[2] / 255,
            a: COMPARE_CLEAR[3] / 255,
          },
          loadOp: "clear",
          storeOp: "store",
        },
      ],
    });
    pass.setPipeline(pipeline);
    pass.setVertexBuffer(0, buffer);
    pass.draw(3);
    pass.end();
    device.queue.submit([encoder.finish()]);
    await device.queue.onSubmittedWorkDone();
    document.getElementById("compare-status").textContent = COMPARE_STATUS_NATIVE;
    document.getElementById("viewport-badge").textContent = "RGBA8 · 320×240";
    setCompareReady(slot, "native");
  } catch (error) {
    const message = String(error && error.message ? error.message : error);
    setCompareError(
      slot,
      "native",
      message.includes(COMPARE_ERROR_WEBGPU) ? COMPARE_ERROR_WEBGPU : message,
    );
  }
}
```

`WEBGPU_FLIP_Y` stays `false` unless Task 7 shows the triangle upside down. If that happens, change only this constant to `true` and rerun Task 7. Do not flip the CPU frame. There is no `requestAnimationFrame` and no MSAA resolve target. The shader does not sample a CPU image.

- [ ] **Step 4: Point index.html at the native presenter**

In `web/index.html`, import `presentNativeWebgpu` from `./native-webgpu.js` and replace `window.glgpuPresentCompare` with:

```javascript
window.glgpuPresentCompare = (mode) => {
  const slot = document.getElementById("compare-slot");
  if (mode === "reference") {
    presentReference(slot, wasm.compare_frame_rgba8);
    return;
  }
  if (mode === "native") {
    return presentNativeWebgpu(slot);
  }
  throw new Error(`unknown compare mode ${mode}`);
};
```

Do not import `./native-webgl.js` from this file.

In `pages_loader_stays_relative_and_serve_headers_stay_real`, add:

```rust
assert!(index.contains("\"./compare-scene.js\""));
assert!(index.contains("\"./compare-present.js\""));
assert!(index.contains("\"./native-webgpu.js\""));
assert!(!index.contains("native-webgl.js"));
```

- [ ] **Step 5: Run the tests**

Run: `node --test web/native-webgpu.test.mjs web/compare-scene.test.mjs`

Expected: PASS.

Run: `cargo test -p xtask pages_loader_stays_relative_and_serve_headers_stay_real -- --test-threads=1`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add web/native-webgpu.js web/native-webgpu.test.mjs web/index.html xtask/src/main.rs
git commit -m "Draw the compare scene with native WebGPU."
```

## Task 6: Native WebGL2 presenter

**Files:**
- Create: `web/native-webgl.js`
- Create: `web/native-webgl.test.mjs`
- Modify: `web/webgl.html`
- Modify: `xtask/src/main.rs`
- Test: `web/native-webgl.test.mjs`

**Interfaces:**
- Consumes: the same scene and present helpers as Task 5, plus `COMPARE_ERROR_WEBGL`.
- Produces: `WEBGL2_CONTEXT_ATTRIBUTES`, `triangleVertexData()`, `webglShaderSource()`, `presentNativeWebgl(slot)`.

- [ ] **Step 1: Write the failing test**

Create `web/native-webgl.test.mjs`:

```javascript
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
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `node --test web/native-webgl.test.mjs`

Expected: FAIL with `ERR_MODULE_NOT_FOUND`.

- [ ] **Step 3: Write the WebGL2 module**

Create `web/native-webgl.js`:

```javascript
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
```

`WEBGL_FLIP_Y` stays `false` unless Task 7 shows this page upside down. Then change only that constant. This shader is browser ESSL, not the learning subset. Do not read the editor textarea.

- [ ] **Step 4: Point webgl.html at the native presenter**

In `web/webgl.html`, import `presentNativeWebgl` from `./native-webgl.js` and replace `window.glgpuPresentCompare` with the reference/native split from Task 5, calling `presentNativeWebgl(slot)` for `"native"`. Do not import `./native-webgpu.js`.

Update the xtask loader test:

```rust
assert!(webgl.contains("\"./compare-scene.js\""));
assert!(webgl.contains("\"./compare-present.js\""));
assert!(webgl.contains("\"./native-webgl.js\""));
assert!(!webgl.contains("native-webgpu.js"));
assert!(!index.contains("native-webgl.js"));
```

Keep the Task 5 assertions that `index.html` contains `./native-webgpu.js`.

- [ ] **Step 5: Run the tests**

Run: `node --test web/native-webgl.test.mjs web/native-webgpu.test.mjs web/compare-scene.test.mjs`

Expected: PASS.

Run: `cargo test -p xtask pages_loader_stays_relative_and_serve_headers_stay_real -- --test-threads=1`

Expected: PASS.

Run: `rg -n "WebGl2RenderingContext|GpuCanvasContext|GpuDevice" crates/glgpu-web crates/glgpu-webgl crates/glgpu-webgl-web crates/glgpu-webgpu crates/glgpu-raster`

Expected: no matches. Exit code 1 from `rg` means no matches; that is the required result.

- [ ] **Step 6: Commit**

```bash
git add web/native-webgl.js web/native-webgl.test.mjs web/webgl.html xtask/src/main.rs
git commit -m "Draw the compare scene with native WebGL2."
```

## Task 7: Playwright exact compare

**Files:**
- Create: `e2e/package.json`
- Create: `e2e/package-lock.json`
- Create: `e2e/playwright.config.mjs`
- Create: `e2e/compare.spec.mjs`
- Modify: `.gitignore`
- Test: `e2e/compare.spec.mjs`

**Interfaces:**
- Consumes: `#compare-slot` attributes `data-compare-state`, `data-compare-ready`, `data-compare-error`; buttons named `参考实现` and `原生`; Task 1 error sentences; scene width and height.
- Produces: pixel artifacts under `target/e2e-compare/webgpu/` and `target/e2e-compare/webgl/`. The test fails the Playwright process when either page differs or reports an error.

- [ ] **Step 1: Add the package and gitignore**

Create `e2e/package.json`:

```json
{
  "name": "glgpu-e2e",
  "private": true,
  "type": "module",
  "scripts": {
    "test": "playwright test --config playwright.config.mjs"
  },
  "devDependencies": {
    "playwright": "1.64.1",
    "pngjs": "7.0.0"
  }
}
```

Append this line to `.gitignore`:

```text
e2e/node_modules/
```

Create `e2e/playwright.config.mjs`:

```javascript
import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: ".",
  timeout: 120_000,
  workers: 1,
  reporter: "line",
  use: {
    baseURL: process.env.GLGPU_E2E_BASE,
    headless: true,
    viewport: { width: 1280, height: 800 },
    deviceScaleFactor: 1,
    channel: "chrome",
    launchOptions: {
      args: ["--enable-unsafe-webgpu"],
    },
  },
});
```

Do not add SwiftShader, `--use-gl=swiftshader`, or `--enable-unsafe-swiftshader`.

Run:

```bash
npm install --save-exact --package-lock-only --prefix e2e
```

Expected: `e2e/package-lock.json` exists and pins `playwright` at `1.64.1` and `pngjs` at `7.0.0`.

Then install the modules for the local test:

```bash
npm ci --prefix e2e
```

Expected: exit 0. `e2e/node_modules/` is gitignored.

- [ ] **Step 2: Write the spec**

Create `e2e/compare.spec.mjs`:

```javascript
import fs from "node:fs";
import path from "node:path";
import { test, expect } from "@playwright/test";
import { PNG } from "pngjs";
import {
  COMPARE_ERROR_WEBGL,
  COMPARE_ERROR_WEBGPU,
  COMPARE_HEIGHT,
  COMPARE_WIDTH,
} from "../web/compare-scene.js";

const BYTE_LENGTH = COMPARE_WIDTH * COMPARE_HEIGHT * 4;

function writePng(file, rgba) {
  const png = new PNG({ width: COMPARE_WIDTH, height: COMPARE_HEIGHT });
  png.data.set(rgba);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, PNG.sync.write(png));
}

function writeArtifacts(name, reference, native) {
  const directory = path.resolve("target/e2e-compare", name);
  fs.mkdirSync(directory, { recursive: true });
  writePng(path.join(directory, "reference.png"), reference);
  writePng(path.join(directory, "native.png"), native);
  const diff = Buffer.alloc(BYTE_LENGTH);
  let mismatches = 0;
  let first = null;
  for (let index = 0; index < BYTE_LENGTH; index += 4) {
    const same =
      reference[index] === native[index] &&
      reference[index + 1] === native[index + 1] &&
      reference[index + 2] === native[index + 2] &&
      reference[index + 3] === native[index + 3];
    if (same) {
      diff.set(reference.subarray(index, index + 4), index);
    } else {
      diff.set([255, 0, 255, 255], index);
      mismatches += 1;
      if (!first) {
        const pixel = index / 4;
        const x = pixel % COMPARE_WIDTH;
        const y = Math.floor(pixel / COMPARE_WIDTH);
        first = `first: x=${x} y=${y} reference=${reference[index]},${reference[index + 1]},${reference[index + 2]},${reference[index + 3]} native=${native[index]},${native[index + 1]},${native[index + 2]},${native[index + 3]}`;
      }
    }
  }
  writePng(path.join(directory, "diff.png"), diff);
  fs.writeFileSync(
    path.join(directory, "mismatch.txt"),
    `mismatch count: ${mismatches}\n${first ?? "first: none"}\n`,
  );
  return mismatches;
}

async function readDisplayed(page, state) {
  return page.evaluate(async ({ state, width, height }) => {
    const slot = document.querySelector("#compare-slot");
    const source = slot.querySelector("canvas");
    if (source.width !== width || source.height !== height) {
      throw new Error(`canvas buffer is ${source.width}x${source.height}`);
    }
    const read = document.createElement("canvas");
    read.width = width;
    read.height = height;
    const context = read.getContext("2d", { willReadFrequently: true });
    context.imageSmoothingEnabled = false;
    context.drawImage(source, 0, 0);
    const drawn = Array.from(context.getImageData(0, 0, width, height).data);
    let direct = null;
    if (state === "reference") {
      const existing = source.getContext("2d");
      direct = Array.from(existing.getImageData(0, 0, width, height).data);
    }
    return { drawn, direct };
  }, { state, width: COMPARE_WIDTH, height: COMPARE_HEIGHT });
}

async function waitReady(page, state) {
  await page.waitForFunction(
    (expected) => {
      const slot = document.querySelector("#compare-slot");
      const ready = slot?.getAttribute("data-compare-ready");
      return slot?.getAttribute("data-compare-state") === expected && (ready === "true" || ready === "error");
    },
    state,
    { timeout: 30_000 },
  );
  const error = await page.locator("#compare-slot").getAttribute("data-compare-error");
  const ready = await page.locator("#compare-slot").getAttribute("data-compare-ready");
  if (ready === "error") {
    throw new Error(error || `${state} compare failed`);
  }
}

async function capturePage(page, pathname) {
  const errors = [];
  const onError = (error) => errors.push(String(error));
  page.on("pageerror", onError);
  await page.goto(pathname);
  expect(page.url().includes("threads=")).toBe(false);
  await waitReady(page, "reference");
  const referenceRead = await readDisplayed(page, "reference");
  expect(referenceRead.drawn.length).toBe(BYTE_LENGTH);
  expect(referenceRead.direct).toEqual(referenceRead.drawn);
  await page.getByRole("button", { name: "原生" }).click();
  await waitReady(page, "native");
  const nativeRead = await readDisplayed(page, "native");
  expect(nativeRead.drawn.length).toBe(BYTE_LENGTH);
  expect(nativeRead.direct).toBe(null);
  page.off("pageerror", onError);
  return {
    reference: Uint8Array.from(referenceRead.drawn),
    native: Uint8Array.from(nativeRead.drawn),
    errors,
  };
}

test("both pages match native RGBA exactly", async ({ page }) => {
  const failures = [];
  const pages = [
    { name: "webgpu", pathname: "/", nativeError: COMPARE_ERROR_WEBGPU },
    { name: "webgl", pathname: "/webgl.html", nativeError: COMPARE_ERROR_WEBGL },
  ];
  for (const entry of pages) {
    try {
      const captured = await capturePage(page, entry.pathname);
      const mismatches = writeArtifacts(entry.name, captured.reference, captured.native);
      if (mismatches !== 0) {
        failures.push(`${entry.name} has ${mismatches} mismatched pixels`);
      }
      if (captured.errors.length !== 0) {
        failures.push(`${entry.name} page errors: ${captured.errors.join("\n")}`);
      }
    } catch (error) {
      const message = String(error);
      failures.push(
        message.includes(entry.nativeError) ? `${entry.name}: ${entry.nativeError}` : `${entry.name}: ${message}`,
      );
    }
  }
  expect(failures, failures.join("\n")).toEqual([]);
});
```

`capturePage` takes `page` and `pathname`. `waitReady` throws the slot's `data-compare-error` unchanged. When that text contains `entry.nativeError`, the failure list stores that exact Task 1 sentence. The reference read calls `getContext("2d")` only when `state === "reference"`. The native read does not. Comparison is `mismatches !== 0` with no epsilon. One page's failure does not return before the other page is captured.

- [ ] **Step 3: Confirm the test file parses**

Run: `node --check e2e/compare.spec.mjs`

Expected: exit 0.

Do not start Chrome in this task. Task 8 is what launches the browser. A missing Chrome binary is not a reason to add `test.skip`.

- [ ] **Step 4: Commit**

```bash
git add e2e/package.json e2e/package-lock.json e2e/playwright.config.mjs e2e/compare.spec.mjs .gitignore
git commit -m "Add the local exact-RGBA Playwright compare."
```

## Task 8: `xtask e2e-compare`

**Files:**
- Modify: `xtask/src/main.rs`
- Test: `xtask/src/main.rs` tests module

**Interfaces:**
- Consumes: existing `serve`, `build_web`, and `parse_build_web_options` behavior. The runner shells out to `cargo xtask` rather than reimplementing Wasm builds.
- Produces: `parse_e2e_compare_options` and command `e2e-compare`. Default bind `127.0.0.1:8765`. Optional `--bind <address>` only.

- [ ] **Step 1: Write the failing parser tests**

Add these tests inside `xtask/src/main.rs` `mod tests`:

```rust
#[test]
fn e2e_compare_defaults_the_bind_address() {
    let options = parse_e2e_compare_options(std::iter::empty::<OsString>()).unwrap();
    assert_eq!(options.bind, "127.0.0.1:8765");
}

#[test]
fn e2e_compare_accepts_bind_and_rejects_other_arguments() {
    let options = parse_e2e_compare_options([
        OsString::from("--bind"),
        OsString::from("127.0.0.1:9"),
    ])
    .unwrap();
    assert_eq!(options.bind, "127.0.0.1:9");

    let error = parse_e2e_compare_options([OsString::from("--release")]).unwrap_err();
    assert!(error.to_string().contains("unknown e2e-compare argument"));

    let error = run([OsString::from("e2e-compare"), OsString::from("--gpu")]).unwrap_err();
    assert!(error.to_string().contains("unknown e2e-compare argument"));
}

#[test]
fn usage_lists_e2e_compare() {
    assert!(usage().contains("xtask e2e-compare [--bind <address>]"));
}
```

Do not call `e2e_compare` or `run` with only `e2e-compare`. That would build Wasm and launch Chrome.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p xtask e2e_compare -- --test-threads=1`

Expected: FAIL to compile, `cannot find function parse_e2e_compare_options`.

- [ ] **Step 3: Implement the command**

Change `usage()` to:

```rust
fn usage() -> &'static str {
    "Usage:\n  xtask serve [--dir <directory>] [--bind <address>]\n  xtask build-web [--crate <wasm-crate>] [--out-dir <directory>] [--out-name <name>] [--dev|--release] [-- <extra cargo build args>]\n  xtask build-web-mt\n  xtask e2e-compare [--bind <address>]"
}
```

In `run`, before the `help` arm:

```rust
"e2e-compare" => e2e_compare(parse_e2e_compare_options(args)?),
```

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
struct E2eCompareOptions {
    bind: String,
}

fn parse_e2e_compare_options(
    args: impl IntoIterator<Item = std::ffi::OsString>,
) -> Result<E2eCompareOptions, TaskError> {
    let mut bind = "127.0.0.1:8765".to_owned();
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.to_string_lossy().as_ref() {
            "--bind" => bind = required_value("--bind", args.next())?,
            other => {
                return Err(TaskError::new(format!(
                    "unknown e2e-compare argument {other:?}\n\n{}",
                    usage()
                )));
            }
        }
    }
    Ok(E2eCompareOptions { bind })
}

const COMPARE_ERROR_CHROME: &str =
    "原生对照需要本机已安装的 Google Chrome。Playwright 的 chrome 通道启动失败，命令失败。";

struct KillOnDrop(std::process::Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn e2e_compare(options: E2eCompareOptions) -> Result<(), Box<dyn Error>> {
    let workspace = workspace_root();
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let status = Command::new(&cargo)
        .current_dir(&workspace)
        .args(["xtask", "build-web", "--release"])
        .status()?;
    if !status.success() {
        return Err(TaskError::new("build-web --release failed").into());
    }
    let status = Command::new(&cargo)
        .current_dir(&workspace)
        .args([
            "xtask",
            "build-web",
            "--release",
            "--crate",
            "crates/glgpu-webgl-web",
            "--out-dir",
            "web/pkg-webgl",
            "--out-name",
            "glgpu_webgl_web",
        ])
        .status()?;
    if !status.success() {
        return Err(TaskError::new("webgl wasm build failed").into());
    }

    let modules = workspace.join("e2e/node_modules");
    let lock = workspace.join("e2e/package-lock.json");
    let needs_install = match (fs::metadata(&modules), fs::metadata(&lock)) {
        (Ok(modules_meta), Ok(lock_meta)) => {
            !modules_meta.is_dir() || modules_meta.modified()? < lock_meta.modified()?
        }
        _ => true,
    };
    if needs_install {
        let status = Command::new("npm")
            .current_dir(workspace.join("e2e"))
            .args(["ci"])
            .status()?;
        if !status.success() {
            return Err(TaskError::new("npm ci in e2e/ failed").into());
        }
    }

    let _server = KillOnDrop(
        Command::new(&cargo)
            .current_dir(&workspace)
            .args(["xtask", "serve", "--bind", &options.bind])
            .spawn()?,
    );
    wait_for_http_ok(&options.bind, Duration::from_secs(30))?;

    let playwright_name = if cfg!(windows) {
        "playwright.cmd"
    } else {
        "playwright"
    };
    let playwright = workspace.join("e2e/node_modules/.bin").join(playwright_name);
    let output = Command::new(playwright)
        .current_dir(&workspace)
        .env("GLGPU_E2E_BASE", format!("http://{}", options.bind))
        .args(["test", "--config", "e2e/playwright.config.mjs"])
        .output()?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    eprint!("{combined}");
    if !output.status.success() {
        let mentions_sentence = combined.contains(COMPARE_ERROR_CHROME)
            || combined.contains("当前环境没有可用的 WebGPU")
            || combined.contains("当前环境不能创建 WebGL2");
        if !mentions_sentence
            && (combined.contains("browserType.launch")
                || combined.contains("Failed to launch")
                || combined.contains("Chromium distribution"))
        {
            eprintln!("{COMPARE_ERROR_CHROME}");
        }
        return Err(TaskError::new("e2e-compare failed").into());
    }
    Ok(())
}

fn wait_for_http_ok(bind: &str, timeout: Duration) -> Result<(), TaskError> {
    let started = std::time::Instant::now();
    let request = format!("GET / HTTP/1.1\r\nHost: {bind}\r\nConnection: close\r\n\r\n");
    loop {
        if let Ok(mut stream) = std::net::TcpStream::connect(bind) {
            stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
            if stream.write_all(request.as_bytes()).is_ok() {
                let mut buffer = String::new();
                if stream.read_to_string(&mut buffer).is_ok() && buffer.contains("200") {
                    return Ok(());
                }
            }
        }
        if started.elapsed() > timeout {
            return Err(TaskError::new(format!(
                "timed out waiting for http://{bind}/"
            )));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
```

`Drop` for `KillOnDrop` runs when `e2e_compare` returns, including when Playwright or the HTTP wait fails, so the serve child is killed on both success and failure. Do not call `build-web-mt`. Do not add a unit test that invokes `e2e_compare`.

- [ ] **Step 4: Run the xtask tests**

Run: `cargo test -p xtask -- --test-threads=1`

Expected: PASS, including `e2e_compare_defaults_the_bind_address`, `e2e_compare_accepts_bind_and_rejects_other_arguments`, and `usage_lists_e2e_compare`. The test log must not contain `browserType.launch` or `npm ci`.

- [ ] **Step 5: Run the full local command when Chrome is available**

Run: `cargo xtask e2e-compare`

Expected on a machine whose Google Chrome can create WebGPU and WebGL2: exit 0. `writeArtifacts` still runs, and both `mismatch.txt` files contain `mismatch count: 0`.

Expected when `channel: "chrome"` cannot launch: non-zero exit, and stderr contains `原生对照需要本机已安装的 Google Chrome。Playwright 的 chrome 通道启动失败，命令失败。`

Expected when Chrome launches but `navigator.gpu` or WebGL2 is missing: non-zero exit, and the output contains the matching WebGPU or WebGL sentence from Task 1. The other page is still captured.

If bytes differ, the command fails and `target/e2e-compare/<webgpu|webgl>/{reference,native,diff}.png` plus `mismatch.txt` exist. Repair only by spec section 4.2: context flags, Y flag in the native module, or an NDC edit that updates spec section 5.2 and `web/compare-scene.js` together. Do not edit the comparator.

- [ ] **Step 6: Commit**

```bash
git add xtask/src/main.rs
git commit -m "Add the local e2e-compare xtask command."
```

## Task 9: Acceptance doc

**Files:**
- Modify: `docs/testing-and-acceptance.zh-CN.md`
- Test: `rg` checks below

**Interfaces:**
- Consumes: the command, paths, and sentences already implemented.
- Produces: section `### 7.5 本地原生像素对照` and corrected first-frame wording in sections 7.2 and 7.3.

- [ ] **Step 1: Update the hash wording**

In section 7.2 step 3, replace:

```text
记录 capability 行中的 `build=ST` 和首帧 metrics hash；
```

with:

```text
记录 capability 行中的 `build=ST`。加载后的第一帧是 320×240 固定对照场景，不是 720×480 hash。720×480 hash 在点击「运行」之后的编辑器预览里读取；
```

In section 7.3, change the table row label `首帧 hash` to `编辑器预览 hash`. Replace the paragraph that starts `该 hash 是当前 720×480 P0 triangle scene 的证据快照。` with:

```text
该 hash 是编辑器预览的 720×480 P0 triangle scene 的证据快照，不是页面加载后的第一帧。加载后的第一帧是 320×240 固定对照场景。实现或场景有意改变后可以更新 hash，但同一个候选版本的 ST/MT 仍必须完全一致。
```

Leave the hex value `2898a668a856be56` in place. Do not change L1–L8 pass conditions.

- [ ] **Step 2: Add the local compare section**

Insert this after section 7.4:

```markdown
### 7.5 本地原生像素对照

`cargo xtask e2e-compare` 只在本地运行。它构建 ST `web/pkg` 和 `web/pkg-webgl`，启动 `serve`，用本机 Google Chrome 打开 `/` 与 `/webgl.html`，并要求每一页的参考画布与原生画布逐字节相等。比较的是 320×240 RGBA8，共 307200 字节，无容差。

缺少 Google Chrome、`navigator.gpu` 或 WebGL2 时命令失败，不是 skip。这条门不替代 `stable_hash64`，也不进入 GitHub Actions。`2898a668a856be56` 仍是编辑器预览的 720×480 ST/MT 证据。加载后的第一帧是 320×240 固定对照场景，角标是 `RGBA8 · 320×240`。点击「运行」后的编辑器预览角标仍是 `RGBA8 · 720×480`。

失败产物写在 `target/e2e-compare/webgpu/` 和 `target/e2e-compare/webgl/`：`reference.png`、`native.png`、`diff.png`、`mismatch.txt`。
```

- [ ] **Step 3: Check the doc and the workflow diff**

Run: `rg -n "本地原生像素对照|无容差|不是 skip|320×240" docs/testing-and-acceptance.zh-CN.md`

Expected: matches in section 7.5 and the updated 7.2/7.3 sentences.

Run: `git diff --name-only -- .github/workflows/pages.yml`

Expected: no output.

- [ ] **Step 4: Commit**

```bash
git add docs/testing-and-acceptance.zh-CN.md
git commit -m "Document the local native pixel compare."
```

## Spec coverage

| Spec requirement | Task |
| --- | --- |
| 320×240 exact RGBA, no tolerance | Tasks 2, 3, 7, 8 |
| Shared scene constants, one JS source | Tasks 1, 2, 3 |
| Same slot, default reference, not side-by-side | Task 4 |
| Both `index.html` WebGPU and `webgl.html` WebGL2 | Tasks 5, 6 |
| Native JS stays out of CPU crates | Tasks 5, 6 grep step |
| Editor run stays 720×480 and does not call `compare_frame_rgba8` | Task 4 |
| Native mode makes the editor read-only and disables 编译 / 运行 | Task 4 |
| Reference click reads the CPU frame; native click does not trace | Task 4 |
| WebGL2 context flags, no MSAA, one frame, DPR not applied | Tasks 5, 6, 7 |
| `cargo xtask e2e-compare`, bind `127.0.0.1:8765`, no MT, no CI | Task 8 |
| Hard-fail sentences for Chrome / WebGPU / WebGL2 | Tasks 1, 5, 6, 7, 8 |
| Diff PNG and `mismatch.txt`, both pages still sampled | Task 7 |
| `stable_hash64` and L1–L8 unchanged | Tasks 2, 3, 9 |
| Acceptance doc section and first-frame hash wording | Task 9 |
| NOT CONFORMANT banners unchanged | Task 4 note text |
| Future CI, extra scenes, editor-driven native shaders, ST/MT inside this command | Not in this plan |

## Self-review

- Spec sections 2 through 7.5 each have a task in the table above. Section 8 follow-ups are absent on purpose.
- No step says TBD, TODO, or "similar to Task N". WebGL raster code, shaders, and shell wiring are written out.
- `compare_frame_rgba8`, `glgpuPresentCompare`, `glgpuPresentEditorFrame`, `glgpuReadCanvasPixel`, `CompareFrameError` display strings, and the three error sentences use the same names in every task.
