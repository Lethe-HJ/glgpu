# GLGPU P0 测试与验收基线

> 状态：P0 执行文档 v0.1  
> 日期：2026-08-24
> 适用范围：`CpuGpuSubset`、CPU triangle raster、WGSL parse/validate、Canvas 2D ST/MT 演示  
> 规范与能力边界以 [`spec-lock.toml`](../spec-lock.toml) 和 [`webgpu-p0.toml`](support-matrix/webgpu-p0.toml) 为准

## 1. 文档目的与判定原则

本文把三类信息严格分开：

1. **自动化测试已证明的事实**：由 Rust tests、Clippy 或构建脚本测试覆盖；
2. **本机浏览器已有证据的事实**：记录浏览器、构建模式、hash 和控制台结果；
3. **仍待验收的目标**：没有证据时保持 `OPEN`，不能根据源码推断为“已支持”。

P0 的正确性 oracle 是内部 framebuffer，不是 Canvas 截图：

- `Framebuffer::stable_hash64()` 对 extent、RGBA8 color plane 和每个 depth f32 的精确 bit pattern 做稳定 FNV-1a hash；
- sequential/parallel 一致性还应比较完整 framebuffer、stats 和 selected-pixel history；
- Canvas 2D 只负责显示，浏览器 compositor、CSS 缩放或截图不能替代内部结果检查。

本文所称：

- **ST**：`web/pkg` 中的普通、非 shared-memory Wasm 构建；
- **MT**：`web/pkg-mt` 中的 shared-memory Wasm 构建，使用 `wasm-bindgen-rayon`；
- **owner**：持有 `ImmediateMainOwnerQueue`、Canvas 2D context 和同步 `submit` 调用的线程；
- **Rayon worker**：仅执行 tile kernel，不拥有 DOM、Promise 或 Canvas API。

当前 MT 并不是异步 WebGPU queue，也不是 worker-hosted owner：`submit` 会等 CPU raster 完成后才返回，页面 owner 仍可能阻塞。

## 2. 当前 P0 能力边界

当前可测试的闭环为：

```text
CpuGpuSubset
  → CpuAdapter / CpuDevice
  → CommandEncoder
  → clear + ordered screen-space triangles + selected pixel
  → ImmediateMainOwnerQueue::submit
  → RGBA8/depth framebuffer + pixel history + stable hash
  → readback_rgba8
  → ImageData + CanvasRenderingContext2D.putImageData
```

WGSL 当前只进行 Naga 30 parse/validate 和 entry-point metadata 提取；CPU triangle 使用已经位于 framebuffer 坐标中的 `ScreenVertex`，没有执行 WGSL vertex/fragment/compute shader。

## 3. 环境前提

版本由仓库锁定：

| 项目 | P0 值 |
| --- | --- |
| Rust | `1.93.0` |
| ST target | `wasm32-unknown-unknown`，普通 memory |
| MT toolchain | `nightly-2025-11-15` |
| Naga | `30.0.1` |
| wasm-pack | `0.14.0` |
| wasm-bindgen / CLI | `0.2.127` |
| wasm-bindgen-rayon | `1.3.0` |
| Rayon | `1.12.0` |

MT 还要求：

- nightly 安装 `rust-src` 和 `wasm32-unknown-unknown`；
- `.cargo/mt.toml` 中的 atomics、bulk-memory、shared/imported memory 和 TLS exports；
- shared memory 最大值当前为 1 GiB；
- `wasm-bindgen` CLI 必须是 `0.2.127`；`xtask` 会检查 `PATH` 和 wasm-pack 的版本化缓存；
- 浏览器处于可信安全上下文。localhost 可用于开发，生产应使用 HTTPS；
- 页面和 worker 处于 cross-origin isolated 环境。

## 4. 测试层次

