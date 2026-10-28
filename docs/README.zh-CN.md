# GLGPU 文档索引

本文档集描述 GLGPU 的长期架构和当前已经实现的 P0 WebGPU-first 垂直切片。阅读时必须区分“当前契约”和“规划目标”：规划文档中出现的功能不代表代码已经支持。

## 推荐阅读顺序

### 第一次运行项目

1. 阅读仓库根目录的[项目总览与快速开始](../README.md)。
2. 按[开发指南](development-guide.zh-CN.md)准备 stable、nightly、Wasm target 和 `wasm-bindgen` 工具。
3. 阅读[测试与验收](testing-and-acceptance.zh-CN.md)，确认本机的 native、ST 和 MT 路径均通过。

### 使用或扩展 Rust API

1. 阅读[P0 API 与行为契约](api-reference.zh-CN.md)。
2. 检查[机器可读支持矩阵](support-matrix/webgpu-p0.toml)。
3. 对照[实施路线图](roadmap.zh-CN.md)，确认目标能力属于当前里程碑还是后续阶段。

### 修改核心架构

1. 阅读[完整架构与阶段规划](architecture-plan.zh-CN.md)。
2. 阅读[ADR 0001：WebGPU-first](adr/0001-webgpu-first.md)。
3. 阅读[ADR 0002：ST/MT 双构建](adr/0002-wasm-build-profiles.md)。
4. 若改动 webgl-mock，阅读[ADR 0003](adr/0003-parallel-webgl-mock.md)。它不改变 ADR 0001 的产品顺序。
5. 检查根目录 [`spec-lock.toml`](../spec-lock.toml) 中的规范、工具链和依赖锁。
6. 架构边界发生变化时先增加或修改 ADR，再修改代码和支持矩阵。

## 文档清单

| 文档 | 定位 | 是否描述当前实现 |
| --- | --- | --- |
| [根 README](../README.md) | 项目入口、快速开始、功能边界 | 是 |
| [API reference](api-reference.zh-CN.md) | P0 Rust API、错误、线程和确定性契约 | 是 |
| [Development guide](development-guide.zh-CN.md) | 环境、构建、部署、调试和故障排查 | 是 |
| [Testing and acceptance](testing-and-acceptance.zh-CN.md) | 测试命令、浏览器验收、性能口径 | 是 |
| [Support matrix](support-matrix/webgpu-p0.toml) | 机器可读 implemented/partial/unsupported | 是 |
| [Roadmap](roadmap.zh-CN.md) | 当前进度、下一迭代和阶段 gate | 当前状态 + 规划 |
| [Architecture plan](architecture-plan.zh-CN.md) | 完整长期技术方案与风险分析 | 当前基线 + 长期规划 |
| [ADR 0001](adr/0001-webgpu-first.md) | WebGPU-first、Rust-first、WebGL 产品实现延后 | 已接受决策 |
| [ADR 0002](adr/0002-wasm-build-profiles.md) | stable ST 与 nightly MT 分离 | 已接受决策 |
| [ADR 0003](adr/0003-parallel-webgl-mock.md) | 并行的 webgl-mock；独立栈，不取代 ADR 0001 | 已接受决策 |
| [webgl-mock](webgl-mock.zh-CN.md) | 模拟 WebGL 的基线、当前子集、线程模型现状与目标 | CPU mock，不是原生 WebGL，也不是合规实现 |
| [webgl-mock 矩阵](support-matrix/webgl-mock.toml) | webgl-mock 能力；implemented / partial / not_started | 是 |
| [`spec-lock.toml`](../spec-lock.toml) | 规范日期、工具版本、构建 profile | 是 |

