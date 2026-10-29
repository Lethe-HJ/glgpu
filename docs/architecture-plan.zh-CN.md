# GLGPU：WebGPU 优先的 CPU 调试模拟器规划（WebGL 后续）

GLGPU 本质上是实验性的，目的是亲自把项目做一遍，抓住 WebGL 与 WebGPU 的核心想法，而不是做成完整的、可上生产的实现。范围保持精简：保真度只要够理解这些核心，不为求完备而过度设计。这是文档里的宗旨，不是代码开关。webgl-mock 与 webgpu-mock 是对等的两条 CPU 实现。原生 WebGL / 原生 WebGPU 只在 demo 层做并排对照，目前还没有这两页。

## 现有实现与未来设计

后文阶段规划里，凡是「当前实现快照」没有列出的能力，都是未来的设计，目前还未实现。和呈现、线程直接相关的决定如下。

| 设计点 | 状态 | 目前代码 |
| --- | --- | --- |
| webgpu-mock P0：CPU 三角形、depth、同步 queue、Canvas 2D | 现有 | `CpuGpuSubset`、`ImmediateMainOwnerQueue`、`glgpu-canvas2d` 的 `put_image_data`。标识 `CpuGpuSubset — NOT WEBGPU CONFORMANT`。WGSL 只校验，不执行 |
| webgpu MT 的 Rayon tile | 现有，范围有限 | `wasm-bindgen-rayon` 的 SharedArrayBuffer 是 Wasm 线性内存。worker 只跑 tile kernel。这不是 front/back 两槽呈现 |
| webgl-mock：clear、clip、ESSL→bytecode、shader 三角形 demo | 现有 | `DemoContext` 的 `Vec<Rgba8>` 与 `draw_shader_triangle`。主线程 `put_image_data` |
| 不用 OffscreenCanvas；页面 canvas 只呈现 | 现有 | 两条 demo 都拿 Canvas 2D，不把页面 canvas 当原生 GL/GPU drawing buffer |
| webgl-mock 与 webgpu-mock 对等 | 现有 | 两个 crate、两张 mock 页（`web/webgl.html` 与 `web/index.html`），互不依赖 |
| demo 层的原生 WebGL / 原生 WebGPU 对照页 | 未来 | 这是未来的设计，目前还未实现。目前没有并排的原生三角形页 |
| Worker + Wasm 后端与主线程 Web API façade | 未来 | 这是未来的设计，目前还未实现。目前两条 Wasm 都在主线程跑绘制 |
| SharedArrayBuffer 双缓冲 + Atomics；back 槽是逻辑绘制缓冲 | 未来 | 这是未来的设计，目前还未实现。目前 webgl 像素在 `Vec<Rgba8>`；webgpu 像素在 `glgpu-core` 的 framebuffer |
| 三缓冲不是默认 | 未来约束 | 目前没有双槽，也没有第三块呈现缓冲 |

下文第 14.2 节里 2026-10-28 的双缓冲条文同样是未来的设计。

> 状态：执行基线 v0.4（WebGPU-first；P0 垂直切片已落地，完整 P0 gate 仍在进行）  
> 日期：2026-08-22  
> 仓库现状：Cargo workspace、规范锁、ADR、P0 CPU 渲染/追踪与 ST/MT Web 演示已建立  
> 目标平台：浏览器中的 Rust + WebAssembly；CPU 执行；Canvas 2D 呈现

## 当前实现快照

本次已经完成可运行的 P0 垂直切片，而不是完成整份长期路线图：

- 16×16 tile、定点边函数、top-left 规则、RGBA8 + depth 的确定性 CPU 三角形光栅化；
- 顺序执行器与 Rayon tile 执行器逐值/hash 一致；
- 选定像素的 coverage/depth/color history；
- Rust-native `CpuGpuSubset`、同步 `ImmediateMainOwnerQueue` 和明确的支持矩阵；
- Naga WGSL parse/validate 与 source-span 诊断；当前只校验 WGSL，尚不执行 WGSL；
- Rust/Wasm 创建全部页面、样式、事件和 Canvas 2D 调试 UI；HTML 只保留极薄的双包启动器；
- stable ST 包和锁定 nightly 的 `wasm-bindgen-rayon` MT 包；开发服务器对所有资源发送 COOP/COEP；
- MT presentation 在 SharedArrayBuffer 与 `ImageData` 之间执行必要的非共享 staging copy；
- 本地 Chromium 实测 ST/MT 均绘制同一帧 hash，MT 确认使用 Rayon executor，控制台无错误。

尚未实现资源/绑定组/纹理、WGSL lowering 与执行 VM、异步 queue timeline、owner worker、compute、标准
`navigator.gpu` façade、capture/replay 文件格式，以及路线图上的 WebGL 产品实现。它们仍按后文 P1–P9 gate 推进。`glgpu-webgl` 是独立的 CPU 可视化栈，不把 P7 标成已经开工。

与当前代码直接对应的交接文档：

- [P0 API 与行为契约](api-reference.zh-CN.md)
- [开发、构建、部署和故障排查](development-guide.zh-CN.md)
- [测试与验收](testing-and-acceptance.zh-CN.md)
- [实施路线图](roadmap.zh-CN.md)
- [机器可读支持矩阵](support-matrix/webgpu-p0.toml)

## 目录

