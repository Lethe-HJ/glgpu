# ADR 0003：并行的纯 CPU WebGL2 学习轨道

- 状态：Accepted（2026-10-27 补充决定取代第 5、6 条；不取代 ADR 0001）
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
5. 首个变更只放入脚手架。**这一条已被文末补充决定取代：** 学习轨道可以在 `glgpu-webgl` 内继续实现独立的 CPU 子集，但仍然不实现 WebGL 状态机、`drawArrays` / `drawElements`、`readPixels`、JavaScript façade 或浏览器上下文包装。
6. 早期文字要求以后复用公共 CPU 内核。**这一条已被文末补充决定取代：** 学习轨道保持独立栈，不依赖 WebGPU 轨道的 crate，也不调用浏览器渲染器。
7. 若要把产品路线从“P7a+ 才实现 WebGL”改成“主线提前交付可运行的 WebGL2”，必须再写一份 ADR。本 ADR 不构成那次重新评估。`spec-lock.toml` 里 `[deferred.webgl]` 的产品阶段和禁止项保持不变。

crate 名沿用架构规划第 6 节已经写过的 `glgpu-webgl`。调试页 `glgpu-web` 继续只服务 P0 WebGPU 路径，避免未实现的 WebGL2 入口出现在现有 UI 上。

## 后果

### 正面

- WebGL2 的学习方向在仓库里有独立入口，读代码的人能看到它是纯 CPU、从零开始，而不是封装浏览器。
- P0 crate、`CpuGpuSubset` 行为和 `NOT WEBGPU CONFORMANT` 契约保持原样。
- 支持矩阵从第一天就公开缺口。补充决定之后，只有有测试的学习切片可以标成 implemented 或 partial。

### 成本

- 路线图上的 P7a–P9 仍然是产品交付 gate。学习轨道上的切片不能被引用成“WebGL 产品已经开工”或 “WebGL conformant”。
- `glgpu-webgl` 的测试锁定这条独立栈的行为，不是 WebGL 验收，也不证明与 WebGPU P0 光栅逐像素相同。
- 学习栈不复用 WebGPU 轨道的 framebuffer。WebGL 左下原点和 clip Z 仍未实现。

## 明确拒绝的做法

### 用浏览器 WebGL 或 WebGPU 完成绘制

拒绝。学习目标是自己实现栈。`web-sys` 的 `WebGLRenderingContext`、`navigator.gpu` 和宿主 `drawArrays` 都不能作为 `glgpu-webgl` 的依赖或后端。

### 在一次变更里实现完整 WebGL2

拒绝。状态机、完整 ESSL、资源模型、`drawArrays` 和 conformance 仍是后续步骤。补充决定只接受独立 crate 里的学习切片。

### 改写 P0 来“顺便”容纳 WebGL 调用

拒绝。WebGPU 风格 API 与 WebGL 风格 API 保持分离。学习轨道另外不得依赖 WebGPU 轨道的 crate；产品主线也不为了 WebGL 去改 P0。

## 补充决定（2026-10-27，开始落地）

本补充取代上面第 5 条和第 6 条，不取代 ADR 0001，也不修改 `[deferred.webgl]`。

1. 学习轨道是独立栈。`crates/glgpu-webgl` 不得依赖 `glgpu-core`、`glgpu-trace`、`glgpu-raster`、`glgpu-shader`、`glgpu-webgpu`、`glgpu-canvas2d` 或 `glgpu-web`。ESSL 前端留在这个 crate 里，不放进使用 Naga 的 `glgpu-shader`。WebGL 的 clip 空间和窗口原点以后也在这个 crate 里实现。
2. 规范阅读基线冻结在 `spec-lock.toml` 的 `[learning_track.webgl2]`：Khronos WebGL 2.0.0（2017-04-11），GLSL ES 3.00.6（2016-01-29）。ESSL 行为的 accept/reject 参照是 Khronos glslang `16.6.0`。该工具是外部 oracle，本 crate 不链接、不调用它。Naga 的 desktop GLSL 前端不是 ESSL 实现。
3. 编译路径定为：ESSL 源码 → AST → 项目自有、可调试的 bytecode IR → CPU 解释器。第一阶段不做机器码 JIT。指令上的 source span 和逐步局部变量快照留给以后的单步调试器；调试器 UI 本身还没有。
4. 当前允许的实现止于学习切片：RGBA8 clear；一个已经变换到 framebuffer 坐标的三角形（左上原点，用来对齐 WebGPU P0 三角形 demo 的里程碑，不是 `drawArrays`，也不是 WebGL 左下原点或 clip Z `-w..w`）；`#version 300 es` 的子集词法/语法；只含 `load_const` 和 `move` 的解释器。光照旋转立方体、状态机、present、`readPixels` 和 JavaScript façade 仍未开始。
5. 标识保持 `CpuWebGl2LearningTrack — NOT WEBGL CONFORMANT`。支持矩阵按实际能力写成 `implemented` 或 `partial`，不能把已经有测试的切片继续写成全部 `NotStarted`，也不能把 partial 说成 WebGL 入口已经可用。

## 仍未开始的步骤

1. 把 glslang `16.6.0` 接成 native CI 的 accept/reject oracle，只比较子集内外的接受与拒绝，不把它编译进 Wasm。
2. 在本 crate 内补 WebGL 窗口空间：左下原点和 clip Z `-w..w`。在那之前，屏幕空间三角形保持 partial。
3. 让解释器执行的顶点结果驱动三角形，而不是继续使用手工屏幕坐标。立方体和光照晚于这条垂直切片。
4. 扩展 bytecode，使单步记录能覆盖 uniform、varying 和更多运算。然后再做调试器 UI。
5. mutable state、`drawArrays` / `drawElements`、`readPixels`、Canvas 2D present。
6. JavaScript 形状的 adapter 放在最后，并且永远不是浏览器原生 `WebGL2RenderingContext`。

叙事说明、目录草图和命令见 [WebGL2 学习轨道](../webgl2-learning-track.zh-CN.md)。机器可读矩阵见 [`webgl2-learning.toml`](../support-matrix/webgl2-learning.toml)。
