# ADR 0003：并行的 webgl-mock

- 状态：Accepted（2026-10-27 补充决定取代第 5、6 条；不取代 ADR 0001）
- 日期：2026-10-27
- 决策范围：webgl-mock 的目标、禁止的后端，以及当前允许落地的范围

> 更名记录：本文早期标题和标识使用过 `WebGl2LearningTrack`、`LearningContext`、`CpuWebGl2LearningTrack`，以及「WebGL2 学习轨道」。那些名字只作历史对照。当前入口是 `WebGlMock`，三角形 demo 的上下文是 `DemoContext`，标识是 `webgl-mock — NOT WEBGL CONFORMANT`。

## 术语

- **webgl-mock / 模拟 WebGL**：本决策的 CPU 实现。入口 `WebGlMock`。页面上的三角形是 demo，上下文类型是 `DemoContext`。标识 `webgl-mock — NOT WEBGL CONFORMANT`。
- **webgpu-mock / 模拟 WebGPU**：WebGPU 主线的 CPU 实现。标识保持 `CpuGpuSubset — NOT WEBGPU CONFORMANT`。
- **原生 WebGL / 原生 WebGPU**：浏览器内建 API。禁止作为渲染后端。

## 线程模型

目标：浏览器放进原生 WebGL / 原生 WebGPU 内部的工作，在本项目里放在 **Worker + Wasm 后端**。管线结果先写入 Worker 的离屏缓冲区，不直接画到页面 canvas。之后才经 Canvas 2D present 路径把这块缓冲区交到可见 canvas。主线程暴露 Web API façade。主线程与该后端之间的高效通信是 **SharedArrayBuffer**，以及可转移 `ArrayBuffer` / `OffscreenCanvas` 的零拷贝转移。普通 `postMessage` 拷贝不是这条高效路径。GL/GPU 语义不在页面 canvas 上执行。

现状：`/webgl.html` 仍把 `glgpu-webgl-web` 作为主线程上的单线程 Wasm 跑 webgl-mock，再用 Canvas 2D `putImageData` 把像素交到页面 canvas。还没有 Web API façade，也没有 Worker 离屏缓冲区，更没有 SharedArrayBuffer 或零拷贝转移这条通道。这次不把 demo 迁到 Worker。WebGPU 主线上的 Rayon worker 只执行 tile kernel，同样不是这条目标模型。

## 背景

仓库在不调用原生 WebGL / 原生 WebGPU 的前提下，自己实现相近的 API，作为运行在 CPU 上的模拟。当前 `main` 的主线是 webgpu-mock，也就是 P0 垂直切片。页面契约仍然是 `CpuGpuSubset — NOT WEBGPU CONFORMANT`。

[ADR 0001](0001-webgpu-first.md) 把产品顺序定为 WebGPU 优先，ESSL 与 WebGL 的产品实现放在 P7a+。该决定继续有效。公共 IR、光栅约定和 capture schema 还没有稳定，不能在产品主线上同时展开两套可执行栈。

模拟 WebGL 需要一条看得见的 CPU 路径。否则仓库只表达 webgpu-mock，后加入的人无法判断 WebGL2 的边界在哪里。因此允许在 P7a 产品 gate 之前，用独立 crate 和文档开始并行的 webgl-mock。这条栈提前于路线图里的产品实现，但它只搭建骨架，不把 P7 标成进行中，也不是 ADR 0001 所拒绝的“从第一天同时实现 WebGPU 与 WebGL”。

## 决策

1. 新增并行的 webgl-mock。目标与 webgpu-mock 相同：从 0 到 1，在 CPU 上从零实现一套完整的 WebGL2 风格栈。执行、校验和光栅都写在本仓库的软件路径里。页面上的三角形是 demo。
2. 这条栈不依赖原生 WebGL 或原生 WebGPU。禁止把 `WebGLRenderingContext`、`canvas.getContext("webgl2")`、`navigator.gpu` 或任何宿主 GPU 绘制调用当作后端、参考执行器或脚手架依赖。宿主 GPU 将来若出现，也只能作为测试 oracle，不能成为渲染后端；这一条与 ADR 0001 对宿主 WebGPU / WebGL 的拒绝一致。
3. 最终画面仍按项目既有方向，把 CPU framebuffer 转成 RGBA8，经 Canvas 2D `putImageData` 呈现。目标是先写入 Worker 离屏缓冲区，再经 Canvas 2D present 到可见 canvas；页面 canvas 不是管线的绘制目标。本决策的脚手架还没有接入呈现；当前 demo 页在主线程壳 crate 里把像素直接交给页面 canvas，那不是 `DemoContext` 的 present 方法。
4. 产品顺序不变。ADR 0001 仍是主线。P0 的 `NOT WEBGPU CONFORMANT` 文案不得删除或改成合规声明。webgl-mock 自己的永久标识是 `webgl-mock — NOT WEBGL CONFORMANT`。支持矩阵在实现对应行为之前只能记录 `NotStarted`，不能把缺口报成已支持。
5. 首个变更只放入脚手架。**这一条已被文末补充决定取代：** 这条 CPU 栈可以在 `glgpu-webgl` 内继续实现独立的 CPU 子集，但仍然不实现 WebGL 状态机、`drawArrays` / `drawElements`、`readPixels`、JavaScript façade 或浏览器上下文包装。
6. 早期文字要求以后复用公共 CPU 内核。**这一条已被文末补充决定取代：** 这条栈保持独立，不依赖 WebGPU crate，也不调用浏览器渲染器。
7. 若要把产品路线从“P7a+ 才实现 WebGL”改成“主线提前交付可运行的 WebGL2”，必须再写一份 ADR。本 ADR 不构成那次重新评估。`spec-lock.toml` 里 `[deferred.webgl]` 的产品阶段和禁止项保持不变。

