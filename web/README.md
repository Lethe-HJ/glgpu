# Web 运行边界

本目录不是传统前端应用。`index.html` 只提供 `#app` mount point、ST/MT capability detection 和最小 Wasm 启动逻辑；DOM、样式、事件、Canvas 以及调试 UI 全部由 `crates/glgpu-web` 中的 Rust/Wasm 创建。

## 生成目录

| 目录 | 来源 | 线程模型 |
| --- | --- | --- |
| `web/pkg` | `cargo xtask build-web --release` | stable ST |
| `web/pkg-mt` | `cargo xtask build-web-mt` | fixed-nightly SharedArrayBuffer + Rayon |

两个目录都是生成物，已写入 `.gitignore`，不能手工修改其中的 JavaScript、Wasm、TypeScript declaration 或 worker snippet。

## 构建与运行

```bash
cargo xtask build-web --release
cargo xtask build-web-mt
cargo xtask serve
```

打开 `/` 使用 ST；打开 `/?threads=1` 请求 MT。MT loader 检查跨源隔离、SharedArrayBuffer、共享 WebAssembly memory 和 CPU 数，最多初始化 8 个 worker。检测或初始化失败会回退到独立 ST 包。

不要直接使用不带响应头的通用静态服务器测试 MT。`cargo xtask serve` 会为 HTML、JavaScript、Wasm 和 worker helper 统一发送 COOP/COEP/CORP，并为 `.wasm` 返回 `application/wasm`。

## 所有权

- 浏览器主线程拥有 `CpuDevice`、同步 `ImmediateMainOwnerQueue`、DOM 和 Canvas 2D context；
- Rayon worker 只执行 tile kernel，不拥有 DOM/Canvas；
- MT framebuffer 位于 SharedArrayBuffer-backed Wasm memory；
- Canvas presenter 负责复制到 non-shared typed array，再调用 `putImageData`；
- `initThreadPool` 每个 Wasm 实例只调用一次。

## 修改规则

- UI 和事件修改 `crates/glgpu-web/src/lib.rs`；
- RGBA8/ImageData 边界修改 `crates/glgpu-canvas2d/src/lib.rs`；
- loader capability/fallback 修改 `web/index.html`；
- 构建和 HTTP header 修改 `xtask/src/main.rs`；
- 修改后必须重建相应 artifact，并按[测试与验收](../docs/testing-and-acceptance.zh-CN.md)复测 ST/MT。

完整工具链、部署和故障排查见[开发指南](../docs/development-guide.zh-CN.md)。

