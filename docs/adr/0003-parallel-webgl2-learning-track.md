# ADR 0003：并行的纯 CPU WebGL2 学习轨道

- 状态：Accepted（仅脚手架；不取代 ADR 0001）
- 日期：2026-10-27
- 决策范围：WebGL2 学习轨道的目标、禁止的后端，以及当前允许落地的范围

## 背景

仓库的原始目标是学习 WebGPU 和 WebGL：在不依赖浏览器原生 WebGL / WebGPU 的前提下，自己实现相近的 API。当前 `main` 只有 WebGPU 风格的 CPU / reference 路径，也就是 P0 垂直切片。页面契约仍然是 `CpuGpuSubset — NOT WEBGPU CONFORMANT`。

[ADR 0001](0001-webgpu-first.md) 把产品顺序定为 WebGPU 优先，ESSL 与 WebGL 的产品实现放在 P7a+。该决定继续有效。公共 IR、光栅约定和 capture schema 还没有稳定，不能在产品主线上同时展开两套可执行栈。

学习目标本身需要一条看得见的 WebGL2 轨道。否则仓库只表达 WebGPU，后加入的人无法判断 WebGL2 打算怎么学、边界在哪里。因此允许在 P7a 产品 gate 之前，用独立 crate 和文档开始一条并行学习轨道。这条轨道提前于路线图里的产品实现，但它只搭建骨架，不把 P7 标成进行中，也不是 ADR 0001 所拒绝的“从第一天同时实现 WebGPU 与 WebGL”。

## 决策

1. 新增并行学习轨道。目标与 WebGPU 轨道相同：从 0 到 1，在 CPU 上从零实现一套完整的 WebGL2 风格栈，供学习使用。执行、校验和光栅都写在本仓库的软件路径里。
2. 这条轨道不依赖浏览器原生 WebGL 或 WebGPU。禁止把 `WebGLRenderingContext`、`canvas.getContext("webgl2")`、`navigator.gpu` 或任何宿主 GPU 绘制调用当作后端、参考执行器或脚手架依赖。宿主 GPU 将来若出现，也只能作为测试 oracle，不能成为渲染后端；这一条与 ADR 0001 对宿主 WebGPU / WebGL 的拒绝一致。
3. 最终画面仍按项目既有方向，把 CPU framebuffer 转成 RGBA8，经 Canvas 2D `putImageData` 呈现。本决策的脚手架还没有接入呈现。
4. 产品顺序不变。ADR 0001 仍是主线。P0 的 `NOT WEBGPU CONFORMANT` 文案不得删除或改成合规声明。WebGL2 轨道自己的永久标识是 `CpuWebGl2LearningTrack — NOT WEBGL CONFORMANT`。支持矩阵在实现对应行为之前只能记录 `NotStarted`，不能把缺口报成已支持。
5. 本决策当前只接受脚手架：`crates/glgpu-webgl`、本 ADR、学习说明、机器可读支持矩阵，以及一个不可运行的示例占位。首个变更不实现 ESSL 3.00 parser、WebGL 状态机、`draw*`、`readPixels`、JavaScript façade 或浏览器上下文包装。
6. 以后的实现要复用或显式对齐公共 CPU 内核（framebuffer、光栅、trace），并用架构规划里的 `RasterConvention` 表达 WebGL 的 clip Z `-w..w` 和左下原点。不得另写一条会调用浏览器的渲染器。
7. 若要把产品路线从“P7a+ 才实现 WebGL”改成“主线提前交付可运行的 WebGL2”，必须再写一份 ADR。本 ADR 不构成那次重新评估。`spec-lock.toml` 里 `[deferred.webgl]` 的产品阶段和禁止项保持不变。

crate 名沿用架构规划第 6 节已经写过的 `glgpu-webgl`。调试页 `glgpu-web` 继续只服务 P0 WebGPU 路径，避免未实现的 WebGL2 入口出现在现有 UI 上。

## 后果

### 正面

- WebGL2 的学习方向在仓库里有独立入口，读代码的人能看到它是纯 CPU、从零开始，而不是封装浏览器。
- P0 crate、`CpuGpuSubset` 行为和 `NOT WEBGPU CONFORMANT` 契约保持原样。
- 支持矩阵从第一天就是“全部未开始”，后续能力只能逐项打开。

### 成本

- 路线图上的 P7a–P9 仍然是产品交付 gate。学习轨道上的笔记不能被引用成“WebGL 已经开工实现”。
- 骨架 crate 会进入 workspace 测试。它只锁定不变量，不是 WebGL 验收。
- 公共内核里的 WebGL clip / origin 约定仍然没有实现；在 `RasterConvention` 落地之前，不能声称已经有 WebGL2 绘制。

## 明确拒绝的做法

### 用浏览器 WebGL 或 WebGPU 完成绘制

拒绝。学习目标是自己实现栈。`web-sys` 的 `WebGLRenderingContext`、`navigator.gpu` 和宿主 `drawArrays` 都不能作为 `glgpu-webgl` 的依赖或后端。

### 在脚手架变更里实现完整 WebGL2

拒绝。状态机、ESSL、资源模型和 conformance 仍是后续步骤。本 ADR 只固定方向和空壳。

### 改写 P0 来“顺便”容纳 WebGL 调用

拒绝。WebGPU 风格 API 与 WebGL 风格 API 保持分离，复用点停在以后的内部 packet / 内核层，与架构规划第 5.2 节一致。

## 学习轨道的后续步骤

下列事项不属于本 ADR 的落地范围，而是下一次实现时应按顺序考虑的工作：

1. 为学习轨道单独冻结有日期的 WebGL 2.0 与 ESSL 3.00 阅读基线，并写明这不是合规声明。基线写入 `spec-lock.toml` 的学习轨道段或后续 ADR，不改 `[deferred.webgl]` 的产品阶段。
2. 定义 Rust-native 上下文类型和错误通道。继续用结构化 `NotStarted`（以及日后的 `Unsupported`）表达缺口。不暴露 JavaScript `getContext`。
3. 做与 P0 同级的 CPU 垂直切片：clear，加一个三角形，写入 CPU framebuffer，再显式 present 到 Canvas 2D。文档必须写清 WebGL 左下原点与 clip Z `-w..w`。光栅约定没有测试之前，矩阵里的 draw 保持 `NotStarted`。
4. 在垂直切片之后再引入有限的 mutable state 子集。WebGL 状态机的产品 gate 仍是 P7b。
5. ESSL 3.00 前端单独实现。Naga 的 GLSL 输入是 desktop GLSL，不是 ESSL，不能当作 parser 替代品。
6. JavaScript façade 晚于 Rust-native 子集，并且永远不是浏览器原生 `WebGL2RenderingContext`。

叙事说明、目录草图和命令见 [WebGL2 学习轨道](../webgl2-learning-track.zh-CN.md)。机器可读矩阵见 [`webgl2-learning.toml`](../support-matrix/webgl2-learning.toml)。