| 层次 | 覆盖内容 | 当前主要证据 |
| --- | --- | --- |
| L0 静态门 | 格式、编译、所有 feature 的 lint | `cargo fmt`、`cargo clippy` |
| L1 公共数据 | extent、plane 长度、坐标、color/depth hash | `glgpu-core` unit tests |
| L2 trace | primitive 顺序与最终 pixel value | `glgpu-trace` unit test |
| L3 shader | VS/FS/compute parse+validate、source span、语义负例、P0 禁止的 optional capability | `glgpu-shader/tests/wgsl.rs` |
| L4 raster | coverage、top-left shared edge、depth/equal-depth 顺序、clockwise 输入、pixel history | `glgpu-raster` tests |
| L5 executor | sequential 与 native Rayon 的 framebuffer/hash/stats/history 完全一致 | `sequential_and_rayon_outputs_are_identical` |
| L6 subset API | support matrix、Unsupported、clear/draw/submit/readback、WGSL 只验证不执行 | `glgpu-webgpu/tests/immediate_queue.rs` |
| L7 Canvas boundary | RGBA8 长度、overflow、错误信息；Wasm 上的 ImageData/putImageData | native validation tests + 浏览器 smoke |
| L8 build/server | ST/MT 命令锁、路径安全、COOP/COEP、MIME | `xtask` tests + 实际 HTTP HEAD |
| L9 browser | ST/MT boot、fallback、Canvas、点击 trace、console、hash | 手工浏览器验收记录 |
| L10 性能 | raster、readback、SAB staging、putImageData 的分段与端到端成本 | 尚需专用 benchmark harness |

## 5. 必跑命令

所有命令从 workspace 根目录执行。

### 5.1 提交前快速门

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

`cargo test --workspace` 使用默认 feature，除了验证正常路径，还验证“没有启用 `parallel` 时请求并行必须明确失败”。

### 5.2 全 feature 与并行专项门

```bash
cargo test --workspace --all-features
cargo test -p glgpu-raster --features parallel
cargo test -p glgpu-webgpu --features parallel
```

其中 raster parallel test 必须同时满足：

- `stable_hash64` 相同；
- 完整 color/depth framebuffer 相同；
- `Stats` 相同；
- `PixelHistory` 相同。

只比较“画面看起来一样”不算通过。

### 5.3 ST/MT Wasm 构建

```bash
cargo xtask build-web --release
cargo xtask build-web-mt
```

预期产物：

```text
web/pkg/glgpu_web.js
web/pkg/glgpu_web_bg.wasm
web/pkg-mt/glgpu_web.js
web/pkg-mt/glgpu_web_bg.wasm
```

`build-web-mt` 是无参数的锁定命令；传入 `--release` 或其他覆盖参数应失败。它必须使用：

```text
cargo +nightly-2025-11-15 --config .cargo/mt.toml \
  build -Z build-std=std,panic_abort \
  --target wasm32-unknown-unknown --release \
  -p glgpu-web --features parallel
```

随后由 `wasm-bindgen --target web` 写入 `web/pkg-mt`。

### 5.4 本地服务器与 HTTP 检查

```bash
cargo xtask serve
```

在另一个终端执行：

```bash
curl -I http://127.0.0.1:8080/
curl -I http://127.0.0.1:8080/pkg/glgpu_web.js
curl -I http://127.0.0.1:8080/pkg/glgpu_web_bg.wasm
curl -I http://127.0.0.1:8080/pkg-mt/glgpu_web_bg.wasm
```

每个响应都必须包含：

```http
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
Cross-Origin-Resource-Policy: same-origin
X-Content-Type-Options: nosniff
Cache-Control: no-store
```

MIME 必须满足：

| 文件 | `Content-Type` |
| --- | --- |
| HTML | `text/html; charset=utf-8` |
| `.js` / `.mjs` | `text/javascript; charset=utf-8` |
| `.wasm` | `application/wasm` |
| source map / JSON | `application/json; charset=utf-8` |

不能用 `application/octet-stream` 提供 Wasm；在 `nosniff` 下这会直接阻断正确加载或流式编译。

## 6. 当前已复跑的自动化基线

2026-08-22 在锁定 Rust 1.93.0 环境中得到：

| 检查 | 结果 |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo test --workspace` | PASS，38 tests |
| `cargo test --workspace --all-features` | PASS，38 tests |
| raster `--features parallel` | PASS，6 tests |
| webgpu `--features parallel` | PASS，6 tests |
| all-target/all-feature Clippy，`-D warnings` | PASS |
| `cargo xtask build-web --release` | PASS |
| `cargo xtask build-web-mt` | PASS |
| 实际 HTTP HEAD：HTML、ST Wasm、MT Wasm、JS | PASS，隔离头和 MIME 正确 |

测试数量是快照信息，不应被写成永久固定值；以后新增测试时，以“全部通过且无 skip/ignored 的意外增加”为门。

## 7. ST/MT hash 一致性

### 7.1 Native 层已经证明的内容

启用 `glgpu-raster/parallel` 后，同一组有重叠和 depth 顺序的 triangles 在 sequential executor 与 Rayon tile executor 上得到完全一致的：

- color plane；
- depth plane 的精确 f32 bits；
- stable hash；
- raster stats；
- selected-pixel history。

这证明当前 native tile kernel 不依赖 worker 抢占顺序，但不能替代浏览器 ST Wasm 与 shared-memory MT Wasm 的跨 artifact 验证。

### 7.2 浏览器验收步骤

1. 完成两套 release 构建并启动 `cargo xtask serve`；
2. 打开 `http://127.0.0.1:8080/`；
3. 记录 capability 行中的 `build=ST` 和首帧 metrics hash；
4. 打开 `http://127.0.0.1:8080/?threads=1`；
5. 必须确认 capability 行显示 `build=MT`，metrics 显示 `Rayon tile executor`；
6. 记录 `crossOriginIsolated=true`、SharedArrayBuffer 可见性和 MT hash；
7. 比较 ST/MT hash，必须逐字符相同；
8. 点击相同像素，比较最终 RGBA、depth、primitive outcome 顺序；
9. 检查 console，不允许 panic、warning、fallback 或 worker 初始化错误。

