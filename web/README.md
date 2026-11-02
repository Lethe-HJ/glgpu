# Web 运行边界

本目录不是传统前端应用。`index.html` 只提供站点顶栏、`#app` mount point、ST/MT capability detection 和最小 Wasm 启动逻辑；webgpu-mock 页的 DOM、样式、事件、Canvas 以及调试 UI 由 `crates/glgpu-web` 创建。`webgl.html` 是 webgl-mock 的另一条 Wasm 入口，UI 由 `crates/glgpu-webgl-web` 创建。顶栏点击另一侧会整页重载。

这是现有设计，代码中已实现：页面跨源隔离时，呈现是 SharedArrayBuffer 双缓冲加 Atomics。back 槽是逻辑绘制缓冲，front 槽由主线程 `putImageData` / `ImageData` 呈现。命令字节写在同一块缓冲里。不使用 OffscreenCanvas，也没有三缓冲。未隔离时 webgl-mock 仍在主线程用 `DemoContext` 再 `putImageData`。`?threads=1` 的 Rayon worker 仍只跑 tile kernel，queue 留在 owner 线程。

## 生成目录

| 目录 | 来源 | 线程模型 |
| --- | --- | --- |
| `web/pkg` | `cargo xtask build-web --release` | WebGPU stable ST |
| `web/pkg-mt` | `cargo xtask build-web-mt` | WebGPU fixed-nightly SharedArrayBuffer + Rayon |
| `web/pkg-webgl` | `cargo xtask build-web --release --crate crates/glgpu-webgl-web --out-dir web/pkg-webgl --out-name glgpu_webgl_web` | webgl-mock 编辑器壳。未隔离时它自己光栅 |
| `web/pkg-webgl-worker` | `cargo xtask build-present-workers` | webgl-mock Worker，共享内存双缓冲 |
| `web/pkg-webgpu-worker` | `cargo xtask build-present-workers` | webgpu-mock Worker，共享内存双缓冲。不是 `?threads=1` |

这些 `pkg*` 目录都是生成物，已写入 `.gitignore`，不能手工修改其中的 JavaScript、Wasm、TypeScript declaration 或 worker snippet。`web/webgl-worker.js`、`web/webgpu-worker.js` 和 `web/present-slots.js` 是源码。

## 构建与运行

```bash
cargo xtask build-web --release
cargo xtask build-web-mt
cargo xtask build-web --release --crate crates/glgpu-webgl-web --out-dir web/pkg-webgl --out-name glgpu_webgl_web
cargo xtask build-present-workers
cargo xtask serve
```

打开 `/` 使用 webgpu-mock。本地服务器已经隔离，且没有 `?threads=1` 时，绘制在 present worker。打开 `/?threads=1` 请求 webgpu-mock MT。打开 `/webgl.html` 使用 webgl-mock；隔离时绘制在 `webgl-worker`。MT loader 检查跨源隔离、SharedArrayBuffer、共享 WebAssembly memory 和 CPU 数，最多初始化 8 个 worker。检测或初始化失败会回退到独立 ST 包。webgl-mock 页没有 Rayon MT 包。没有 SharedArrayBuffer 时，两条 mock 都退回主线程绘制。

不要直接使用不带响应头的通用静态服务器测试 MT。`cargo xtask serve` 会为 HTML、JavaScript、Wasm 和 worker helper 统一发送 COOP/COEP/CORP，并为 `.wasm` 返回 `application/wasm`。

`index.html`、`webgl.html`、`api-nav.css`、`coop-coep.js` 和生成包都使用相对 URL，因此同一目录既可以在本地 `/` 提供，也可以在 GitHub Pages 项目站点 `/<repo>/` 提供。不要添加 `<base>`。顶栏用 `./index.html` 与 `./webgl.html` 互相切换。

GitHub Pages 不能发送 COOP/COEP。`coop-coep.js` 只在 URL 带 `threads=1` 且 `crossOriginIsolated !== true` 时注册，给后续导航补上与本地服务器相同的隔离头并刷新一次。本地 `cargo xtask serve` 已经隔离页面，所以不会注册这个 worker。发布步骤见[开发指南](../docs/development-guide.zh-CN.md)的 GitHub Pages 一节。

## 所有权

- 隔离且未请求 `?threads=1` 时，present worker 拥有绘制和两块像素槽。主线程拥有 DOM、Canvas 2D，以及编辑器 façade；
- `?threads=1` 时，浏览器主线程仍拥有 `CpuDevice` 和同步 `ImmediateMainOwnerQueue`。Rayon worker 只执行 tile kernel，不拥有 DOM/Canvas；
- present worker 与 MT 包的 Wasm 内存都是 SharedArrayBuffer，但是两套构建；
- Canvas presenter 从 front 槽复制到 non-shared typed array，再调用 `putImageData`；
- `initThreadPool` 每个 MT Wasm 实例只调用一次。

## 修改规则

- WebGPU UI、WGSL 编辑器、高亮、本地保存、CPU demo 预览、编译诊断和事件修改 `crates/glgpu-web/src/lib.rs`；
- webgl-mock 编辑器修改 `crates/glgpu-webgl-web`。Worker 光栅修改 `crates/glgpu-webgl-worker`。不要把它们连到 webgpu-mock crate；
- webgpu-mock present worker 修改 `crates/glgpu-webgpu-worker`。它不依赖 `glgpu-webgl`；
- WebGPU RGBA8/ImageData 边界修改 `crates/glgpu-canvas2d/src/lib.rs`；WebGL 页在自己的壳里复制像素；
- loader capability/fallback 修改 `web/index.html`；WebGL loader 修改 `web/webgl.html`；
- 顶栏样式修改 `web/api-nav.css`；
- Pages 上的 COOP/COEP service worker 修改 `web/coop-coep.js`；本地响应头仍只改 `xtask/src/main.rs`；
- 构建和 HTTP header 修改 `xtask/src/main.rs`；
- 修改后必须重建相应 artifact，并按[测试与验收](../docs/testing-and-acceptance.zh-CN.md)复测 ST/MT。

完整工具链、部署和故障排查见[开发指南](../docs/development-guide.zh-CN.md)。