crate 名沿用架构规划第 6 节已经写过的 `glgpu-webgl`。调试页 `glgpu-web` 继续只服务 P0 WebGPU 路径。**“不要在那一页上放 WebGL2 入口”已被文末 2026-10-28 站点切换补充取代：** 站点顶栏在两个独立 HTML 入口之间整页重载，WebGPU Wasm 本身仍然不执行 ESSL。

## 后果

### 正面

- WebGL2 在仓库里有独立入口，读代码的人能看到它是纯 CPU、从零开始，而不是封装浏览器。
- P0 crate、`CpuGpuSubset` 行为和 `NOT WEBGPU CONFORMANT` 契约保持原样。
- 支持矩阵从第一天就公开缺口。补充决定之后，只有有测试的 CPU 子集可以标成 implemented 或 partial。

### 成本

- 路线图上的 P7a–P9 仍然是产品交付 gate。这条 CPU 栈上的子集不能被引用成“WebGL 产品已经开工”或 “WebGL conformant”。
- `glgpu-webgl` 的测试锁定这条独立栈的行为，不是 WebGL 验收，也不证明与 WebGPU P0 光栅逐像素相同。
- 这条栈不复用 WebGPU 路径的 framebuffer。窗口原点和 clip Z 的 CPU 子集见文末 2026-10-28 补充；framebuffer 第 0 行仍是过渡的图像上方。

## 明确拒绝的做法

### 用浏览器 WebGL 或 WebGPU 完成绘制

拒绝。目标是自己实现栈。`web-sys` 的 `WebGLRenderingContext`、`navigator.gpu` 和宿主 `drawArrays` 都不能作为 `glgpu-webgl` 的依赖或后端。

### 在一次变更里实现完整 WebGL2

拒绝。状态机、完整 ESSL、资源模型、`drawArrays` 和 conformance 仍是后续步骤。补充决定只接受独立 crate 里的 CPU 子集。

### 改写 P0 来“顺便”容纳 WebGL 调用

拒绝。WebGPU 风格 API 与 WebGL 风格 API 保持分离。这条 CPU 栈另外不得依赖 WebGPU crate；产品主线也不为了 WebGL 去改 P0。

## 补充决定（2026-10-27，开始落地）

本补充取代上面第 5 条和第 6 条，不取代 ADR 0001，也不修改 `[deferred.webgl]`。

1. 这条栈是独立的。`crates/glgpu-webgl` 不得依赖 `glgpu-core`、`glgpu-trace`、`glgpu-raster`、`glgpu-shader`、`glgpu-webgpu`、`glgpu-canvas2d` 或 `glgpu-web`。ESSL 前端留在这个 crate 里，不放进使用 Naga 的 `glgpu-shader`。WebGL 的 clip 空间和窗口原点以后也在这个 crate 里实现。
2. 规范阅读基线冻结在 `spec-lock.toml` 的 `[webgl_mock]`：Khronos WebGL 2.0.0（2017-04-11），GLSL ES 3.00.6（2016-01-29）。ESSL 行为的 accept/reject 参照是 Khronos glslang `16.6.0`。该工具是外部 oracle，本 crate 不链接、不调用它。Naga 的 desktop GLSL 前端不是 ESSL 实现。
3. 编译路径定为：ESSL 源码 → AST → 项目自有、可调试的 bytecode IR → CPU 解释器。第一阶段不做机器码 JIT。指令上的 source span 和逐步局部变量快照留给以后的单步调试器；调试器 UI 本身还没有。
4. 当时允许的实现止于 CPU 子集：RGBA8 clear；一个已经变换到 framebuffer 坐标的三角形（左上原点，用来对齐 WebGPU P0 三角形 demo 的里程碑）；`#version 300 es` 的子集词法/语法；只含 `load_const` 和 `move` 的解释器。这条现状已被 2026-10-28 补充取代。光照旋转立方体、状态机、present、`readPixels` 和 JavaScript façade 在该补充之后仍未开始。
5. 标识保持 `webgl-mock — NOT WEBGL CONFORMANT`。支持矩阵按实际能力写成 `implemented` 或 `partial`，不能把已经有测试的子集继续写成全部 `NotStarted`，也不能把 partial 说成 WebGL 入口已经可用。