仅看到相同画面不算 MT 通过：加载器在 MT 失败时会自动回退 ST，因此必须同时检查 `build=MT`、Rayon executor 和 console。

### 7.3 当前 Chromium 证据

本机 Codex in-app Chromium 已实测：

| 项目 | ST | MT |
| --- | --- | --- |
| build 标识 | `ST` | `MT` |
| executor | sequential | Rayon |
| `crossOriginIsolated` | `true` | `true` |
| 首帧 hash | `2898a668a856be56` | `2898a668a856be56` |
| console warning/error | 无 | 无 |

该 hash 是当前 720×480 P0 triangle scene 的证据快照。实现或场景有意改变后可以更新 hash，但同一个候选版本的 ST/MT 仍必须完全一致。

### 7.4 尚未关闭的线程矩阵

当前 `?threads=1` 中的 `1` 只是“请求 MT”的布尔开关，不代表一个 Rayon worker。loader 实际使用：

```text
min(8, max(1, hardwareConcurrency - 1))
```

因此当前证据只覆盖本机自动选择的一个 worker 数，**不能声称 1/2/4/8 worker 都一致**。要关闭该 gate，需要增加显式 worker-count override/test harness，并在独立 Wasm/Worker 实例中分别初始化 1、2、4、8 workers；全局 Rayon pool 不能在同一实例中重复初始化。

## 8. 浏览器人工检查表

每个浏览器记录完整版本、OS、CPU、逻辑核数、页面 URL、实际 worker 数、ST/MT hash 和控制台摘要。

### 8.1 ST 必测

- `/` 显示 `build=ST`；
- Canvas 显示带颜色插值的 triangle；
- 页面明确显示 `CpuGpuSubset — NOT WEBGPU CONFORMANT`；
- WGSL 编辑器显示完整示例源码，关键字、attribute、类型、数字和注释高亮，源码与高亮层在横纵滚动时保持对齐；
- `Ctrl/⌘ + S` 和“保存”按钮均阻止浏览器的保存网页动作、将源码写入本地存储，并在刷新后恢复；编辑后状态显示“未保存”；
- 修改源码后显示“未编译”；默认源码点击“编译”显示 vertex `vs_main` 与 fragment `fs_main` 两个入口点，且 submission serial 不增加；
- 语法或类型错误显示 Parse/Validation、1-based 行列、源码片段与 caret；错误位置在含中文注释时仍能正确选中；
- 修改示例 `positions` 或 `colors` 十进制常量后运行，Canvas、hash 与 submission serial 都必须更新；
- 无效源码或无法提取 demo triangle 常量的源码点击“运行”不得提交新帧或覆盖上一帧；修复后运行会清除旧错误并使 submission serial 增加；
- 页面持续说明 P0 CPU 预览只读取 demo 的 position/color 常量，不得声称已经执行任意 WGSL 控制流；
- 点击 Canvas 后 selected pixel 坐标与点击位置对应；
- center pixel 有最终 RGBA/depth 和 primitive history；
- 不在 triangle 内的 pixel 显示无写入或 coverage 结果；
- 连续点击不会改变 framebuffer hash；
- console 无 panic/error。

### 8.2 MT 必测

- `/?threads=1` 实际显示 `build=MT`，不是 fallback ST；
- `crossOriginIsolated=true`；
- `SharedArrayBuffer=true`；
- metrics 显示 `Rayon tile executor`；
- ST/MT hash 完全相同；
- selected-pixel history 完全相同；
- 页面可重复提交且 worker pool 只初始化一次；
- console 无 worker、memory、ImageData 或 wasm-bindgen 错误。

### 8.3 fallback 负例