- [当前实现快照](#当前实现快照)
- [1. 结论先行](#1-结论先行)
- [2. 默认假设](#2-本文采用的默认假设)
- [3–4. 产品定义与规范基线](#3-产品定义)
- [5–7. 总体架构、仓库结构和对外 API](#5-总体架构)
- [8–9. 当前 WebGPU 主线与后续 WebGL 前端](#8-webgpu-前端设计当前主线)
- [10–12. Shader、光栅和纹理](#10-shader-子系统)
- [13–14. 多线程与 Canvas 2D](#13-多线程执行模型)
- [15–18. 调试、测试、性能与部署](#15-调试器能力)
- [19–20. 路线图与 P0](#19-分阶段路线图与人力估算)
- [21. 风险清单](#21-风险清单)
- [22. 首个公开演示](#22-建议的首个公开演示)
- [23. 待讨论决策](#23-需要优先讨论的方案问题)
- [24. 一手资料](#24-一手资料)
- [25. 下一步](#25-下一步)

## 1. 结论先行

这个项目技术上可行，但必须把产品定位定准：它应当是一个**确定性的 CPU 参考渲染器和调试器**，而不是首版就完整替代浏览器 GPU 驱动的高性能 WebGPU/WebGL 实现。

建议采用以下路线：

1. 所有图形 API 状态、资源、验证、shader 执行和光栅化均在 Rust/Wasm 中完成，不调用宿主 WebGPU/WebGL 进行实际绘制。
2. 最终颜色缓冲转换为 RGBA8，通过 Canvas 2D `putImageData` 呈现。
3. 使用 tile-based 软件光栅化；tile 之间用 Rayon 并行，tile 内严格保持 draw/primitive 顺序，以兼顾性能、确定性和调试可解释性。
4. **实现顺序已经确定为 WebGPU 优先、WebGL 后续。** 先完成 WGSL、WebGPU objects/commands/queue/render/compute/debugger，再复用资源和渲染内核增加 WebGL 状态机与 ESSL 前端。
5. WebGPU 和 WebGL 使用各自的 API 前端，但共享资源格式、shader 字节码虚拟机、纹理采样、CPU 光栅器和调试追踪基础设施。WGSL 使用 Naga 完成解析和验证，再降低到项目自有 IR；WebGL 的 ESSL 1.00/3.00 在后续阶段单独实现。
6. 第一公开版本只承诺一个明确的功能子集，并提供机器可读的支持矩阵。Rust-native API 可返回结构化 `Unsupported`；WebGPU/WebGL façade 必须保持规范错误通道，并把子集缺口放到独立诊断通道，不能伪造规范错误或静默错画。
7. 页面结构、事件、Canvas 和调试 UI 全部由 Rust/Wasm 通过 `web-sys` 控制；只保留浏览器无法消除的最小 HTML、`wasm-bindgen` 生成胶水和可选启动/兼容 shim。

完整 WebGL 1/2 加 WebGPU 合规实现是大型长期工程。单人可以较快做出“可解释的三角形—纹理—深度—混合”垂直切片，但达到广泛运行真实框架项目的程度，预计要一年以上；完整合规可能需要多年和专门团队。

## 2. 本文采用的默认假设

下表同时记录已冻结的产品决策与仍需用 P0/P2 数据关闭的默认项；“已确认”项只有通过新 ADR 才能改变。

| 编号 | 默认方案 | 原因 | 状态 |
| --- | --- | --- | --- |
| D1 | 先做 Rust-native API；现有 JavaScript 应用兼容层随后做 | 最符合“Rust/Wasm 控制页面”，也能先验证核心渲染正确性 | **已确认** |
| D2 | “不用传统前端”定义为无 React/Vue/TS/Node 业务层；允许最小 HTML、生成的 JS glue 和极薄 shim | 浏览器加载 Wasm、创建 worker 以及透明拦截 JS API 无法做到字面意义的零 JS | **已确认** |
| D3 | 产品顺序为公共垂直切片 → internal `CpuGpuSubset` Render Alpha → Render/Compute/Queue MVP → WebGPU Debugger Beta → Namespaced JS Adapter → WebGPU standard-façade track → `CpuWebGlSubset` → WebGL 1/2 | 用户已决定先做 WebGPU，WebGL 后续复用公共内核 | **已确认** |
| D4 | 正确性、确定性和诊断质量优先于帧率 | 这是调试器与普通软件渲染器最重要的区分 | **已确认** |
| D5 | 多线程版与单线程 fallback 同时存在 | SharedArrayBuffer 依赖部署响应头，不能假设所有嵌入环境都满足 | **已确认并已实现** |
| D6 | 首批浏览器为 Chromium + Firefox；Safari 纳入测试但允许先使用单线程 fallback | 先降低多线程部署和兼容矩阵风险 | 待 Firefox/Safari 实测冻结 |
| D7 | 后续 WebGL 阶段的 ESSL 使用受限纯 Rust 前端；glslang/ANGLE 只作为 CI oracle | 不影响 WebGPU-first 开工，到 WebGL 阶段前再最终确认 | 延后确认 |

## 3. 产品定义

### 3.1 核心目标

- 当前先在 CPU 上模拟 WebGPU 核心语义；后续复用公共内核支持 WebGL 1/2。
- 最终画面只通过 Canvas 2D 呈现。
- 支持 Rust/Wasm 多线程；当前 P0 已用 `wasm-bindgen-rayon` 落地 ST/MT 双包，后续继续验证跨浏览器与 owner-worker 模式。
- 页面、调试面板和交互由 Rust/Wasm 控制，不引入传统前端框架。
- 对无数据竞争的 workload，在 deterministic-debug profile 中，同一 capture 在单线程和不同线程数下得到相同结果。
- 提供比原生 API 更强的验证错误、状态追踪、shader 单步、像素历史和资源检查能力。
- 核心库不依赖 DOM，既能编译到 Wasm，也能在 native Rust 测试/回放工具中运行。

### 3.2 非目标

- 首版不追求 GPU 级帧率。
- 首版不承诺运行任意 three.js/Babylon.js/WebGPU 引擎而无需修改。
- 首版不承诺通过全部 Khronos WebGL Conformance Suite 或 WebGPU CTS。
- 不模拟真实 GPU 的时序、缓存、并行占用率或厂商特有浮点误差。
- 不为规范未定义或实现相关的行为伪造“唯一正确”的硬件结果；调试模式会采用记录在 profile 中的确定性策略。
- 不把 CPU 执行耗时直接解释为 GPU 性能数据。

### 3.3 “完成”的分层定义

| 层级 | 含义 |
| --- | --- |
| Vertical Slice | CPU triangle → framebuffer → Canvas2D；线程数切换输出一致；可解释一个像素 |
| `CpuGpuSubset` MVP | 可创建基本资源、记录 render/compute commands、运行 WGSL 子集并回读；不冒充满足全部 capability guarantees 的标准 adapter |
| WebGPU Debugger Beta | capture/replay、资源/command/queue 查看、WGSL trace、pixel/invocation history 可用于真实小型 Rust/Wasm `CpuGpuSubset` 项目调试 |
| Standard WebGPU Track | 满足锁定规范的 core capability guarantees，并通过标准 JS façade/CTS gate |
| `CpuWebGlSubset` MVP | 后续通过 WebGL 风格显式 API 复用同一 CPU 内核；不冒充完整 WebGL context |
| Conformance Track | 逐条提升官方测试通过率，并公开 `implemented/partial/unsupported` 矩阵 |

## 4. 规范基线与版本策略

项目不能跟随“浏览器当前行为”随意漂移，必须有可复现的规范锁。

建议基线按实现优先级排列：

- WebGPU：实现启动时锁定一个有日期的 [W3C WebGPU Candidate Recommendation Draft](https://www.w3.org/TR/webgpu/)。
- WGSL：与上述 WebGPU 基线匹配的 [W3C WGSL Draft](https://www.w3.org/TR/WGSL/)。
- Canvas/ImageData：WHATWG HTML Living Standard 的固定 commit。呈现不使用 OffscreenCanvas。
- WebAssembly threads 与绑定：固定 Rust nightly、`wasm-bindgen`、`wasm-bindgen-rayon` 和 Naga 版本。
- 后续 WebGL 1：Khronos WebGL 1.0.3；同时追踪[最新 WebGL 1 编辑草案](https://registry.khronos.org/webgl/specs/latest/1.0/)。
- 后续 WebGL 2：Khronos WebGL 2.0.0；同时追踪[最新 WebGL 2 编辑草案](https://registry.khronos.org/webgl/specs/latest/2.0/)。
- 后续 ESSL：GLSL ES 1.00 与 GLSL ES 3.00；P7a 开始前再冻结精确语法与验证基线。

仓库中应增加 `spec-lock.toml`，记录：

- 规范名称、URL、发布日期或 commit；
- WebIDL 文件 hash；
- conformance/CTS commit；
- 依赖版本；
- 项目 profile 的 limits、formats、extensions/features；
- 已知有意差异。

WebGPU 方法、常量和 descriptor 的静态骨架优先从官方 WebIDL 生成，避免手抄遗漏；WebGL 在 P8 JS adapter 阶段采用同一生成机制。生成只负责“表面结构”，具体验证和执行仍须手工实现及测试。

## 5. 总体架构

```text
Rust client API                  optional JS façade
       │                                │
       ├──────── webgpu-mock ───────────┤  与 webgl-mock 对等；现有 CPU 子集
       │    objects/encoders/queue      │
       │                                │
       └──────── webgl-mock ────────────┘  与 webgpu-mock 对等；现有 CPU 子集
               state + validation
demo 层（未来）：原生 WebGPU / 原生 WebGL 对照页，不进入 mock crate
                        │
              immutable packets
        DrawPacket / DispatchPacket / CopyPacket
                        │
          validation diagnostics + trace recorder
                        │
              ┌─────────┴─────────┐
              │                   │
       shader compiler/VM    resource system
       WGSL / later ESSL     buffers/textures/views
              → IR
              │                   │
              └─────────┬─────────┘
                        │
        CPU vertex/compute/raster/fragment pipeline
                        │
          color/depth/stencil attachments in Wasm
                        │
        presentation conversion + MT staging copy
                        │
              ImageData + putImageData
                        │
                    Canvas 2D

Trace/state/resource/shader/pixel events ──> Rust/Wasm debugger UI
```

### 5.1 必须共享的内核

- 资源句柄、生命周期、内存预算与版本。
- buffer/texture/format/view/sampler 公共数据模型。
- shader 公共 typed IR、bytecode、解释器、source map。
- 顶点获取后的图元装配、裁剪、tile binning、光栅化和逐片元操作。
- compute workgroup 调度器。
- capture/replay 格式和结构化诊断。
- 性能计数与调试 instrumentation。

### 5.2 必须保持分离的部分

- WebGL 的 immediate-mode 大状态机和 `getError` 行为。
- WebGPU 的对象/descriptor/command encoder/queue/error scope/Promise 行为。
- WebGPU 与 WebGL 的坐标、clip-depth、纹理上传、格式和验证规则。
- ESSL 与 WGSL 的解析、类型系统、内建函数和链接规则。

不要为了“复用”而把两个规范压成一个公开 API。复用点应位于验证完成后的内部 packet 层。

## 6. 建议仓库结构

```text
glgpu/
  Cargo.toml                  # workspace
  rust-toolchain.toml         # 固定 nightly，Wasm threads build-std
  .cargo/config.toml          # atomics/shared-memory 配置
  spec-lock.toml
  crates/
    glgpu-core/               # 资源、格式、句柄、limits、packet、错误
    glgpu-shader/             # WGSL 主线、后续 ESSL、IR、bytecode、VM
    glgpu-raster/             # clip/setup/bin/raster/depth/stencil/blend
    glgpu-webgpu/             # WebGPU 对象、encoder、queue、验证
    glgpu-webgl/              # webgl-mock：CPU WebGL2 子集；不依赖 webgpu-mock crate（ADR 0003，产品 gate 仍是 P7b）
    glgpu-webgl-web/          # WebGL 调试页 Wasm 壳；只依赖 glgpu-webgl
    glgpu-trace/              # capture、replay、state diff、pixel history
    glgpu-canvas2d/           # ImageData 呈现；不使用 OffscreenCanvas
    glgpu-web/                # wasm-bindgen、web-sys、Rust DOM/UI
  apps/
    debugger-web/             # 调试器页面入口
  examples/
    triangle/
    textured-cube/
    compute-vector-add/
  tests/
    golden/
    differential/
    conformance-webgpu/
    conformance-webgl/        # 后续 P8/P9
    browsers/
  xtask/                      # Rust 构建、serve、IDL 生成、测试编排
  docs/
    adr/
    support-matrix/
    architecture-plan.zh-CN.md
```

早期可以让 `glgpu-core`、`glgpu-shader`、`glgpu-raster` 保持较粗粒度；只有当依赖边界稳定后再继续拆 crate，避免一开始产生大量循环依赖和发布负担。

## 7. 对外 API 与兼容策略

### 7.1 Rust-native API：建议首发

提供 Rust 风格对象，同时保持与规范概念的一一映射：

```rust
let gpu = CpuGpuSubset::new(adapter_options).await?;
let device = gpu.request_device(device_desc).await?;
// create resources, encode commands, queue.submit(...)

// P7b 才增加：
// let mut gl = CpuWebGlContext::new(WebGlProfile::WebGl1, surface_desc)?;
```

优点：

- Rust 调用无需把每个热路径参数跨 Wasm/JS 边界来回转换。
- 页面、example 和调试器可以完全由 Rust/Wasm 编写。
- 可先验证核心语义，再承担 JS 原生对象兼容的额外复杂度。
- 核心可在 native test runner 中运行。

### 7.2 现有 JavaScript WebGPU/WebGL 代码

如果目标是“不改现有 JS 程序，替换 `canvas.getContext("webgl")` 或 `navigator.gpu`”，则必须增加 JS façade：

- 从 WebIDL 生成 JS 可见的方法、常量、descriptor 转换和 Promise 包装。
- WebGL 可选择显式注入 context，或包装 `HTMLCanvasElement.prototype.getContext`。
- WebGPU 建议显式注入 `gpu` provider；直接替换 `navigator.gpu` 的可靠性和品牌检查兼容性较差。
- `wasm-bindgen` 导出的对象不是浏览器原生 `WebGLRenderingContext/GPUDevice`，因此 `instanceof`、原生 brand check、属性描述符和某些框架探测不可能百分之百透明。
- 同一个 canvas 一旦绑定 2D context，就不能再绑定原生 WebGPU/WebGL context；模拟 context 必须持有 2D canvas，而不能假装同时拥有原生 context。

因此建议按顺序公开五个兼容级别：

1. `rust-native`：首要、完全受控。
2. `js-explicit-subset-adapter`：兼容常规方法调用和 typed arrays，但对象明确命名为项目 subset，不承诺标准品牌检查或全部 core capability。
3. `js-standard-shape-provider`：P6 只有 core capability 和 observable-semantics gate 通过后才开放；显式取得，不声称具有浏览器原生品牌。
4. `cts-harness-injection`：仅用于固定测试环境，把 provider 和 Canvas factory 注入 harness；测试文件保持不改，但注入差异必须公开。
5. `navigator.gpu-monkey-patch`：实验功能，仅在选定框架和浏览器矩阵中声明支持，不作为 P6 的必要出口。

P5 的 JS Adapter 合同选择 **main-thread wrapper → dedicated owner worker proxy**：应用仍可在主线程同步创建轻量 handle 和记录 encoder，wrapper 把冻结后的 descriptors/packets 发送给 worker；worker 拥有资源、Rayon kernels 和 CPU framebuffer。像素放在 SharedArrayBuffer 双缓冲里，用 Atomics 把 front 槽交给主线程 Canvas 2D `putImageData`。设计不使用 OffscreenCanvas，三缓冲也不是默认。迁移一个普通样例时需要替换两个显式注入点：`gpu provider` 与 `canvas-context factory`；只替换 `navigator.gpu` 不足以处理 `canvas.getContext("webgpu")` 和 2D context 所有权。P0 必须以一个端到端 mock 验证 `main wrapper → worker → Rayon → 主线程 Canvas 2D → Promise`，P5 再实现完整 proxy。

### 7.3 “Rust 控制页面”的工程边界

建议明确允许：

- 一个最小 `index.html`；
- `wasm-bindgen` 生成的 JS glue；
- 为 Wasm 初始化、线程 feature detection、worker 或 API façade 所需的极薄 JS；
- 所有 DOM 创建、事件监听、状态管理、样式注入、Canvas 控制和调试逻辑均在 Rust 中。

不使用 React、Vue、Svelte、TypeScript UI 层或 Node/Vite 运行时。构建和开发服务器由 Cargo `xtask` 完成，服务器负责返回 COOP/COEP 响应头。

如果要求“仓库中一行手写 JS 都不能有”，则只能优先支持 Rust-native 调用，不能承诺透明运行现有 JS WebGPU/WebGL 程序。

## 8. WebGPU 前端设计（当前主线）

### 8.1 对象和命令模型

最小**概念对象面**：

- `GPU`、`Adapter`、`Device`、`Queue`；
- `Buffer`、`Texture`、`TextureView`、`Sampler`；
- `ShaderModule`、bind group/layout、pipeline layout；
- render/compute pipeline；
- command encoder/buffer、render pass encoder、compute pass encoder；
- `GPUCanvasContext` façade，内部连接 Canvas 2D presenter。

P2–P5 的内部/公开类型统一使用 `CpuGpuSubset`、`CpuAdapter`、`CpuDevice`、`CpuQueue`、`CpuCanvasContext` 等项目命名；P2 仍是 internal alpha。上述标准拼写只表示语义映射，P6 wrapper 才可以在通过对应 gate 后公开标准形状与 WebIDL 行为。

WebGPU 调用先记录和验证，再将 command buffer 降低为不可变 packets。`queue.submit` 保留 command buffer 和 pass 内的严格顺序；只有在资源依赖分析证明安全时才并行不同 pass。

Rust 侧 registry 使用 generation handle。JS wrapper 只持有 handle，不把 `wasm-bindgen` 默认的 `.free()` 作为 WebGPU 资源生命周期语义。command buffer 对引用资源持有强引用；调用 `destroy()` 后先进入逻辑失效状态，实际内存要等到引用它的 queue serial 完成后再回收。

JS adapter 的 `GPUBuffer.getMappedRange()` 按规范需要返回普通 `ArrayBuffer`。多线程 Wasm 的资源存储位于 SharedArrayBuffer-backed memory，不能直接冒充该返回值；必须建立非共享 mapping staging，在 map 完成时复制到 staging，并在 write mapping `unmap()` 时复制回 CPU buffer。这个开销和映射区间重叠规则要进入 benchmark 与 validation 测试。

异步语义不能被“Rust 函数返回 Result”简单替代。JS adapter 最终需要正确表达：

- `requestAdapter` / `requestDevice` Promise；
- `mapAsync`；
- error scopes 与 uncaptured errors；
- device loss；
- async pipeline creation；
- queue completion。

Rust-native API 可以提供 Rust `Future`，JS façade 再转换为 Promise。

### 8.2 时间线与错误模型

WebGPU 的 content、device 和 queue 工作不是一条简单同步调用栈。实现应使用单调递增 serial 模拟时间线：

- content timeline：JS/Rust API 调用与 WebIDL 转换；
- device timeline：validation、对象创建、error scope；
- queue timeline：submitted command、map/copy 和完成通知。

多数 `create*` validation failure 不应粗暴同步 panic/throw；需要返回可继续传播的 invalid object，并按规范让错误进入 scope 或 uncaptured error。WebIDL 类型转换、非法 enum 等属于调用边界错误，应与 GPU validation 分开。

error 模型至少覆盖：

- `pushErrorScope` 的 validation/out-of-memory/internal filter；
- 嵌套 scope 由最内层匹配项捕获，单个 scope 只保留第一个错误；
- `popErrorScope` 等待此前相关 device work 后 resolve；
- 未捕获错误触发 `uncapturederror`；
- `device.lost` 以异步结果表达；
- async pipeline 失败通过 pipeline error reject；
- shader module 提供带源码位置的 compilation info。

调试器可以额外提供 `strictDebugBreak`、规范条款链接和同步内部断点，但不能因此改变公开 API 可观察到的异步顺序。

### 8.3 P2–P3 `CpuGpuSubset` 总目标与标准 façade gate

第一版命名为非标准 `CpuGpuSubset`：P2/P3 只通过 Rust-native API 获取，P5 才增加显式 `SoftGpuProvider` JS adapter。它在项目支持矩阵中公开 format/feature 缺口，但不冒充标准 `GPUAdapter`：WebGPU 没有任意“supported formats 列表”，而标准 adapter 必须满足锁定规范的最低 limits、基础 formats 和 capability guarantees，不能靠少报值删除 core 能力。

只有当相应 WebGPU feature level 的全部基础保证实现并通过 gate 后，才暴露标准形状的显式 provider 和 CTS harness injection。标准 limits 可以在规范允许范围内选择，但不能低于 required minima；可选 feature 则只在完整实现后报告。直接修改 `navigator.gpu` 仍是独立实验项，不因 P6 自动开放。

P2 与 P3 的执行合同不同，不能把异步 queue 语义假装已经提前完成：

| 阶段 | 执行/queue 合同 | 明确不包含 |
| --- | --- | --- |
| P2 Render Alpha | 仅供 Rust-native 与测试使用的 `ImmediateMainOwnerQueue`；`submit` 在主 owner 顺序执行，readback 返回直接结果；资源仍记录 access set 和临时 serial | worker owner、`mapAsync`、completion Future、标准 queue/task 时序 |
| P3 Render + Compute + Queue MVP | `SerialOwnerQueue`；优先 worker-hosted，按 submit serial 调度 bounded kernels；readback/map/completion/error 以 Rust Future 结算 | JS Promise/WebIDL/品牌行为，留给 P5/P6 |

P3 同时交付 Rust-native error-scope stack、uncaptured diagnostic queue、device-lost Future，以及 map/completion/error 与 submit serial 的关联；P5 只把这些既有状态映射成 JS Promise/Event，P6 再补齐规范边缘行为。

P2–P3 合起来的 `CpuGpuSubset` MVP 建议包含：

- buffer create、write、copy、map readback；
- 2D `rgba8unorm`、`bgra8unorm`、基础 depth 格式；
- texture view、nearest/linear sampler、基础 addressing；
- bind group 和显式 pipeline layout；
- WGSL vertex/fragment 子集；
- render pass clear/load/store，vertex/index draw；
- queue writeBuffer/writeTexture、submit；
- canvas configure/getCurrentTexture/unconfigure；
- 基本 storage buffer compute 与 `dispatchWorkgroups`；
- validation error、trace hooks 和最小 capture schema；完整 capture/replay 与 UI 属于 P4。

P2 的纹理采样只开放显式 LOD；需要隐式导数的 `textureSample` 在 quad VM 完成前报告不支持。该限制可以存在于非标准 subset，但必须在 P6 标准 façade capability gate 前消除。

模拟 `GPUCanvasContext` 内部实际持有 2D context：`getCurrentTexture()` 返回当前 CPU presentation texture，提交完成后由 Rust presenter 按 canvas update 时机复制到 ImageData。current texture 的获取次数、失效和 resize/configure 行为必须按锁定规范测试，不能让应用直接接触内部 Canvas2D 对象。

为保持 tile ownership 和结果可解释性，MVP render shader 暂不允许 writable storage buffer/texture side effects；只读 storage 可在实现 bounds/validation 后开放。compute workgroup 间的非原子数据竞争不承诺唯一结果：deterministic-debug profile 可串行化并标记为诊断策略，spec-like profile 的测试只验证规范允许结果集合。

后置功能：MSAA、query set/timestamp、external texture、compressed texture、大量格式、indirect draw/dispatch、完整 atomics/subgroups、所有 canvas color space。若其中任何一项属于锁定规范的 adapter core guarantee，它就同时阻断标准 façade，只是不阻断非标准 subset provider。

### 8.4 不建议一开始复用完整 wgpu-core

Naga 非常适合 WGSL，但直接把软件渲染器实现成 `wgpu-hal` 后端会把项目绑定到庞大且持续变化的内部接口。建议：

- MVP 使用 Naga + 项目自有 WebGPU validation/command model；
- 可复用 `wgpu-types` 中稳定且合适的 descriptor 类型，但保持转换边界；
- 在 P0 做一个 `wgpu custom backend`/`wgpu-core` 可行性 ADR；
- 若未来需要让现有 Rust `wgpu` 程序直接切换后端，再单独实现 compatibility adapter。

## 9. 后续 WebGL 前端设计

本节用于提前冻结公共内核的兼容边界，不代表当前实现顺序。P0 只做限时架构探针，P1/P2 做兼容性复核；ESSL 编译器从 P7a 开始、WebGL 状态机从 P7b 开始、JS adapter/正式 façade 从 P8 开始，当前产品主线见第 8 章。

webgl-mock 与 webgpu-mock 对等，见 [ADR 0003](adr/0003-parallel-webgl-mock.md) 和 [webgl-mock](webgl-mock.zh-CN.md)。它在独立 crate 里用 CPU 实现模拟 WebGL，不依赖 webgpu-mock 的 crate。原生 WebGL 对照页放在 demo 层，这是未来的设计，目前还未实现。页面上的三角形是 demo。当前 demo 页在主线程跑单线程 Wasm，并用 `putImageData` 直接交到页面 canvas。双缓冲是未来的设计，目前还未实现。2026-10-28 已决定主路径是 SharedArrayBuffer 双缓冲加 Atomics，逻辑绘制缓冲是可写的 back 槽，不使用 OffscreenCanvas，三缓冲不是默认。目前 demo 在主线程用 `Vec<Rgba8>` 或 webgpu framebuffer，再 `putImageData`。它还没有本节的状态机。

### 9.1 状态与对象

`WebGlContextState` 至少包含：

- buffer、program、framebuffer、renderbuffer、VAO、transform feedback 等绑定；
- active texture unit 以及每个 unit/target 的 texture 和 sampler；
- 每个 vertex attribute 的 enable、buffer、format、stride、offset、divisor 和 current value；
- viewport、depth range、scissor、front face、cull、line width、polygon offset；
- depth、front/back stencil、blend RGB/alpha、blend constant、color/depth/stencil masks；
- PACK/UNPACK alignment、flipY、premultiply alpha、colorspace conversion；
- context attributes、limits、extensions 和 error flags。

对象存储使用 `(context_id, slot, generation)` handle arena。删除对象后先 tombstone；只有绑定、attachment、packet 和 trace 中都没有引用时才真正回收，防止 use-after-free、跨 context 对象误认和延迟执行读到复用后的对象。

P1 公共资源层先提供安全 policy/capability，P7b 的 WebGL validator 再映射精确 API 行为：

- descriptor/offset/size 全部使用 checked arithmetic，并在分配前做配额检查；
- 公共分配支持零初始化与可配置 attachment 初值；`bufferData(size)`、无源 texture/renderbuffer 和 default depth 的精确映射属于 P7b；
- vertex/index/texture/shader 访问始终 bounds-check，不允许读到 Wasm allocator 的相邻内存；
- 公共层提供 OOB read/write policy hook；WebGL 锁定规范要求的返回值、丢弃与 `getError` 映射属于 P7b；
- 任何从 shader、trace 或 JS 传入的长度都不能直接用于不受控 `Vec` 扩容。

### 9.2 调用与 draw packet

每次 API 调用按以下顺序处理：

1. 参数和当前状态验证；
2. 按 WebGL 规则设置 error flag；
3. 同时生成不改变 WebGL 行为的详细诊断；
4. 更新状态或资源版本；
5. `draw*` 时冻结不可变 `DrawPacket`。

标准 `getError()` 只反映 WebGL 规范定义的错误。WebGL core 没有通用 `NOT_SUPPORTED`；对合法但尚未实现的 core 调用，JS subset façade 只能按版本化的 `intentionally-different` 合同处理，并通过独立 diagnostics/support API 报告。Rust-native API 则可直接返回 `Result::Unsupported`。在没有为某个缺口定义行为之前，不得暴露对应 JS 方法为“已支持 WebGL”。

一个 `DrawPacket` 必须包含：

- program/shader bytecode；
- vertex/index layout 与所引用 buffer 版本；
- uniforms、textures、samplers；
- primitive topology、index range、instance range；
- framebuffer attachment handle、当前可写 serial 和格式；
- viewport/scissor/cull/depth/stencil/blend/masks；
- draw ID、primitive ID 基准、trace/source metadata；
- resource read/write access set。

延迟执行时，资源更新采用 version + copy-on-write，或在修改前强制执行 barrier。旧 draw 绝不能观察到后续 `bufferSubData`、`texSubImage2D` 或 uniform 更新。

只给 tile 内图元排序并不足以保证跨 draw 可见性。调度器依据每个 packet 的 access set 分割 render epochs；RAW/WAR/WAW hazard、attachment→sample/copy/readback、FBO 切换、`readPixels`、copyTex、mipmap generation 都是全局 barrier。不可变输入可按 version/COW 快照；可写 render target 按 submit serial 原地演进，不能为每个 draw 盲目复制整个 framebuffer。

### 9.3 Drawing buffer 与呈现合同

context 创建时冻结请求属性与实际属性，并公开实际的 `alpha/depth/stencil/antialias/premultipliedAlpha/preserveDrawingBuffer`。首版可把不支持的 antialias 固定为 false，但返回值必须诚实。

- canvas intrinsic size 改变时，在明确的 API/frame boundary 重新分配 drawing buffer，更新 `drawingBufferWidth/Height`、viewport 相关诊断和 attachment 版本；
- Rust-native API 使用显式 `present()`；WebGL JS façade 在 rAF/end-of-task 附近调度 present，这只是对浏览器 compositing 时点的近似，列为 `intentionally-different`；
- `preserveDrawingBuffer=false` 时，present 后按锁定规范使 drawing buffer 内容失效/清除；capture 若要保存必须在此之前做 checkpoint；
- resize、allocation failure 和显式测试钩子可触发 context loss/restoration 状态机；
- ImageData 是非预乘表示。`premultipliedAlpha`、`alpha:false`、alpha=0、页面 compositor 的预乘规则需要显式转换和 golden tests，避免二次预乘。

### 9.4 WebGL 分阶段支持矩阵

P0–P2 的公共内核探针不是 WebGL API，也不计入任何 WebGL 支持级别。`CpuWebGlSubset` MVP 从 P7b 才通过 Rust-native 或显式 adapter 获取；只有 WebGL 1 core contract 达到单独 gate 后，才开放标准 façade/monkey-patch 实验。

| 功能 | `CpuWebGlSubset` MVP（P7b） | WebGL 1 Beta（P8） | WebGL 2 Track（P9） |
| --- | --- | --- | --- |
| primitive | triangles/indexed | points、lines、strip、fan | instancing、primitive restart |
| buffers/attribs | ARRAY/ELEMENT、常见类型 | 完整 WebGL1 vertex input | integer attrib、VAO、UBO、PBO |
| shader | ESSL100 Appendix-A 子集；内部 quad 采样 | 暴露 ESSL100 derivatives 扩展 | ESSL300 |
| textures | RGBA8 2D、non-mip nearest/linear、rho 判定 MIN/MAG | mip、cube、depth/float 优先扩展 | 3D、array、integer、sampler object |
| framebuffer | default color + depth | FBO/RBO、color/depth/stencil | MRT、blit、MSAA |
| tests | cull/depth/blend/masks | stencil、polygon offset、pixel store | WebGL2 全状态逐步加入 |
| readback | readPixels 子集 | pack 规则、FBO readback | PBO/多格式 |

`CpuWebGlSubset` MVP 明确不包含：MSAA、完整 mip LOD、cube/3D/array、压缩纹理、浮点 attachment、MRT、query/sync、transform feedback 和任意扩展。

WebGL 1 Beta 优先扩展：

- `OES_standard_derivatives`；
- `OES_vertex_array_object`；
- `OES_element_index_uint`；
- `ANGLE_instanced_arrays`；
- `WEBGL_depth_texture`；
- 之后再评估 float/half-float 与 anisotropic filtering。

任何扩展只有在语义和测试已实现后才报告支持。

## 10. Shader 子系统

### 10.1 统一流水线

```text
source
  → preprocess
  → parser + source spans
  → name/type/semantic validation
  → stage linking/reflection
  → project-owned typed IR
  → optimized register bytecode
  → scalar / quad / workgroup VM
```

公共 IR 不直接暴露 Naga 内部结构。这样可以固定 capture 格式和调试器 source map，也避免 Naga 升级直接破坏执行器。

### 10.2 WGSL

- 固定 Naga 版本进行 WGSL parse + validate。
- 将 Naga `Module` 通过薄 adapter 降低到项目 typed IR。
- 对未支持的 WGSL feature 给出带 source span 的明确 pipeline error。
- Naga 升级必须运行 IR snapshot、shader golden 和已移植 CTS vectors，不能自动漂移；未有标准 JS façade 前不把它表述为“官方 CTS 通过”。

### 10.3 后续 ESSL：WebGL 阶段的最大技术风险

当前 Naga 官方说明其 GLSL 输入是 GLSL 440+、Vulkan semantics，并不是 WebGL 所需的 ESSL100/300。`glsl-lang` 也以 GLSL 4.x 为主且预处理支持不完整。因此 ESSL 不能被当作“换一个 parser 就完成”。

推荐路线：

1. P0 只用 3–5 个代表性 ESSL shader 做手工 lowering，确认公共 IR、source map 与 capture schema 没有绑定 WGSL/Naga；该探针限时且不要求可用 parser。
2. P7a 再用官方 WebGL shader corpus 评估 lexer/AST 方案并冻结 ESSL100 实现路线，不让选型阻断 WebGPU 主线。
3. 分成两层：ESSL100 parser/type checker，以及叠加 WebGL 限制的 shader validator/linker；后者负责 Appendix A、资源 limits、安全和 WebGL 专有规则。
4. WebGL MVP 只开放明确列表中的语法与 builtins；遇到其他语法 compile fail 并指向源码位置。
5. ESSL300 作为独立阶段，不把 WebGL2 语法偷偷混入 ESSL100。
6. native CI 可调用 Khronos glslang 做 ESSL 差分、调用 ANGLE/WebGL 实现做 WebGL overlay 差分，但不要求把 C++ 编译器打入浏览器 Wasm 包。glslang 单独不能作为 WebGL 合法性的完整 oracle。

ESSL100 MVP 子集：

- ESSL source 类型 `bool/int/float`、vec、mat、array 子集、swizzle；降低到 IR 后才使用 `i32/f32` 等内部类型名；
- arithmetic、comparison、conversion、常用数学 builtins；
- uniform/attribute/varying；
- function、if，以及满足 GLSL ES Appendix A 静态可分析限制的 `for`；MVP 禁止 `while/do-while`，数组和循环索引遵守相应常量表达式规则；
- sampler2D、texture2D；MVP 以 fragment quad 计算 rho 来选择 MIN/MAG，但只执行 non-mip base level 过滤；
- fragment `discard`；
- precision qualifier 先按 f32 执行，并明确标记 `precision-emulation=relaxed`。

WebGL shader overlay 同时校验：

- `webgl_`/`_webgl_` 保留名和 WebGL 禁止/扩展 gating 的 constructs；
- token length、struct nesting、static-use 等安全限制；
- attribute/uniform/varying packing 和 profile limits；
- stage interface/link rules、global initialization 与 WebGL 要求的初始化行为；
- texture/extension/builtin 只能在当前 context profile 实际支持时使用。

### 10.4 VM 执行模型

- vertex：单 invocation，可批量并行。
- fragment：最终采用 2×2 quad lanes，维护 execution mask，支持 `discard`、helper lanes、derivatives 和隐式 LOD。
- compute：按 workgroup 调度；每个 invocation 执行到 barrier 后挂起，全部活跃 invocation 到达后继续下一轮。
- 所有 instruction 保留 source span。
- debug mode 可对选中 vertex/fragment/invocation 逐条记录 locals、inputs、outputs、memory access。
- shader 有 instruction/loop/memory budget，防止不可信代码锁死页面。
- 浮点默认严格 f32、关闭 fast-math；NaN/Inf、除零、越界和未初始化值可被 instrumentation 捕获。

需要实现的 VM 能力按顺序为：

1. scalar/vector/matrix 和 structured control flow；
2. uniform/storage/buffer access；
3. 2D non-mip base-level sampling 与显式 LOD；
4. P3 所需的 storage-buffer read/write、workgroup memory 与可 yield 的 barrier 调度；
5. P6 前完成语义正确的 scalar 2×2 quad、helper lanes、基础 derivatives/rho，再扩展到完整 implicit mip LOD；
6. storage texture/image write、atomics；
7. ESSL300/WGSL 高级 builtins。

SIMD 是 quad 语义正确之后的优化，不是 quad 的替代品。为缩短 WebGPU Render Alpha，P2 textured cube 明确使用 `textureSampleLevel(..., 0.0)`；隐式 `textureSample` 在 quad/derivative 能力通过 gate 前直接拒绝，绝不静默退化为 LOD0。quad 在 WebGPU standard-façade gate 和后续 WebGL `texture2D` 之前必须完成。

## 11. CPU 光栅管线

### 11.1 阶段

1. draw validation 与 vertex/index fetch；
2. vertex shader；
3. primitive assembly；
4. homogeneous clipping；
5. perspective divide、viewport/depth transform；
6. front-face、cull；
7. triangle setup、window-depth gradients、polygon offset 和 tile binning；
8. coverage、varying interpolation、fragment shader；
9. stencil/depth、blend、write mask、format conversion；
10. attachment resolve/present conversion。

### 11.2 裁剪与坐标约定

- WebGL clip Z 为 `-w..w`，WebGPU 为 `0..w`；用 `RasterConvention` 参数化，不能硬编码成一种。
- 在 clip space 对六个平面做齐次裁剪，连同 varyings 一起插值。
- 内部 attachment 的 row layout 不暴露给 API；通过 addressing layer 处理 WebGL 左下原点、WebGPU/Canvas 上方原点和 readback 规则。
- fragment 可见的 `gl_FragCoord`/position 必须按对应规范计算，不能直接使用内存行号。
- winding/culling、provoking vertex、flat interpolation 必须有专项测试。

### 11.3 三角形光栅化

- 使用 fixed-point edge equations 和确定性的 half-open/top-left ownership 规则。
- sample center 首版为 `(x+0.5, y+0.5)`；subpixel bits 固定在 profile 中。
- smooth varying 使用 `Σ(λᵢ·vᵢ/wᵢ) / Σ(λᵢ/wᵢ)` perspective-correct interpolation；window depth 使用 screen barycentric 线性插值，不能复用 varying 公式；`gl_FragCoord.w = Σ(λᵢ/wᵢ)`。三者分别有 golden tests。
- polygon offset 在 setup 得到 `dz/dx`、`dz/dy` 后计算，作用于 depth-test candidate；若 shader 按扩展/后续版本写 fragment depth，则按锁定规范处理其覆盖与量化次序。
- 三角形 bbox 被分配到 16×16 起始 tile；8/16/32 通过 benchmark 选择。
- 每个 tile 的 primitive list 按 `(submit_id, draw_id, primitive_id)` 稳定排序。
- tile 独占对应 color/depth/stencil 像素，线程间不需要逐像素锁。
- binning 使用 checked bbox、分 epoch/chunk 处理并有独立引用预算，防止全屏高 overdraw 形成 `primitive_count × tile_count` 的无界列表。NaN/Inf 或无法安全转为 fixed-point 的位置按 profile 丢弃并给出诊断，不能发生整数溢出。

### 11.4 逐片元操作

正确性优先的首版采用保守顺序：

- coverage/scissor；
- fragment shader/discard；
- stencil test 与相应 op；
- depth test/write；
- blending；
- color write mask 与格式量化。

只有静态分析证明 shader 不会 `discard`、写 depth 或产生相关 side effect 时，才启用 early depth/stencil 优化。

P1 只保留可插拔的 post-fragment/dither policy hook，当前 WebGPU 路径不启用 WebGL `DITHER` 状态。P7b/P8 实现 WebGL 时，`DITHER` 是可查询、默认 enabled 的 core state；若 profile 选择确定性的 no-op dither，`enable/disable/getParameter` 仍须保持状态，并在支持矩阵列为 `intentionally-different`。

首批内部格式：

- RGBA8 UNORM color；
- D24 或 D32F depth；
- S8 或 D24S8 stencil；
- shader 运算使用 f32，写回时按格式 clamp/round/encode。

sRGB、float attachment、integer attachment、MSAA 和多 render target 后置，但格式系统一开始必须可扩展，不能把所有纹理都等同于 `Vec<u8>` RGBA。

## 12. 纹理系统

建议模型：

```text
Texture
  → target/dimension
  → mip level
  → face/layer
  → Format + extent + byte storage

TextureView + SamplerState
```

WebGL 1 可把 sampler state 放在 texture object；WebGL 2/WebGPU 使用独立 sampler/view 抽象，再由前端映射。

实现顺序：

1. P1：RGBA8 2D storage、base level、通用 row-layout/format-conversion hook；
2. P2：WebGPU `writeTexture`/copy 布局、texture view/sampler、显式 LOD、CLAMP/REPEAT/MIRROR、non-mip NEAREST/LINEAR；
3. P3：compute 所需的 storage-buffer 路径；storage texture 继续后置；
4. P6 前：fragment quad rho、MIN/MAG、mip chain、mip filters 与完整 implicit LOD；
5. P6/P10：depth、float/half/sRGB、3D/2D-array/integer/storage/compressed formats 按 capability gate 加入；
6. P7b：WebGL PACK/UNPACK、pixel-store、flipY/premultiply/colorspace 与 NPOT completeness；P8 再加入 cubemap orientation 和 FBO feedback rules。

P1/P2 必须专项测试 texel center、normalized coordinates、wrap edge、WebGPU row layout/copy validation 和 read/write format conversion；NPOT completeness 从 P7b、FBO feedback loop 从 P8 才进入正式门禁。

## 13. 多线程执行模型

### 13.1 数据并行策略

一次 submit/draw batch 分成有 barrier 的阶段：

1. API validation/packet freeze：单线程、有序；
2. 根据 packet resource read/write sets 建立无 hazard 的 execution epoch；
3. vertex shading：按 vertex/index chunk 并行；
4. primitive setup/binning：thread-local bins，并行生成后 stable merge；
5. tile raster：`tiles.par_iter_mut()`，tile 间并行；
6. compute：workgroup 间并行，workgroup 内按 barrier 语义调度；
7. presentation conversion：按 scanline/tile 并行；
8. Canvas API 调用：只在 context owner 线程执行。

禁止直接 `par_iter` 所有 primitives 后同时写 framebuffer。深度、模板和混合依赖顺序，乱序会得到错误且不稳定的结果。

WebGPU 额外使用一个**由 Dedicated Worker/owner event loop 驱动的串行 queue dispatcher**：对调用方尽快返回，owner 严格按 submit serial 启动作业；Rayon 只执行有界 CPU kernels，绝不能用一个长期阻塞的 Rayon task 充当 dispatcher，否则单 worker、嵌套并行和 channel wait 都可能饥饿/死锁。kernel 完成后通过 `(serial, result)` worker message 回 owner，由 owner 按任务顺序 settle Future/Promise；不能让任意 Rayon worker 直接操作 MessagePort、Promise 或 DOM，也不能只依赖隐藏标签页可能暂停的 `requestAnimationFrame`。

P0 queue spike 至少覆盖：1-worker 无死锁、连续多个 submit、`queue.submit` 返回延迟、`mapAsync`/`onSubmittedWorkDone`/`popErrorScope` 顺序、hidden tab、device loss、取消和 worker 关闭。若 owner-worker + Rayon 无法可靠组合，P3 只能先交付 main-owner 的显式异步 `CpuGpuSubset`；P5 的 JS adapter 与 P6 的标准 façade 继续被 gate 阻断。

### 13.2 确定性规则

- command、pass、draw 和 primitive 都有单调 ID。
- tile 合并保持 ID 顺序。
- hot path 不使用抢锁顺序决定结果。
- 对无 storage race/跨 invocation side effect 的 workload，deterministic-debug profile 在顺序执行器与不同 worker 数下 output hash 必须相同。
- f32 不启用 fast-math。
- WebGPU 非原子 storage race、跨 tile shader side effect 和规范允许多结果的 atomics 不承诺唯一 hash；调试 profile 可提供“确定性串行化模式”，同时标记这是诊断策略而非硬件时序。spec-like mode 验证允许结果集合。

### 13.3 `wasm-bindgen-rayon` 约束

当前维护版通过 Web Workers + SharedArrayBuffer 运行 Rayon。构建和部署需要：

- `--target web`；
- 固定 nightly 并用 `-Z build-std` 重建支持 atomics 的标准库；
- `+atomics,+bulk-memory` 和 shared/imported memory linker 配置；
- 页面处于 cross-origin isolated 环境；
- 常见响应头为 `Cross-Origin-Opener-Policy: same-origin` 和 `Cross-Origin-Embedder-Policy: require-corp` 或 `credentialless`；
- 启动后异步调用 `initThreadPool`；
- threads 不可用时选择独立的单线程构建产物。

`wasm-bindgen-rayon` 构建的是进程/实例内全局 pool：同一 Wasm 实例只能成功初始化一次，worker 数不能运行时重建。`hardwareConcurrency <= 1` 时直接选择 ST；MT 初始化数量至少为 1。UI 中“顺序/并行切换”只切换执行器，若要改变实际 worker 数，必须新建 Worker/Wasm 实例或刷新页面；1/2/4/8 测试也使用独立实例运行。

不要把 Rayon 直接散布到所有 crate。定义内部 `ParallelExecutor`/feature boundary，以便未来替换为稳定 Wasm threads API 或其他 worker runtime。

### 13.4 两种运行模式

**模式 A：主线程集成模式（P0–P2 与永久 fallback）**

- Rust/Wasm 主实例拥有 API、调试 UI 和 Canvas 2D。
- CPU 阶段通过 Rayon worker pool 并行。
- 后续 WebGL 同步 readback/finish 最容易保证语义；这只是 P7b+ 约束，P0–P2 不实现调用。
- 缺点是主线程在等待软件渲染时仍可能卡顿。
- 适合 P0–P2 和 Rust-native 同步调试；若 WebGPU queue 无可靠 owner-worker dispatcher，只能暴露显式 async subset，不能声称标准 queue 行为。

**模式 B：worker-hosted owner 模式（WebGPU 主线，P0 验证、P3 交付）**

- 主线程 Rust/Wasm 只负责调试 UI。
- 资源与渲染 owner 在 Dedicated Worker。逻辑绘制缓冲是 SharedArrayBuffer 双缓冲里可写的 back 槽，主线程呈现 front 槽。不使用 OffscreenCanvas。Rust-native 应用可以同驻 worker，P5 的 JS 应用也可以留在主线程并通过 wrapper/packet proxy 调用 owner。
- owner 再用 Rayon worker pool 并行 tile。
- 后续 WebGL 的同步调用可发生在应用 worker 内而不阻塞页面 UI；这只是 P7b+ 设计约束。
- “worker 内再初始化 wasm-bindgen-rayon pool”必须在 P0 做跨浏览器 spike，验证后才能承诺。
- P3 的 queue/Future 和 P5 的 queue/Promise 模型以此模式通过压力测试为前提。

仅作 P7b/P8 设计约束：对现有主线程 JavaScript WebGL 程序，不能简单把所有工作异步丢给另一个 worker；`readPixels`/`finish` 是同步 API，而浏览器主线程不能用 `Atomics.wait` 等待。可采用 packet 延迟到 frame/sync point 后在主实例并行执行，但必须严格处理资源版本和 UI 卡顿。

## 14. Canvas 2D 呈现

### 14.1 多线程路径不可消除的共享内存复制

多线程 Wasm framebuffer 位于 SharedArrayBuffer-backed linear memory。当前 `ImageData` 的 IDL 使用未标注 `[AllowShared]` 的 `Uint8ClampedArray/Float16Array`；Web IDL 因而不接受以 SharedArrayBuffer 为 backing 的 typed array。结果是：

1. Rayon 在共享 Wasm memory 中完成渲染；
2. present 阶段必须把像素复制到非共享 `ImageData.data`；
3. 再调用 `putImageData` 更新 Canvas bitmap。

1080p RGBA8 单帧为 7.91 MiB；仅按 60fps 把共享 framebuffer 复制到 ImageData 就约 0.5 GB/s。4K RGBA8 单帧约 31.6 MiB，60fps 约 1.85 GiB/s，还未计算 Canvas 内部更新。因此这是首要性能风险，不应宣称“零拷贝”。

这项显式 staging copy 只对 MT/SAB 路径是强制的。ST build 的普通 ArrayBuffer-backed Wasm memory 可以在内存稳定时直接构造 ImageData view，从而省去 shared→ImageData 这一步；`putImageData` 更新 Canvas bitmap 的成本仍存在，且 memory grow 后必须重建 view。

### 14.2 推荐呈现路径

这一节是未来的设计，目前还未实现。目前 webgl-mock 与 webgpu-mock 都在主线程把 CPU framebuffer 交给 `putImageData`。

- 这是未来的设计，目前还未实现。2026-10-28 已决定：默认是一块 SharedArrayBuffer 的双缓冲。front 给主线程呈现，back 给 Worker 光栅，交接用 Atomics（帧序号，以及哪一块可读）。逻辑绘制缓冲就是可写的 back 槽，不为每一帧再拷一份全帧逻辑 framebuffer。三缓冲不是默认，只有测到呈现停顿才考虑。目前 webgl-mock 用 `Vec<Rgba8>`，webgpu-mock 用 `glgpu-core` 的 framebuffer，两边都由主线程 `putImageData`。
- 不把这唯一的 SharedArrayBuffer transfer 走。主线程呈现 front 槽时，若 `ImageData` 不能接收共享视图，只从该槽复制。
- Rayon 并行完成 origin flip、format、sRGB/alpha 和 debug overlay，写入 `WRITING` 的共享 presentation buffer；普通 ArrayBuffer-backed ImageData 只能由 owner 线程复制，不能让 Rayon 直接并行写。
- full-frame 模式复用一个普通 ImageData，执行等价于 `imageData.data.set(sharedView)` 的整帧复制；resize 时才重建。
- dirty 模式不能先做整帧 `.set`。它按合并后的矩形逐行局部复制到 ImageData，再对相同矩形调用 `putImageData`；普通 3D 场景可能仍然全屏 dirty。
- 复用 framebuffer、staging array 和 ImageData，避免逐帧分配。
- 默认一次 full-frame `putImageData`；只有 benchmark 证明更快时才使用少量 dirty rectangles。
- 跟踪 dirty tiles，合并成少量矩形或一个 bounding rectangle。
- 调试器允许降低内部渲染分辨率并用 CSS 显示缩放。
- 呈现主路径是 SharedArrayBuffer 双缓冲，不使用 OffscreenCanvas。`ImageData` 不能直接吃共享视图时，主线程只从当前 front 槽复制，再 `putImageData`。

benchmark 分别统计：format/present conversion、shared→ImageData copy、`putImageData` 三段；P0 还要构造“renderer 明显快于 presenter”的压力场景，验证不会撕裂或同时读写同一 buffer。

`putImageData` 会直接替换像素，不受 transform、globalAlpha、composite operation 和 shadow 影响，适合作为确定性的最终呈现接口。

## 15. 调试器能力

### 15.1 WebGPU Debugger Beta（P4）必须有

- API call 序列、descriptor、对象 label、耗时、error 和详细 validation message；
- resource、binding、pipeline、encoder、pass 和 command buffer 的状态与依赖快照；
- frame/draw capture 与确定性 replay；
- 资源创建/更新/删除版本，buffer/texture 内容 hash；
- buffer inspector：按 vertex/index/uniform/storage layout 解码；
- texture/attachment viewer：mip、face、layer、color、depth、stencil；
- shader module compilation info、pipeline diagnostics、typed IR、bytecode 和 source mapping；
- command buffer/queue serial timeline，以及 `mapAsync`、completion、error scope、uncaptured error 和 subset/internal device-loss event 的结算顺序；P6 才要求标准可观察 device-loss 语义；
- 选中 vertex/fragment/compute invocation 的指令 trace；
- selected-pixel history；
- overdraw、primitive ID、tile occupancy、depth、NaN/Inf 等 overlay；
- stage timing、shader instruction count、采样次数、复制字节数等 CPU 指标。

WebGL 的 global state diff、FBO、program compile/link 和 `getError` 专用面板在 P7b/P8 叠加到同一 capture/debugger 基础设施上，不进入 P4 发布门。

### 15.2 Pixel history 数据

对一个像素/样本记录：

- 哪次 clear/draw/primitive 触及；
- clip/cull/bbox/coverage/scissor 是否通过；
- 插值输入和 shader 输出；
- 是否 discard；
- stencil before/test/op/after；
- depth before/test/after；
- blend src/dst/factors/result；
- color mask、format conversion 和最终颜色。

这项能力应在架构早期植入。若到最后再加，光栅和 shader VM 很可能没有足够的 source/value metadata。

### 15.3 Capture 格式

建议 `.glgputrace` 为版本化容器：

- header：schema、spec lock、build、profile、endianness；
- ordered event stream；
- content-addressed resource blobs；
- shader source/IR/bytecode；
- 可选 framebuffer checkpoints；
- diagnostics 和 performance counters。

MVP 可先使用可读 JSON metadata + 独立二进制 blobs；稳定后改为 postcard/CBOR 等紧凑格式。所有 reader 必须验证长度、预算和版本，防止恶意 capture 导致 OOM。

## 16. 正确性和测试计划

### 16.1 测试层次

1. **纯 Rust unit tests**：format、packing、sampling、clip、edge、blend、state validation。
2. **property tests**：共享边无裂缝/重绘、资源 bounds、state transition、无 race workload 的 single/multi 一致性。
3. **shader snapshots**：source → AST/IR/bytecode/reflection/diagnostic。
4. **golden images**：小尺寸 exact coverage；浮点/过滤按明确 tolerance。
5. **differential tests**：当前主线与数学 oracle、Naga 和可选原生 WebGPU 结果比较；P7a 加入 glslang shader oracle，P7b/P8 再加入 ANGLE/原生 WebGL。
6. **suite vectors**：先把 Khronos/WebGPU CTS 的独立测试向量移植到 Rust core；这不能表述为官方 suite 通过。
7. **official suite gates**：P6 在固定 CTS harness injection 下运行未修改的 WebGPU CTS test files，公开 harness diff、commit、query prefix、用例数和通过率；WebGL 的未修改 Conformance Suite 到 P8/P9 再运行。未满足原生品牌/环境要求时，不把 harness 结果表述成官方 conformance certification。
8. **browser tests**：Chromium、Firefox、Safari；single-thread、multi-thread、main/worker mode。
9. **fuzzing**：先覆盖 WGSL、WebGPU API call stream、descriptor 与 capture decoder；P7a 加入 ESSL，P7b 加入 WebGL call stream。

### 16.2 合规报告

每个 API/format/extension/feature 公开四态：

- `implemented`；
- `partial`；
- `unsupported`；
- `intentionally-different`。

每项链接到规范条款、实现 issue 和测试。未通过完整 suite 前，不使用“WebGL conformant”或“WebGPU conformant”表述。

### 16.3 分阶段验收样例

| 阶段 | 固定验收 |
| --- | --- |
| P0 | solid triangle；最小 coverage/depth history；独立 1/2/4/8-worker 实例 hash 一致；present 无撕裂；端到端 mock queue 通过；提交有界 WebGL future-proofing fixtures，但不实现 WebGL API/parser |
| P1 | 固定 shader 的 indexed/depth/blend/render-to-texture；资源 hazard 与双 `RasterConvention` goldens；公共 packet/debug schema 兼容复核 |
| P2 | internal Rust-native WGSL indexed textured cube（显式 LOD）；render-to-texture/readback；source→IR→bytecode/pixel trace；用不超过 1–2 人日重跑 IR/trace future-proofing freeze check |
| P3 | 首个公开 `CpuGpuSubset` Render + Compute + Queue MVP；vector add、workgroup shared-memory reduction 与多 barrier；submit/map/completion/error/device-loss Future 顺序正确 |
| P4 | 选中像素和 compute invocation 可完整解释；queue/error timeline 可检查；8–10 类故障 capture/replay |
| P5 | 筛选过的 JS WebGPU 样例替换 namespaced provider + Canvas factory 后运行；只用显式 LOD/已支持 feature；WebIDL、invalid object、Promise/error scope 顺序正确 |
| P6 | `G6-capability`、`G6-observable`、`G6-cts` 三门均有证据；锁定 capability matrix、harness diff、CTS commit/query/用例数/通过率 |
| P7a | 锁定 ESSL100 grammar/builtin 列表；WebGL Appendix-A shader corpus 通过率；负例 source-span diagnostics |
| P7b | depth occlusion、culling、alpha blend/color mask、readPixels；只使用 `CpuWebGlSubset` API/texture/filter 列表 |
| P8 | stencil portal、FBO render-to-texture、mip/cube 和 WebGL1 Beta suite vectors |
| P9 | ESSL300、MRT/UBO/3D-array texture 等逐 gate 的 WebGL2 样例 |
| P10 | 每版 support report、固定 runner 性能回归和新增 suite gate 可复现 |

所有阶段都要求 unsupported shader/API 得到稳定、带源码/调用位置的独立诊断；standard façade 的规范 error 行为不得被项目诊断污染。

## 17. 性能与内存计划

### 17.1 优化顺序

1. 建立 stage benchmark 和 correctness hash；
2. tile ownership 去锁；
3. 复用 packet、bins、attachments 和 ImageData staging；
4. vertex cache、batch fetch；
5. 语义正确的 scalar quad shader VM；
6. Wasm SIMD128 的 4-lane fragment/math；
7. dirty tile present；
8. 只有 profile 证明有价值时才做 bytecode specialization/JIT-like codegen。

不建议早期动态生成并执行新的 Wasm shader module：它会显著增加 CSP、异步编译、source map、缓存和调试复杂度。解释器 + SIMD 是更适合第一版的基线。

### 17.2 基准项目

- clear/fill 与 present copy GB/s；
- small/large triangle throughput；
- vertex shader invocations/s；
- fragment bytecode instructions/s；
- nearest/linear texture samples/s；
- depth-only、blend-heavy、overdraw-heavy；
- tile size 8/16/32；
- 1/2/4/8 threads speedup 和一致性；
- 720p/1080p full-frame 与 dirty-frame Canvas2D 提交。

数值帧率目标应在 P0 指定参考机器和场景后再冻结，不能用模糊的“流畅”作为验收。

### 17.3 内存预算

- shared Wasm memory 需要声明最大值，不能无限增长。
- 建议默认软预算先设 256 MiB，构建最大值先以 512 MiB/1 GiB 做浏览器 spike 后决定。
- framebuffer、depth/stencil、resource、trace 各自计费。
- 默认 capture 使用有界 ring buffer；大 blob 内容寻址去重。
- 所有 descriptor/shader/capture 尺寸先做 checked arithmetic 和配额预检；资源容器使用 `try_reserve` 等 fallible allocation，避免普通 `Vec` 扩容或 `memory.grow` failure 直接 trap。
- 超预算时当前主线返回 `GPUOutOfMemoryError`/项目 OOM，P7b 再映射 WebGL OOM；OOM 不默认等同 device/context lost，只有规范或恢复策略要求时才进入 lost 状态。不能崩溃或静默截断。
- 避免在 JS view 存活时依赖 Wasm memory grow；所有 present view 每帧重建或使用稳定 staging 方案。

## 18. 构建、部署和 CI

### 18.1 构建产物

- `glgpu-web-mt`：shared memory、Rayon；宿主部署必须提供 COOP/COEP；
- `glgpu-web-st`：普通 Wasm memory、单线程 fallback；
- `glgpu-native-replay`：native Rust trace replay 和测试 oracle；
- `glgpu-debugger-web`：Rust/Wasm UI。

Cargo features 建议：

- `webgpu`；后续增加 `webgl1`、`webgl2`；
- `parallel`；
- `trace`；
- `shader-debug`；
- `strict-validation`；
- `native-oracle` 只用于开发/CI。

### 18.2 开发服务器

由 Rust `xtask serve` 提供静态文件和：

```http
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

生产部署还需检查所有跨源图片、字体、Wasm、worker 和 source map 的 CORS/CORP 策略。页面启动时显示：

- `crossOriginIsolated`；
- threads/SharedArrayBuffer/SIMD 可用性；不把 OffscreenCanvas 列入呈现设计；
- 选中的 mt/st build；
- worker 数与 resource budget。

多线程生产页面还应运行在 HTTPS 等可信安全上下文；localhost 仅用于本地开发。HTML、Wasm、worker script/response 都必须处于相容的 COEP/隔离环境，不能只给入口 HTML 加头。无 bundler 构建启用 `wasm-bindgen-rayon/no-bundler` 时，worker helper 可能使用 blob URL；Content Security Policy 需要实测 Wasm 编译权限并至少允许相应的同源脚本和 `worker-src 'self' blob:`。COOP 可能影响 OAuth/payment popup 的 opener 关系，COEP `require-corp` 可能拦截没有 CORS/CORP 的第三方资源；iframe 嵌入还要求父子隔离链，并确保 `Permissions-Policy: cross-origin-isolated` 没有阻断。库本身无法替宿主设置响应头，因此单线程构建必须是一等公民。

推荐加载顺序：

1. 检查 cross-origin isolation 与 Wasm threads feature；
2. 选择 mt 或 st 包，避免先实例化 mt 再失败回退；
3. 初始化 Wasm；
4. mt 模式 `await initThreadPool(n)`；
5. 创建 Rust UI、API facade 和 presenter；
6. 显示最终启用的能力矩阵。

worker 数由用户在实例初始化前覆盖；`hardwareConcurrency <= 1` 直接选 ST，否则默认值可从 `max(1, hardwareConcurrency - 1)` 起步并设置合理上限。最终值必须由 P0 benchmark 决定，因为浏览器可能降低 `hardwareConcurrency`，而更多 worker 也会增加栈和调度内存。初始化完成后改变实际 worker 数需要重建实例。

### 18.3 CI 矩阵

- stable native core tests；
- pinned nightly Wasm single-thread build/test；
- pinned nightly Wasm multi-thread build/test；
- Chromium + Firefox headless，带 COOP/COEP test server；
- Safari 定期实机/CI；
- shader/capture fuzz；
- conformance support report；
- benchmark 只在固定 runner 上做回归门禁。

## 19. 分阶段路线图与人力估算

以下估算按一名熟悉 Rust/Wasm 和图形管线的全职高级工程师，且不包含追求完整合规。P0 后应重新估算。

| 阶段 | 时间 | 主要交付 | 出口条件 |
| --- | ---: | --- | --- |
| P0 风险验证 | 4 周 | WebGPU-first 平台/queue/WGSL spike；WebGL 共用边界验证上限 20% | WebGPU G0 数据齐全；WebGL 兼容检查不阻断主线，除非暴露公共架构错误 |
| P1 公共内核 | 4–6 周 | resources、packets、RGBA8/D24、固定 VS/FS、indexed/depth/blend/render-to-texture、Canvas2D | 场景/hazard/双 `RasterConvention` goldens 通过；packet/debug schema 兼容复核；single/multi hash 一致 |
| P2 internal `CpuGpuSubset` Render Alpha | 6–10 周 | WGSL VS/FS→IR/VM、buffer/texture/view/sampler、bind group/pipeline、render pass、depth/blend、`ImmediateMainOwnerQueue` | Rust-native textured cube/render-to-texture/readback；明确 shader/format 子集；1–2 人日 IR/trace freeze check 通过；不构成首个公开 API |
| P3 `CpuGpuSubset` Render + Compute + Queue MVP（首个公开 API） | 6–10 周 | compute/storage/workgroup barrier、`SerialOwnerQueue`、mapping staging、resource serial lifetime、error-scope/uncaptured/device-lost Future | vector add + reduction；1-worker、多 submit、map/completion/error/hidden-tab/teardown 顺序正确；记录 G3 拓扑结果 |
| P4 WebGPU Debugger Beta | 6–10 周 | capture/replay、resource/binding/pipeline、WGSL/pixel/invocation trace、queue timeline | 定位 8–10 类预设错误；真实小型 Rust/Wasm WebGPU-style 样例可调试 |
| P5 Namespaced JS Adapter Preview | 8–14 周 | main-thread wrapper → owner-worker packet proxy、`createCpuGpuProvider()`、Canvas factory、descriptor/typed-array/Promise/invalid-object/error-scope | 筛选样例替换 provider + Canvas factory 即运行；只用显式 LOD/已支持 feature；冻结 G6 matrix/query；不注入 `navigator.gpu` |
| P6 WebGPU-Compatible Façade Candidate | 16–28 周 | 锁定 feature level，补强制 limits/formats/operations、quad/implicit LOD、WebIDL、task/error/device-loss/current-texture 语义、CTS harness injection | `G6-capability/observable/cts` 全通过；未过 gate 时仍不得称 conformant；`navigator.gpu` patch 仍为实验项 |
| P7a ESSL100 Compiler Alpha | 8–12 周 | preprocessor/parser/type checker、Appendix-A WebGL validator、link/reflection | 固定 grammar/builtin/limit 合同、shader corpus 比例和负例门禁 |
| P7b `CpuWebGlSubset` MVP | 8–12 周 | WebGL state/API validation、buffers、texture、depth/blend/readPixels | 固定 API 列表和 goldens；复用公共 renderer/debugger |
| P8 WebGL 1 Beta | 8–14 周 | primitive/FBO、mip/cube、stencil、优先 extensions、JS adapter | 真实样例集和 WebGL1 支持矩阵达到目标 |
| P9 ESSL300 + WebGL 2 Track | 16–28+ 周 | ESSL300、VAO/UBO/MRT/3D-array/integer texture 等逐 gate | 每项 API/format 有独立规范与测试出口 |
| P10 合规/性能 | 持续 | official suites、SIMD、更多 formats/features/extensions | 每版发布 support report 与固定 runner benchmark |

累计时间按一名高级工程师估算：`CpuGpuSubset` Render Alpha 约 4–6 个月；含 Compute + Queue 的 MVP 约 5–8 个月；MVP + WebGPU Debugger Beta 约 7–10 个月；JS Adapter Preview 约 9–14 个月；WebGPU-Compatible Façade Candidate 约 13–21 个月。之后增加 WebGL 1/2 约需 10–17 个月以上。完整标准合规仍不在这个估算内；三人团队可并行 shader/runtime/debugger，但不能简单按三等分工期。

P6 的三个内部 gate 不允许用一个很小的 CTS query 集形式通过：

- `G6-capability`：预先登记的 mandatory limits、formats、operations 与 shader capabilities 矩阵全部为绿；
- `G6-observable`：WebIDL conversion、task/Promise/error、device loss、mapping、current texture 和 Canvas 生命周期样例全部通过；
- `G6-cts`：P5 出口时预先登记的代表性 CTS query 集达到固定通过率，并公开 harness injection 差异。

`G3-core` 允许 main-owner fallback，因此 P4 调试器可以继续但必须标记 UI 可能阻塞；`G3-owner-worker` 要求跨线程 Future/teardown 压测通过，是 P5/P6 的硬前置。

## 20. P0：前四周的具体执行计划

### 第 1 周：工程和规范锁

- 初始化 Git 和 Cargo workspace；
- 固定 Rust nightly、wasm-bindgen-rayon 和 Naga；
- 优先锁定 WebGPU/WGSL、Canvas/WebIDL 与 Wasm threads 基线，建立 `spec-lock.toml` 与 ADR 模板；
- Rust `xtask serve` 返回 COOP/COEP；
- Rust/Wasm 通过 `web-sys` 创建页面、Canvas 和能力诊断面板。

### 第 2 周：线程与呈现 spike

- mt/st 两套构建；
- 用独立 Wasm/Worker 实例分别测试 1/2/4/8 Rayon workers 写共享 RGBA buffer；
- 测量 720p/1080p shared framebuffer → ImageData → putImageData；
- 测 dirty rectangle、双缓冲与 Atomics 交接、不同 tile size；三缓冲只在测到呈现停顿时再测；
- 验证 Chrome/Firefox MT；Safari 只要求 ST smoke/fallback，MT 结果单独记录；
- 测 main owner 与 Worker 后端；呈现是 SharedArrayBuffer 双缓冲到主线程 Canvas 2D，不测 OffscreenCanvas；
- 用 mock kernels 验证 owner queue dispatcher：1-worker、连续 submit、hidden tab、取消与 teardown；
- 完成一次 throwaway 端到端链路：main wrapper 记录 handle/packet → owner worker → Rayon kernel 写入 SAB back 槽 → Atomics 交接 → 主线程从 front 槽 `putImageData`。不经过 OffscreenCanvas，也不每帧再拷一份逻辑 framebuffer。

### 第 3 周：shader 与 API spike

- Naga WGSL parse/validate，并把最小 vertex/fragment/compute 函数降低到项目 typed IR；
- 手工 bytecode VM 执行 vertex + fragment，验证 instruction budget 与 source span；
- 生成一次性 WebGPU explicit-provider 风险 spike，验证 `createBuffer`、descriptor/WebIDL 转换、error scope、queue completion 和 Promise 顺序；它不是 P5 的生产 adapter；
- 验证 subset provider 与标准对象在 `instanceof`/brand check/capability guarantees 上的边界；
- 将 WebGL 兼容探针严格限制在 P0 总工时上限 20%：手工 lowering 3–5 个多特征 ESSL fixtures、构造 WebGL/WebGPU 双 `RasterConvention` goldens、把一组代表性 WebGL state/hazard/sync-readback 映射到公共 packet/trace；不实现 ESSL parser 或 WebGL façade。

这 3–5 个 shader fixture 合起来至少覆盖 VS+FS、precision、attribute/varying/stage IO、uniform/reflection、matrix/array/swizzle、静态 loop、`texture2D`、`discard`、`gl_Position`/`gl_FragColor` 和 source spans。trace fixture 固定一条可 round-trip 的链：`API call id → state delta/validation provenance → immutable DrawPacket → shader/pixel events`；event envelope 至少含 frontend tag、schema version、extension payload，以及 source/call/packet parent IDs。

### 第 4 周：最小软件三角形

- fixed-point edge raster；
- 16×16 stable tile bin；
- Rayon tile execution；
- RGBA8 + depth；
- 复核 IR、packet、raster convention 和 trace schema 不依赖 Naga/WGSL 专有结构；
- Canvas 2D 呈现；
- 点击一个像素显示 coverage/depth/color history；
- 完成 ADR，更新估算与第一版支持矩阵。

P0 的三角形允许是一次性 spike，用来验证 thread/present/shader/raster 组合；P1 负责把结论重构为有资源模型、测试和稳定接口的生产内核，二者不是重复交付。

P0 出口必须包含实际 benchmark、浏览器测试和量化 go/no-go 指标，而不要求 spike 代码可直接进入生产内核。五项 WebGPU G0 结论分别是：

1. mt/st 构建、线程确定性与 fallback 可行；
2. Canvas shared-memory staging copy 的实测成本可接受，或已有明确降级策略；
3. dedicated owner-worker queue dispatcher 与 Rayon 可以稳定组合；
4. Naga WGSL → 项目自有 IR/bytecode/VM 的 lowering 可行；
5. explicit `CpuGpuSubset` provider、Future/Promise/error scope 与标准 façade capability gate 的边界清楚。

P0 同时提交一份不阻断 WebGPU 的 WebGL 兼容性记录和上述冻结 fixtures：公共 IR 不绑定 WGSL、`RasterConvention` 同时覆盖两套 clip/origin 规则、packet/trace schema 能 round-trip WebGL provenance。P2 在最终 IR/VM/source-map/capture decoder 定型时用不超过 1–2 人日重跑同一批 fixtures；失败只阻断 schema freeze，不自动启动 ESSL parser。只有检查发现公共架构会导致未来大规模重写时，才允许阻断 P1/P2；ESSL parser 通过率不属于 P0 出口。

## 21. 风险清单

| 风险 | 影响 | 缓解措施 | 决策门 |
| --- | --- | --- | --- |
| WGSL lowering 或自有 VM 无法承载首版 shader 语义 | 很高 | Naga adapter、typed IR snapshot、明确 feature gate、source-span diagnostics | P0/P2 |
| 公共 IR/capture schema 被 Naga 或 WGSL 结构绑死 | 很高 | project-owned IR；P0 手工 ESSL lowering；P1/P2 WebGL 兼容复核 | P0/P1/P2 |
| 光栅规则硬编码成 WebGPU，未来 WebGL 需重写 | 很高 | 双 `RasterConvention`、两套 clip/origin/frag-depth goldens | P0/P1 |
| packet/hazard/debugger schema 只适合 encoder API | 高 | 用代表性 WebGL state/FBO/sync-readback 做限时映射；保留 API-neutral provenance | P0/P1/P2 |
| ESSL100/300 编译器工作量超预期 | 很高 | 子集合同、source-span error、native oracle、独立前端 | P7a 入口与出口 |
| JS drop-in 与“零 JS”目标冲突 | 很高 | Rust-first；显式 adapter；生成 façade；明确品牌检查限制 | 用户确认 D1/D2 |
| WebGL1/2 + WebGPU 范围爆炸 | 很高 | 垂直切片、支持矩阵、逐功能 gate，不宣称全合规 | 每个版本 gate |
| subset 被误装成标准 WebGPU/WebGL 对象 | 很高 | `Cpu*Subset` 显式命名；core capability gate 后才开放标准 façade | P3/P5/P6/P7b/P8 |
| 跨 draw/submit 资源 hazard 导致并行错序 | 很高 | packet access sets、RAW/WAR/WAW epochs、全局 barrier tests | P1 |
| Canvas shared-memory 复制成为瓶颈 | 高 | benchmark、dirty tiles、复用 staging、分辨率缩放 | P0 present spike |
| 主线程等待 CPU 渲染导致 UI 卡顿 | 高 | packet 化、帧预算、worker-hosted 模式 | P0 worker spike |
| worker 中创建 Rayon pool 不稳定 | 高 | 跨浏览器 spike；保留模式 A 和 st fallback | P0 worker spike |
| queue dispatcher 占住 Rayon 或 Future/Promise 乱序 | 很高 | owner event loop 串行调度；Rayon 仅跑有界 kernel；顺序/关闭压力测试 | P0/P3/P5/P6 |
| 浮点/过滤/边规则与硬件不同 | 高 | fixed conventions、profile、golden/CTS、tolerance 分类 | P1/P2 tests |
| Naga/wgpu/规范快速变化 | 中高 | spec/dependency pin、adapter layer、季度升级 | 每次依赖升级 |
| shared memory/nightly 工具链脆弱 | 中高 | 固定版本、mt/st 双构建、ParallelExecutor 抽象 | CI |
| 不可信 shader/capture 锁死或 OOM | 高 | instruction/resource budget、bounds check、fuzz | P2/P4 |
| 调试 instrumentation 拖慢默认路径 | 中 | feature flags、按 draw/pixel 选择性开启、ring buffer | P4 benchmark |
| 软件 CPU 耗时被误当 GPU 性能 | 中 | UI 明确标注 CPU reference metrics | 产品文档 |

## 22. 建议的首个公开演示

以下演示作为 **P4 WebGPU Debugger Beta 的发布门**。P0 只有无纹理/最小 shader 的验证三角形，P2 内部交付 WGSL textured cube，P3 首次公开并加入 compute 与真实 queue timeline，不把这些层级混为同一里程碑。

一个完全由 Rust/Wasm 创建的页面，包含：

- 顶部始终显示 `CpuGpuSubset — 非 WebGPU conformant` 和当前支持矩阵链接；
- Canvas 2D 输出；
- WebGPU-style WGSL render pipeline 在 CPU 绘制 indexed textured cube，P2 使用显式 LOD；
- compute pass 更新一组顶点/实例数据，再由后续 render pass 消费，展示真实 buffer hazard 与 submit 顺序；
- depth、culling 和 alpha blending 开关；
- 顺序执行器 / 已初始化 Rayon pool 的逻辑切换；改变实际 worker 数需重建实例；
- 实时显示 stage time、present copy bytes 和 output hash；
- resource、bind group、pipeline、pass/encoder 和 queue serial 检查器；
- 点击像素查看 primitive、WGSL output、depth 和 blend history；选择 compute invocation 查看指令与 storage access；
- 故意注入错误 buffer usage/stride、bind group layout、WGSL 类型或 map/error-scope 顺序，显示精确诊断及归属时间线；
- 不调用宿主 WebGPU/WebGL 进行绘制。

这个演示比单纯追求 FPS 更能证明项目的独特价值。

## 23. 需要优先讨论的方案问题

| 问题 | 推荐 | 最晚决定时间 |
| --- | --- | --- |
| Q1 调试对象 | **C：Rust-first，JS adapter 随后（已决定）** | 已决定：2026-08-22 |
| Q2 手写 JS 边界 | **A：允许极薄边界（已决定）** | 已决定：2026-08-22 |
| Q3 首个公开 API | **B：`CpuGpuSubset` Render + Compute + Queue MVP（已决定）** | 已决定：2026-08-22 |
| Q4 首版兼容目标 | **A：明确、可解释子集（已决定）** | 已决定：2026-08-22 |
| Q5 compute 深度 | B：基础 barrier、atomics 后置 | P2 结束前 |
| Q6 性能基线 | 720p/15fps 暂定门，P0 实测冻结 | P0 第 2 周 |
| Q7 运行时语言 | A：浏览器纯 Rust/Wasm | P7a ESSL 实现前 |
| Q8 浏览器矩阵 | A：Chromium + Firefox mt，Safari st fallback | P0 第 2 周 |

### Q1. 主要要调试哪类代码？

- A：Rust/Wasm 程序调用 WebGPU-style API；
- B：现有 JavaScript WebGPU 程序；
- C：两者都要，Rust-first、JS adapter 后补；WebGL 调试对象在 P7a 前再单独确认。

**已决定：C。** 如果未来要求 B 提前，P5 adapter/proxy 就必须前移，路线图与 13–21 个月估算都要重排；P0 也要把 WebGPU WebIDL façade、Promise、typed arrays、provider 注入和框架探测纳入核心风险范围。标准 façade 仍必须等 capability-guarantee gate 通过，不能把 subset provider 注入成 `navigator.gpu`。

### Q2. 对“不要传统前端技术”的严格程度？

- A：不使用前端框架和 JS 业务/UI 代码，但允许生成 glue 与极薄 shim；
- B：仓库中完全无手写 JS。

**已决定：A。** B 会阻止透明 JS API 兼容和灵活的双构建加载。

### Q3. 第一产品里程碑优先级？

- A：公共 WGSL 技术切片之后，先公开 `CpuWebGlSubset` MVP；
- B：先公开非标准 `CpuGpuSubset` render + compute MVP；
- C：两者同时铺开。

**已决定：B，WebGPU 优先。** P2 是 internal Rust-native Render Alpha；P3 才是首个公开的 Render + Compute + Queue MVP，随后进入 P4 WebGPU Debugger Beta。WebGL 从 P7a 开始，不再与首版抢占主线资源。

### Q4. 首版兼容目标？

- A：教学/确定性/可解释的明确子集；
- B：尽快跑选定的真实 WebGPU 框架/样例；
- C：从一开始追官方 conformance。

**已决定：首版选择 A，后续再依次进入 B 和 C。** WebGPU 首版若直接选择 B/C，需要同时承担标准 adapter 的 core capability guarantees、异步 queue/error/device-loss 语义、更多基础 formats/limits、WebIDL 品牌兼容和未修改 CTS，远超一个可解释 CPU 调试 MVP 的范围。

### Q5. Compute 是否必须进入首个 `CpuGpuSubset` MVP？

- A：只做无 barrier 的简单 storage buffer compute；
- B：包含基础 workgroup memory/barrier，atomics 后补；
- C：首个 subset MVP 暂不包含 compute。

**建议：B。** storage texture、完整 atomics 和 subgroup 后补；实现完成前，涉及这些能力的 shader 必须被拒绝。

### Q6. 性能基线是什么？

需要指定：参考机器、浏览器、分辨率、场景规模、目标交互帧率和最大内存。暂定值为“登记实际开发机型号、固定 Chromium 版本、1280×720 简单 textured scene、多线程至少 15fps、256 MiB 软预算”；P0 同时报告 Firefox 和 1080p 数据，再由实测冻结正式门槛。调试器与“实时替代 GPU”的目标不能混在一个模糊指标里。

### Q7. 是否坚持运行时 100% 纯 Rust？

- A：浏览器运行时坚持纯 Rust/Wasm，native CI 可使用外部 oracle；
- B：允许把 glslang/ANGLE 等 C++ 组件编译进浏览器 Wasm。

**建议：A。** 如果未来选择 B，必须单独评估包体、许可证、启动时间和构建复杂度。

### Q8. 第一批浏览器？

- A：Chromium + Firefox 支持多线程，Safari 先保证单线程 fallback；
- B：首版三个引擎都要求多线程；
- C：首版只支持 Chromium。

**建议：A。** Safari 多线程提升在 P0 实测后单独排期。

## 24. 一手资料

- [W3C WebGPU specification](https://www.w3.org/TR/webgpu/)
- [W3C WGSL specification](https://www.w3.org/TR/WGSL/)
- [GPUWeb CTS](https://gpuweb.github.io/cts/)
- [GPUWeb API type inventory](https://gpuweb.github.io/types/)
- [Naga shader translator](https://github.com/gfx-rs/wgpu/tree/trunk/naga)
- [wasm-bindgen-rayon maintained repository](https://github.com/RReverser/wasm-bindgen-rayon)
- [wasm-bindgen parallel raytracing/thread notes](https://rustwasm.github.io/docs/wasm-bindgen/examples/raytrace.html)
- [WHATWG Canvas](https://html.spec.whatwg.org/multipage/canvas.html)
- [WHATWG ImageData](https://html.spec.whatwg.org/multipage/imagebitmap-and-animations.html)
- [Web IDL AllowShared rules](https://webidl.spec.whatwg.org/#AllowShared)
- [MDN SharedArrayBuffer security requirements](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/SharedArrayBuffer)
- [MDN COOP header and cross-origin isolation](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Cross-Origin-Opener-Policy)
- [MDN COEP header](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Cross-Origin-Embedder-Policy)
- [Khronos WebGL API Registry](https://registry.khronos.org/webgl/)
- [WebGL 1 latest specification and IDL](https://registry.khronos.org/webgl/specs/latest/1.0/)
- [WebGL 2 latest specification and IDL](https://registry.khronos.org/webgl/specs/latest/2.0/)
- [Khronos WebGL repository and conformance tests](https://github.com/KhronosGroup/WebGL)
- [Khronos glslang](https://github.com/KhronosGroup/glslang)

## 25. 下一步

Q1/Q2/Q3/Q4 已通过 ADR 和本次 P0 实现锁定为：Rust-native 子集优先、允许极薄启动 shim、WebGPU
优先、明确声明非标准且不冒充 `navigator.gpu`。接下来进入 P1/P2 前应先讨论并冻结 Q5（首个 MVP 的
compute/barrier 范围）和量化 Q6（参考机器、分辨率、场景与帧率）。

建议下一迭代先实现项目自有 shader IR/字节码与 WGSL lowering，让 demo 顶点/片元 shader 真正在 CPU
执行；随后加入 buffer、bind group、render pipeline、texture/sampler 和 capture schema。owner-worker 与异步
queue spike 应作为独立 gate，不能把当前同步 `ImmediateMainOwnerQueue` 宣称成标准 WebGPU queue。WebGL
的产品实现继续冻结到 P7a+，只保留公共架构兼容检查。ADR 0003 的 webgl-mock 在独立的 `glgpu-webgl` 里推进 CPU 子集，不调用原生 WebGL / 原生 WebGPU，也不并入 webgpu-mock crate。demo 页当前仍是主线程单线程 Wasm，并把像素直接交给页面 canvas。这是未来的设计，目前还未实现。权威像素将在 SharedArrayBuffer 双缓冲的 back 槽，主线程呈现 front 槽。目前像素在 CPU framebuffer，主线程 `putImageData`。不使用 OffscreenCanvas。