## 当前状态摘要

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| CPU triangle raster | 已实现 | framebuffer 坐标输入、16×16 tile、RGBA8 + depth |
| Deterministic ST/MT | 已实现并测试 | tile 间并行；tile 内稳定 primitive 顺序 |
| Selected pixel history | 部分实现 | raster-stage；不含 pre-raster cull event |
| WGSL parse/validate | 已实现 | Naga core validation；不执行 shader |
| Rust-native WebGPU 风格 API | P0 子集 | 同步 immediate queue；非标准 |
| Canvas 2D presentation | 已实现 | ST 直接路径；MT 必须 staging copy |
| Browser MT loader | 已实现 | 显式 `?threads=1`；capability check；ST fallback |
| Buffer/bind group/pipeline | 未实现 | P1/P2 |
| WGSL lowering/VM | 未实现 | 下一主线 |
| Texture/sampler | 未实现 | P2 |
| Async owner-worker queue | 未实现 | 独立 gate |
| Compute execution | 未实现 | 范围仍需冻结 |
| Standard `navigator.gpu` façade | 未实现 | 必须等待 capability/CTS gate |
| WebGL/ESSL 产品实现 | 延后 | P7a+；ADR 0001 仍然有效 |
| webgl-mock | CPU 子集 | 模拟 WebGL：clear、WebGL clip/左下窗口、shader 三角形 demo、ESSL 子集、bytecode 和解释器。`/webgl.html` 当前是主线程单线程 Wasm，并用 `putImageData` 直接交到页面 canvas。目标是先写入 Worker 离屏缓冲区，再经 Canvas 2D present 到可见 canvas；高效通信是 SharedArrayBuffer 或零拷贝转移。禁止原生 WebGL/WebGPU。见 ADR 0003 |

## 信息优先级

文档之间出现不一致时，按以下顺序判断当前行为：

1. 可重复的测试和实际代码；
2. `docs/support-matrix/webgpu-p0.toml`；
3. 已接受 ADR；
4. `spec-lock.toml`；
5. API、开发和测试文档；
6. 长期架构/路线图中的规划性描述。

不应因为架构文档规划了某项能力，就把它标记为当前已支持。

## 文档维护规则

以下变更必须同步更新文档：

- 新增或删除公开 API：更新 API reference、支持矩阵和相关测试；
- 改变 ST/MT feature、nightly、link flags 或 worker 策略：更新 ADR 0002、开发指南和 `spec-lock.toml`；
- 改变 framebuffer、coverage、depth 或 hash 语义：更新 API reference、测试文档和 golden/hash；
- 支持新的 WGSL 能力：更新 validator allowlist、支持矩阵、shader tests 和错误示例；
- 引入标准 façade 或 WebGL：必须新增 ADR，不能只修改 README 中的宣传文字；
- 升级锁定规范或关键依赖：按 `spec-lock.toml` 的 policy 补 ADR、shader snapshot、golden 和浏览器 smoke test。

## 术语

- **webgl-mock / 模拟 WebGL**：CPU 上的 WebGL2 风格实现。入口 `WebGlMock`，三角形页面是 demo，上下文是 `DemoContext`。标识 `webgl-mock — NOT WEBGL CONFORMANT`。
- **webgpu-mock / 模拟 WebGPU**：CPU 上的 WebGPU 风格实现。标识保持 `CpuGpuSubset — NOT WEBGPU CONFORMANT`。
- **原生 WebGL / 原生 WebGPU**：浏览器内建 API。本仓库不把它们当作渲染后端。
- **目标线程模型**：管线结果先写入 Worker 离屏缓冲区，不直接画到页面 canvas；之后经 Canvas 2D present 交到可见 canvas。主线程 Web API façade 与 Worker + Wasm 后端之间的高效通信是 SharedArrayBuffer，以及可转移 ArrayBuffer / OffscreenCanvas 的零拷贝转移，不是普通 postMessage 拷贝。webgl-mock 的 demo 页当前仍是主线程单线程 Wasm。
- **P0 垂直切片**：当前可运行的 CPU triangle → framebuffer → Canvas2D 与最小像素追踪，不代表完整 P0 风险验证已经结束。
- **ST**：stable toolchain 构建的非共享内存单线程 Wasm 包。
- **MT**：固定 nightly、重建标准库、SharedArrayBuffer 和 `wasm-bindgen-rayon` 的多线程 Wasm 包。
- **owner**：拥有 `CpuDevice`、同步 queue 和 Canvas 2D context 的浏览器主线程。
- **prepared primitive**：通过输入有限性、退化和可见范围准备阶段后进入 tile binning 的三角形。
- **conformant**：满足锁定 WebGPU 规范 capability guarantees 和可观察语义，并通过相应 CTS gate；当前项目不满足。

