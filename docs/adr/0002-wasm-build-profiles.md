# ADR 0002：分离单线程与多线程 Wasm 构建

- 状态：Accepted for P0
- 日期：2026-08-22
- 决策范围：Rust toolchain、Wasm memory、Rayon、浏览器加载与 Canvas presentation

## 背景

浏览器中的 Wasm threads 需要 atomics、bulk-memory、共享线性内存、重建为线程安全的 Rust 标准库以及跨源隔离。把这些 flags 放进默认 `.cargo/config.toml` 会让所有 Wasm artifact 变成 SharedArrayBuffer 构建，并破坏不能提供 COOP/COEP 的宿主环境。

`wasm-bindgen-rayon` 使用 Web Workers 创建 Wasm 实例级全局 Rayon pool。pool 必须在第一次并行工作前初始化，并且不能在同一个实例内重新配置线程数。

此外，浏览器 `ImageData` constructor 不接受 SharedArrayBuffer-backed `Uint8ClampedArray`。MT renderer 的 framebuffer 不能直接交给 Canvas 2D，必须经过 non-shared staging copy。

## 决策

产出两个独立浏览器 artifact：

| Profile | Toolchain | Wasm memory | Rayon | 输出目录 | 必需性 |
| --- | --- | --- | --- | --- | --- |
| ST | Rust `1.93.0` stable | 普通 `ArrayBuffer` | 否 | `web/pkg` | 必须支持 |
| MT | `nightly-2025-11-15` | imported shared memory / `SharedArrayBuffer` | 是 | `web/pkg-mt` | 可选加速 |

锁定版本和构建条件记录在根目录 [`spec-lock.toml`](../../spec-lock.toml)。普通 `rust-toolchain.toml` 只选择 stable；MT flags 单独保存在 [`.cargo/mt.toml`](../../.cargo/mt.toml)，避免污染 ST。

## 构建合同

### ST

```bash
cargo xtask build-web --release
```

等价核心操作是使用 `wasm-pack --target web` 构建 `crates/glgpu-web` 到 `web/pkg`。

### MT

```bash
cargo xtask build-web-mt
```

该命令故意不接受覆盖参数，固定执行：

```bash
cargo +nightly-2025-11-15 \
  --config .cargo/mt.toml \
  build -Z build-std=std,panic_abort \
  --target wasm32-unknown-unknown \
  --release \
  -p glgpu-web \
  --features parallel
```

随后用严格匹配 `0.2.127` 的 `wasm-bindgen --target web` 输出 `web/pkg-mt`。`xtask` 先查 `PATH`，再查 wasm-pack 的 versioned cache；发现不匹配版本不会继续生成 glue。

MT linker 合同包括：

- `target-feature=+atomics,+bulk-memory`；
- shared/imported memory；
- 1 GiB maximum memory；
- `__wasm_init_tls`、`__tls_size`、`__tls_align`、`__tls_base` exports；
- 重新构建 `std` 与 `panic_abort`。

这些参数是一个整体，不能挑选部分复制到 ST profile。

## Runtime 选择

默认 URL `/` 加载 ST。只有显式访问 `/?threads=1` 才请求 MT。loader 在实例化 MT 前检查：

- `globalThis.crossOriginIsolated === true`；
- 存在 `SharedArrayBuffer`；
- 可以构造 `shared: true` 的 `WebAssembly.Memory`；
- `navigator.hardwareConcurrency > 1`。

通过 preflight 后，loader 初始化 `min(8, max(1, hardwareConcurrency - 1))` 个 worker，再调用 Rust `boot(true)`。一个逻辑 CPU 留给页面 owner，pool 最多 8 个 worker。

preflight 失败时不加载 MT。若 MT package fetch/instantiate 或 pool initialization 在 preflight 后失败，loader 丢弃该 candidate，并加载独立 ST artifact；不会尝试把 shared-memory module 当成 sequential build 使用。

## 部署合同

MT 页面和它加载的所有同源资源必须保持跨源隔离。本地 `cargo xtask serve` 对所有响应发送：

