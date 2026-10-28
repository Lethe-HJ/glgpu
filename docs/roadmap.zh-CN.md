# GLGPU 实施路线图

> 状态日期：2026-08-22  
> 当前主线：WebGPU-first  
> 当前阶段：P0 垂直切片完成；P0 的完整风险验证仍在进行

本文档把[长期架构规划](architecture-plan.zh-CN.md)转换成可执行的交付顺序。阶段编号表示依赖和发布 gate，不代表时间承诺。未通过前一阶段出口条件时，不应通过宣传或 façade 提前暴露后一阶段能力。

## 状态标记

- `DONE`：代码、测试和文档已在仓库中落地；
- `PARTIAL`：已有垂直切片，但出口条件尚未全部满足；
- `NEXT`：下一主线；
- `PLANNED`：已规划但尚未开工；
- `DEFERRED`：明确延后，不占用当前主线。

## 总览

| 阶段 | 状态 | 主要交付 |
| --- | --- | --- |
| P0 | `PARTIAL` | CPU triangle、ST/MT、Canvas2D、最小 pixel history、WGSL validation、风险数据 |
| P1 | `NEXT` | 项目自有 shader IR/bytecode、WGSL lowering、CPU VS/FS 执行、基础资源句柄 |
| P2 | `PLANNED` | Rust-native render alpha：buffer、bind group、pipeline、texture/sampler、render-to-texture |
| P3 | `PLANNED` | 公开 `CpuGpuSubset` MVP：异步 owner queue、compute 基线、版本化 capture |
| P4 | `PLANNED` | WebGPU Debugger Beta：资源/command/queue/shader/pixel UI |
| P5 | `PLANNED` | namespaced JavaScript adapter 和 dedicated owner worker proxy |
| P6 | `PLANNED` | 标准 WebGPU façade 候选；capability、observable semantics 和 CTS gate |
| P7a–P7b | `DEFERRED` | ESSL frontend、`CpuWebGlSubset` 和 WebGL 状态机 |
| P8–P9 | `DEFERRED` | WebGL adapter/façade 与 conformance track |

## P0：平台与确定性风险验证

### 已完成

- Cargo workspace、Rust stable 锁、Wasm target 和依赖锁；
- WebGPU/WGSL dated specification lock；
- DOM-free framebuffer、triangle 和 trace 数据类型；
- 16×16 tile、定点 edge、top-left、稳定 primitive order；
- 顺序/Rayon 输出、统计、history 和 hash 一致性测试；
- Rust-native `CpuGpuSubset` 与同步 `ImmediateMainOwnerQueue`；
- Naga core WGSL parse/validation 及结构化诊断；
- stable ST 和 fixed-nightly MT 双构建；
- `wasm-bindgen-rayon/no-bundler` worker 初始化；
- MT SharedArrayBuffer → non-shared ImageData staging；
- Rust/Wasm UI、Canvas 点击追踪、COOP/COEP 本地服务器；
- GitHub Pages 发布 ST/MT 调试页；Pages 不能发隔离头，`?threads=1` 由同源 service worker 补齐，失败回退 ST；
- Chromium 本机 ST/MT smoke test，当前 demo hash 一致。

### 尚未完成的 P0 出口项

- Firefox MT 和 Safari ST fallback 的固定版本测试记录；
- 独立 1/2/4/8 worker 实例的正式浏览器测试矩阵；
- 720p、1080p 的 raster、shared→ImageData copy、`putImageData` 分段 benchmark；
- 参考机器、浏览器版本、内存峰值和暂定帧率 gate；
- dedicated owner worker 内初始化 Rayon pool 的可行性 spike；
- submission 连续压力、hidden tab、worker shutdown/device loss/cancel 模型；
- project-owned shader IR 能否承载首批 WGSL 的证明；
- 限时 WebGL future-proofing fixtures：双 raster convention、API-neutral trace provenance；
- versioned capture schema 草案。

只有补齐上述风险数据，P0 才能从 `PARTIAL` 变为 `DONE`。当前可运行 demo 不能替代这些出口证据。

## P1：Shader 执行内核

### 目标

让 demo 不再使用手写等价 framebuffer 顶点，而是执行经过校验和 lowering 的 WGSL vertex/fragment entry point。

### 建议顺序

1. 定义项目自有 typed IR，不把 Naga handle 泄漏到公开 API 或 capture；
2. 定义稳定 bytecode、value/type/layout 和 source-map 格式；
3. lowering 最小标量/向量/矩阵、局部变量、控制流和函数调用；
4. 实现确定性 interpreter，而不是依赖宿主 JS `eval` 或 GPU；
5. 接入 vertex input、builtins、inter-stage locations 和 fragment output；
6. 将 shader instruction/variable event 接到选定 invocation/pixel history；
7. 为 unsupported type/op/builtin 建立结构化诊断；
8. 建立 IR snapshot、bytecode snapshot 和 native/Wasm differential test。