在没有 cross-origin isolation 或不支持 shared-memory Wasm 的环境请求 `?threads=1`：

- loader 不应先实例化不可用的 MT 模块；
- 应记录清晰 warning 并启动独立 ST package；
- 页面显示 `build=ST`，triangle、hash 和点击 trace 正常；
- fallback 不能被报告为 MT 成功。

### 8.4 浏览器矩阵状态

| 浏览器门 | 当前状态 | P0 要求 |
| --- | --- | --- |
| Codex in-app Chromium ST | PASS | 保留回归证据 |
| Codex in-app Chromium MT | PASS，自动 worker 数 | 增加精确版本/worker 数记录 |
| Firefox ST | OPEN | 必须 smoke |
| Firefox MT | OPEN | 必须验证 shared-memory、hash、console |
| Safari ST | OPEN | 必须 smoke；P0 允许 ST fallback |
| Safari MT | 非 P0 承诺 | 只记录能力，不阻断 ST fallback |
| Chromium/Firefox 1/2/4/8 workers | OPEN | 增加可控 worker harness 后逐项通过 |

在 Firefox 和独立 worker-count 矩阵关闭前，不能宣称“Chromium + Firefox MT 已支持”或“所有线程数确定性一致”。

## 9. 部署验收

### 9.1 开发服务器

`xtask serve` 的测试已覆盖：

- path traversal 和编码 traversal 拒绝；
- symlink 不能逃逸静态根目录；
- `/` 和 query string 路径解析；
- Wasm/JS MIME；
- 每个 response 的 COOP/COEP；
- ST `wasm-pack --target web` 参数；
- MT nightly、build-std、feature 和 wasm-bindgen 参数锁。

实际 HTTP HEAD 也已验证 HTML、JS、ST Wasm 和 MT Wasm 均返回隔离头，其中 Wasm 为 `application/wasm`。

### 9.2 生产部署仍需单独验证

- HTML、Wasm、JS、worker script/response 都必须处于相容的 COEP 环境；
- 所有跨源图片、字体或其他资源必须满足 CORS/CORP，否则 `require-corp` 会阻断加载；
- iframe 需要父子隔离链，且 `Permissions-Policy: cross-origin-isolated` 不得阻断；
- `wasm-bindgen-rayon/no-bundler` 可能使用 blob worker，严格 CSP 需实测 `worker-src 'self' blob:` 和 Wasm 编译权限；
- COOP 对 OAuth/payment popup 的 opener 行为需要宿主应用评估；
- 库不能替生产宿主设置响应头，ST 必须始终是一等 fallback。

当前 `xtask serve` 没有设置生产 CSP，也没有覆盖第三方资源、iframe 或 popup 集成；不能把本地服务器通过等同于生产部署完成。

## 10. 性能测量口径

### 10.1 当前 UI 数值代表什么

当前页面使用 `Date::now()` 包围一次完整 `render()`，其 elapsed time 包含：

1. command encoder 创建和记录；
2. 同步 `ImmediateMainOwnerQueue::submit`；
3. selected-pixel tracing；
4. framebuffer → owned RGBA8 `Vec` 的 `readback_rgba8` 复制；
5. Canvas presentation；
6. MT 下 SAB-backed bytes → 普通 JS typed array 的强制 staging copy；
7. `putImageData` 调用成本。

因此该数值是单帧端到端诊断值，不是纯 raster 时间，也不是 GPU 时间。ST 虽然省去 MT 专有的 SAB → ordinary ArrayBuffer staging，但当前 demo 仍执行 framebuffer → RGBA8 readback copy。

当前场景固定为 720×480、一个 triangle、一次 selected-pixel trace。单帧 RGBA8 为：

```text
720 × 480 × 4 = 1,382,400 bytes ≈ 1.318 MiB
```

### 10.2 可用于冻结性能门的数据

加入 benchmark harness 后，每个结果必须记录：

- commit、release/debug、ST/MT；
- 浏览器完整版本、OS、CPU、逻辑核、实际 worker 数；
- 分辨率、triangle 数、overdraw、selected-pixel tracing 是否开启；
- warm-up 帧数和采样帧数；
- raster、readback、SAB staging、`putImageData` 和总时间的 median/p95/max；
- 每帧复制字节数、峰值 Wasm memory；
- 所有样本的 correctness hash；
- 页面是否前台、机器电源/节能模式。

建议至少 warm-up 30 帧、采样 120 帧，并分别报告 ST 与每个 MT worker 数。不同浏览器或机器的数据不能直接混成一个平均值。

### 10.3 当前不可用的性能声明