```text
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
Cross-Origin-Resource-Policy: same-origin
X-Content-Type-Options: nosniff
Cache-Control: no-store
```

Wasm 使用 `Content-Type: application/wasm`。生产部署还必须使用 HTTPS 等 secure context，并验证 worker helper、Wasm、JavaScript 和第三方资源符合 COEP/CORP/CORS。

若宿主无法设置上述响应头，必须使用 ST，不能通过关闭浏览器安全检查规避。

GitHub Pages（含 `actions/deploy-pages`）属于这种宿主：它不能配置自定义响应头，`_headers` 也不会生效。仓库仍把 ST 和 MT 产物一起发布到 `https://Lethe-HJ.github.io/glgpu/`。`/` 只启动 ST。`/?threads=1` 在页面尚未隔离时注册同源 `web/coop-coep.js`，由 service worker 在后续响应上写入与本地服务器相同的 `COOP: same-origin`、`COEP: require-corp` 和 `CORP: same-origin`，然后刷新一次。这是在补上同一组隔离头，不是关闭浏览器检查。浏览器拒绝这些头、硬刷新绕过 worker，或 worker 未能取得控制时，loader 回退 ST。

`cargo xtask serve` 继续发送真实响应头。`crossOriginIsolated === true` 时页面不注册该 worker。已经带有 COOP/COEP 的响应会被 worker 原样转发，避免改写本地服务器的字节流。

## Canvas presentation

ST Wasm memory 是普通 ArrayBuffer，`glgpu-canvas2d` 使用 `Clamped<&[u8]>` 构造 `ImageData`。

MT Wasm memory 是 SharedArrayBuffer。Canvas boundary 先创建普通 `Uint8ClampedArray` 并复制 framebuffer，再构造 `ImageData`。这会产生至少一次 full-frame copy：

```text
shared Wasm RGBA8 → ordinary Uint8ClampedArray → ImageData → Canvas bitmap
```

因此：

- 不宣称 MT presentation 零拷贝；
- benchmark 必须拆分 raster、staging copy 和 `putImageData`；
- 后续应复用 staging/ImageData，评估 dirty rect，而不是假设更多 worker 一定提高整帧性能；
- OffscreenCanvas 只能改变 owner 和 UI 卡顿，不能消除 shared→non-shared copy。

## 后果

### 正面

- ST 在所有普通 Wasm 环境中保持一等支持；
- MT build 可明确失败，不会静默生成缺少 atomics/TLS 的 artifact；
- 两个包可以独立回归，ST/MT framebuffer hash 可比较；
- 生产宿主不必为了基础功能强制启用 COOP/COEP。

### 成本

- 构建时间、产物和浏览器测试矩阵翻倍；
- MT 依赖固定 nightly、`rust-src` 和匹配的 `wasm-bindgen` CLI；
- SharedArrayBuffer→ImageData staging 是明确带宽成本；
- 全局 Rayon pool 的 worker 数不能在实例内动态修改；
- COOP/COEP 可能影响 popup、第三方资源和 iframe 集成。

## 已拒绝的替代方案

### 只有 MT artifact

拒绝。许多静态宿主、iframe 和第三方集成无法保证跨源隔离，基础功能不能因此不可用。

### 一个 artifact 在运行时切换 shared/non-shared memory

拒绝。Wasm memory/shared 和 Rust std 的线程能力在构建/实例化时确定，不能在同一二进制中安全切换。

### 使用 bundler 隐藏 worker 处理

当前拒绝。项目不引入传统前端工具链，使用 `wasm-bindgen-rayon/no-bundler` 和 `--target web`。

## 验证与重新评估

当前已验证 stable ST 和 fixed-nightly MT 均能构建；本机 Chromium 中 ST/MT demo framebuffer hash 一致，MT 显示 Rayon executor，控制台无 warn/error。

仍需补齐 Firefox MT、Safari ST fallback、独立 1/2/4/8 worker 实例和 720p/1080p 分段 benchmark。只有数据表明其他 worker/runtime 或单 artifact 方案明显更可靠时，才通过新 ADR 重新评估。