## 补充决定（2026-10-28，clip 与 shader 三角形）

本补充更新 2026-10-27 补充决定里对“当前实现止于左上屏幕三角形和 `load_const` / `move`”的现状描述。不取代 ADR 0001，也不修改 `[deferred.webgl]`。

1. `glgpu-webgl` 在本 crate 内做 WebGL clip 体积 `-w <= x, y, z <= w`。近平面是 `z = -w`。窗口原点在左下，y 向上。
2. Framebuffer 第 0 行仍是图像上方，用来保持和 WebGPU P0 三角形相同的画面朝向。换算是 `storage_y = height - window_y`。这是过渡存储，不是 WebGL 窗口原点。以后的 Canvas 2D present 不应再翻一次 y。
3. 三角形可以由解释器写出的 `v_clip_*` 和 `v_color_*` 驱动。bytecode 增加 uniform / input 读取、output 写入和一次标量乘法。源码 span、逐步局部变量，以及 uniform / input / output 快照都保留。这仍然不是 `drawArrays`。
4. 子集仍是标量 float，没有 `vec4` 和 `gl_Position`。没有深度缓冲。光照旋转立方体、状态机、present、`readPixels`、JavaScript façade、单步调试器 UI、JIT 和 glslang CI 在该补充写下时仍未开始。同日的站点切换补充允许一个独立的调试页复制 RGBA8，那不是本 crate 的 present 方法。

## 补充决定（2026-10-28，同站 WebGL demo 页）

本补充取代“不要在 WebGPU 调试页上增加 WebGL2 按钮”。不取代 ADR 0001，也不修改 `[deferred.webgl]`。

1. 同一站点的顶栏是 `WebGPU | WebGL`。点另一侧会整页重载。默认仍是 WebGPU：`web/index.html` 只启动 `glgpu-web`。WebGL 是 `web/webgl.html`，只启动 `crates/glgpu-webgl-web`。两条 Wasm 入口互不加载。
2. `glgpu-webgl-web` 只依赖 `glgpu-webgl`。它不依赖 `glgpu-webgpu`、`glgpu-core`、`glgpu-raster`、`glgpu-shader`、`glgpu-trace`、`glgpu-canvas2d` 或 `glgpu-web`。页面用 Canvas 2D `putImageData` 复制本栈 RGBA8，不调用 `getContext("webgl")`、`getContext("webgl2")` 或 `navigator.gpu`。
3. Demo 页编辑 ESSL 子集，编译失败给出源码位置，运行时用 CPU 解释器画一个与 WebGPU P0 同朝向的彩色三角形。标识保持 `webgl-mock — NOT WEBGL CONFORMANT`。这不是 `drawArrays`，不是单步调试器，也不是 JavaScript WebGL façade。`DemoContext` 仍然没有 present 方法。
4. 链接使用相对 URL，供本地 `/` 和 GitHub Pages 项目站点 `/glgpu/` 共用。不要加 `<base>`。

## 仍未开始的步骤

1. 把 glslang `16.6.0` 接成 native CI 的 accept/reject oracle，只比较子集内外的接受与拒绝，不把它编译进 Wasm。
2. 补 `vec4` / `gl_Position`、透视正确的 varying，以及深度测试。立方体和光照晚于这条垂直切片。
3. 单步调试器 UI。调试页会整段运行，但没有暂停或逐步控制。span、逐步局部变量和接口快照已经有了。
4. mutable state、`drawArrays` / `drawElements`、`readPixels`，以及 `DemoContext` 自己的 present 方法。调试页在壳 crate 里把已经按图像上方存储的 RGBA8 交给 Canvas 2D，不再翻一次 y。
5. JavaScript 形状的 adapter 放在最后，并且永远不是浏览器原生 `WebGL2RenderingContext`。

叙事说明、目录草图和命令见 [webgl-mock](../webgl-mock.zh-CN.md)。机器可读矩阵见 [`webgl-mock.toml`](../support-matrix/webgl-mock.toml)。