### 出口条件

- demo triangle 的位置和颜色完全来自 WGSL；
- ST/MT framebuffer、shader trace 和 hash 一致；
- source span 能从 WGSL 映射到 IR/bytecode event；
- 不支持的 WGSL 不会静默接受或错画；
- Naga 升级不会直接破坏公开 capture schema。

## P2：Rust-native Render Alpha

### 范围

- versioned resource handle/generation；
- `GPUBuffer` 风格 usage、range 和 copy validation 子集；
- bind group layout、bind group 和 pipeline layout；
- render pipeline、vertex buffer/index buffer；
- texture/view/sampler、显式 LOD texture sampling；
- color/depth attachment、viewport/scissor、基础 blend；
- render-to-texture 和同步 readback；
- internal Rust-native WGSL textured cube。

### 非目标

- 标准 WebIDL brand/prototype；
- 完整异步 queue；
- 任意框架无修改兼容；
- 完整 WebGPU formats/features/limits。

## P3–P4：公开 MVP 与调试器 Beta

P3 必须把同步 P0 queue 替换为有明确 owner、serial、completion、error scope、mapping 和 device-loss 状态机的异步模型。compute 的首个范围在进入 P3 前冻结；默认建议支持 workgroup memory 和基础 barrier，storage texture、完整 atomics 和 subgroup 后补。

P4 在 capture/replay 可版本化之后提供资源、command、queue timeline、shader trace、selected pixel/invocation history 和差分 UI。调试 UI 必须能区分规范错误、项目 subset 缺口、运行时错误和有意确定性策略。

## P5–P6：JavaScript 与标准兼容轨道

P5 只提供 namespaced adapter，不修改 `navigator.gpu`。应用必须显式注入 provider 和 canvas-context factory。owner worker proxy 只有在 P0/P3 的线程/queue spike 证明可靠后才能启用。

P6 只有同时满足以下条件才允许试验标准 façade：

- 锁定规范 core capability guarantees；
- WebIDL 转换、brand/prototype 和 Promise 行为；
- queue/error/device-loss 等可观察语义；
- formats、limits、features 一致报告；
- 未修改 CTS 达到预先冻结的通过门槛；
- unsupported 不会伪装为成功。

未通过 gate 时必须继续使用 `CpuGpuSubset` 或 namespaced adapter。

## P7+：WebGL

WebGL 前端复用资源、IR/VM、texture sampler、raster 和 trace，但必须单独实现：

- ESSL 1.00/3.00 parser、validator、preprocessor 和 lowering；
- mutable WebGL state machine；
- framebuffer completeness、pixel store 和同步 readback；
- WebGL/WebGPU 不同 clip/origin/depth/convention；
- WebGL context loss、extension 和 error semantics；
- WebGL 1/2 adapter 与 conformance gate。

P7a 之前不实现 ESSL parser；P7b 之前不实现 WebGL state machine；P8 之前不实现 WebGL JavaScript façade。上表里的 `DEFERRED` 保持不变。

[ADR 0003](adr/0003-parallel-webgl-mock.md) 另开了一条不改变该产品顺序的 webgl-mock：在 `glgpu-webgl` 里用 CPU 从零实现模拟 WebGL，不依赖原生 `WebGLRenderingContext`、`navigator.gpu` 或 webgpu-mock crate。当前子集是 clear、WebGL clip 体积与左下窗口，以及由解释器输出驱动的三角形 demo。`/webgl.html` 当前用主线程上的单线程 Wasm 跑这个 demo，并用 `putImageData` 直接交到页面 canvas。2026-10-28 已决定：主路径是 SharedArrayBuffer 双缓冲加 Atomics。back 槽是逻辑绘制缓冲，主线程呈现 front 槽。不使用 OffscreenCanvas，三缓冲不是默认。这次不实现 Worker 后端。这不是 P7 开工，也不是 WebGL conformant。说明见 [webgl-mock](webgl-mock.zh-CN.md)。

## 下一迭代建议

下一次实现按以下优先级推进：

1. project-owned shader IR 与版本字段；
2. Naga WGSL → IR 的最小 VS/FS lowering；
3. CPU interpreter 和 source-map trace；
4. 用真实 WGSL 替换当前预变换 triangle；
5. 固化 ST/MT shader execution golden；
6. 同时补 P0 浏览器/benchmark 数据，不提前铺开 texture 或 WebGL。