架构讨论中的“720p 15fps”仍是待实测冻结的暂定目标。当前页面没有稳定的多帧 benchmark loop，也没有分段计时，因此不能根据一次 elapsed time 声称：

- 已达到 15/30/60fps；
- MT 有稳定加速比；
- Canvas copy 不是瓶颈；
- 1080p/4K 可交互；
- UI 在 submit 期间不会卡顿。

P0 性能门当前要求“采集可复现数据并据此作 go/no-go”，不把未经 benchmark 的帧率数字作为已通过门槛。

## 11. P0 验收门与当前状态

| Gate | 必须满足 | 当前状态 |
| --- | --- | --- |
| G0 范围诚实 | subset 命名、support matrix、Unsupported、非 conformant 标识 | PASS |
| G1 Native correctness | 默认 workspace tests、format、Clippy 全绿 | PASS |
| G2 Native parallel determinism | framebuffer/hash/stats/history exact match | PASS |
| G3 WGSL 边界 | VS/FS/compute validate；source span；optional feature 拒绝 | PASS；仅验证、不执行 |
| G4 ST build | release Wasm 可重复构建并生成 `web/pkg` | PASS，本机构建成功 |
| G5 MT build | 锁定 nightly/shared-memory 构建生成 `web/pkg-mt` | PASS，本机构建成功 |
| G6 HTTP isolation | HTML/JS/Wasm headers 与 MIME 正确 | PASS，本机实际 HEAD + tests |
| G7 Chromium ST/MT | 两种 build 确认、console 干净、hash 一致 | PASS，本机 in-app Chromium |
| G8 Firefox ST/MT | ST smoke；MT boot/hash/console | **OPEN** |
| G9 Safari ST fallback | ST smoke 和 MT 请求安全 fallback | **OPEN** |
| G10 1/2/4/8 workers | 独立实例 exact hash/history | **OPEN；当前 loader 不可指定数量** |
| G11 性能数据 | 固定环境、分段计时和可复现报告 | **OPEN；当前只有单帧总时间** |

因此当前可以说“P0 native vertical slice 和本机 Chromium ST/MT 闭环已通过”；完整跨浏览器/线程数 P0 gate 尚未关闭。

## 12. 已知缺口与禁止声称的内容

当前不得声称：

- “WebGPU conformant”或“可替代 `navigator.gpu`”；
- 实现了标准 `GPUAdapter/GPUDevice/GPUQueue`、WebIDL brand 或 Promise 时间线；
- 实现了异步 queue、`mapAsync`、error scope、device loss 或 worker-hosted owner；
- WGSL 已在 CPU 上执行；当前只 parse/validate；
- 支持 bind groups、pipeline layouts、buffers mapping、textures/samplers、compute dispatch；
- 支持 implicit LOD、derivatives、MSAA、queries、timestamps 或 external textures；
- triangle 已经过 vertex shader、clip space、perspective divide；当前输入已是 framebuffer 坐标；
- depth 是标准 WebGPU texture format；当前只是内部 P0 plane；
- Canvas 是零拷贝；MT 明确需要 SAB staging，当前 ST/MT 都还有 readback copy；
- MT submit 不阻塞 UI；当前 queue 是同步 owner queue；
- 1/2/4/8 worker、Firefox MT 或 Safari MT 已通过；
- 已达到任何固定 fps 或真实应用性能；
- 已通过 WebGPU CTS、WebGL conformance、浏览器自动化或生产 CSP/iframe 部署矩阵；
- pixel history 已覆盖 clip/cull/API/queue 全时间线；当前主要是 raster-stage events。

`docs/support-matrix/webgpu-p0.toml` 是机器可读的声明源。实现、测试、页面 badge 与该矩阵不一致时，验收必须失败。

## 13. 浏览器证据记录模板

每次候选发布复制一行填写：

| 日期/commit | 浏览器/版本 | OS/CPU | 模式 | workers | isolated/SAB | hash | console | Canvas/trace | 结论 |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- | --- |
| YYYY-MM-DD / SHA | Chromium x.y | OS / CPU | ST | 0 | true/true | 16 hex | clean | pass | PASS |
| YYYY-MM-DD / SHA | Chromium x.y | OS / CPU | MT | N | true/true | 16 hex | clean | pass | PASS |

最终签署还应附：

- `cargo test`、Clippy、ST build、MT build 的完整日志；
- HTTP HEAD 输出；
- 浏览器 console 导出或截图；
- ST/MT hash 对照；
- 性能原始样本和汇总方法；
- 所有 `OPEN` gate 的 issue 或明确延期决定。
