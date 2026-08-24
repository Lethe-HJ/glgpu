# ADR 0001：WebGPU-first，WebGL 延后

- 状态：Accepted
- 日期：2026-08-22
- 决策范围：产品顺序、首个公开 API、前端技术边界

## 背景

同时实现 WebGPU、WebGL 1/2、WGSL、ESSL、标准 JavaScript façade 和软件渲染器，会把 API 状态机、shader frontend、资源模型、同步语义和 conformance 风险同时引入。项目的首要价值是确定性和可解释性，而不是尽快伪装成浏览器 GPU 实现。

WebGPU 的显式 resource/encoder/queue 模型更适合先建立不可变 command packet、资源生命周期和 capture/trace；WebGL 的 mutable global state、同步 readback、pixel-store 和 ESSL 可以在公共执行内核稳定后单独接入。

## 决策

GLGPU 先实现非标准 Rust-native `CpuGpuSubset`，在满足 capability 和可观察语义 gate 之前，不把任何对象注入 `navigator.gpu`。产品顺序为：

1. P0/P1：平台、公共 CPU renderer、shader IR/VM；
2. P2：internal Rust-native render alpha；
3. P3：公开 render + compute + queue MVP；
4. P4：WebGPU Debugger Beta；
5. P5：namespaced JavaScript adapter；
6. P6：通过 capability、observable-semantics 和 CTS gate 后，才评估标准 WebGPU façade；
7. P7a+：ESSL 与 WebGL；
8. P8/P9：WebGL adapter/façade 与 conformance track。

首个 API 面向 Rust 调用方，使用结构化 `Unsupported` 表达 subset 缺口。P0 spike 使用同步 `ImmediateMainOwnerQueue`；它不模拟标准 WebGPU queue timeline、Promise、error scope、mapping 或 device loss。

浏览器页面、样式、事件、Canvas 和调试 UI 由 Rust/Wasm 控制。允许存在以下不可消除或严格限定的边界代码：

- 最小 HTML mount point；
- `wasm-bindgen` 生成的 JavaScript glue；
- ST/MT 包选择与 worker bootstrap；
- 后续显式注入所需的窄 compatibility shim。

React、Vue、Svelte、TypeScript 应用层和 Node/Vite 浏览器运行时不进入架构。

## 当前 P0 体现

- 类型名称为 `CpuGpuSubset`，UI 永久显示 `NOT WEBGPU CONFORMANT`；
- WGSL 只做 Naga core syntax/semantic validation，不执行；
- command encoder 只接受 clear、预变换 screen-space triangle 和 selected pixel；
- queue 在 owner 线程同步完成；
- Canvas 2D 是唯一 presentation backend；
- 支持矩阵明确列出 implemented、partial、unsupported 和 intentionally different。

## 后果

### 正面

- 可以先证明 shader/raster/thread/present/trace 的正确性；
- 公共内核不被 WebGL mutable state 或浏览器 WebIDL brand 绑定；
- subset 缺口可见，不会静默错画或假装标准支持；
- Rust-native 测试不依赖 DOM 或浏览器。

### 成本

- 现有使用 `navigator.gpu` 的应用不能直接运行；
- P5 之前需要应用显式使用 Rust-native provider；
- WebGL 和 ESSL 不参与近期功能交付；
- 标准 façade 需要单独实现 WebIDL、Promise、queue/error/device-loss 和 CTS gate，不能简单包装当前 API。

## 明确拒绝的替代方案

### 从第一天同时做 WebGPU 与 WebGL

拒绝。它会在公共 IR、raster conventions 和 capture schema 尚未稳定时扩大状态空间。P0 只允许不超过总工时 20% 的 WebGL future-proofing fixture，不允许实现 ESSL parser、WebGL 状态机或 façade。

### 直接 monkey-patch `navigator.gpu`

拒绝。当前实现不满足 core capability guarantees 和标准可观察语义，注入会让应用把 subset 当成合规实现。

### 使用宿主 WebGPU/WebGL 完成实际绘制

拒绝。项目目标是 CPU reference/debug execution；宿主 GPU 只可作为未来测试 oracle，不能成为渲染后端。

### 使用传统前端框架构建调试器

拒绝。Rust/Wasm 负责 UI 状态和交互，避免维护并行的 JavaScript/TypeScript 业务模型。

## 重新评估条件

只有出现以下情况才重新讨论顺序：

- 首个付费/生产场景必须运行未修改的 JavaScript WebGPU 应用；
- project-owned IR 证明无法同时承载 WGSL 与后续 ESSL；
- 标准 façade 的 capability/CTS 工作量显著低于当前估计；
- WebGL 兼容探针发现公共 renderer/trace 架构会导致大规模返工。

重新评估必须新增 ADR，不得只修改 README 宣传口径。

