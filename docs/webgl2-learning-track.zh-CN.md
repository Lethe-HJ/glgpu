# WebGL2 学习轨道

> 状态：脚手架。没有可运行的 WebGL2 实现。  
> 日期：2026-10-27  
> 决策：[ADR 0003](adr/0003-parallel-webgl2-learning-track.md)  
> 产品顺序：不变。WebGPU P0 仍是主线，WebGL 产品实现仍是路线图上的 P7a+。

## 这条轨道要学什么

GLGPU 要在 CPU 上把图形 API 从 0 做到 1，用来学习规范，而不是把浏览器已经提供的 GPU API 包一层。WebGPU 风格的路径已经有 P0：`CpuGpuSubset` 在 CPU 上光栅化，再用 Canvas 2D `putImageData` 把 RGBA8 framebuffer 显示出来，并且永久标明 `NOT WEBGPU CONFORMANT`。

WebGL2 轨道按同一个目标并行开始：从零实现一套 WebGL2 风格的栈，执行发生在本仓库的 CPU / reference 代码里。它不调用浏览器原生 WebGL，也不调用浏览器原生 WebGPU。具体地说，实现过程中不使用：

- `WebGLRenderingContext` / `WebGL2RenderingContext`；
- `canvas.getContext("webgl")` 或 `canvas.getContext("webgl2")` 的包装；
- `navigator.gpu`；
- 任何宿主 WebGL / WebGPU 绘制调用。

宿主 GPU 即使以后出现在测试里，也只能作为对照 oracle，不能成为这个轨道的渲染后端。画面输出仍然走项目自己的 CPU framebuffer，再经 Canvas 2D 呈现。当前脚手架还没有这条呈现路径。

这条轨道不是 WebGL 合规实现。标识字符串是 `CpuWebGl2LearningTrack — NOT WEBGL CONFORMANT`。在相应行为、测试和矩阵更新之前，任何能力都保持 `NotStarted`。

## 为什么现在开分支，而产品表仍写 P7a+

[ADR 0001](adr/0001-webgpu-first.md) 和[路线图](roadmap.zh-CN.md) 把 ESSL parser、WebGL 状态机和 JavaScript façade 的产品交付放在 P7a 及以后。这个顺序继续约束主线：P0 不实现这些组件，`[deferred.webgl]` 里的禁止项也不改。

ADR 0001 同时要求：引入 WebGL 时要新写 ADR，不能只改 README 的宣传口径。ADR 0003 记录的是另一件事：允许一条并行学习轨道早于 P7a 把方向和空壳放进仓库，便于对照 WebGPU 路径学习 WebGL2。它不把 P7 改成进行中，也不实现 ADR 0001 明确拒绝的“两套栈同时开工”。

读文档时按这个顺序判断：

1. P0 行为以代码、`docs/support-matrix/webgpu-p0.toml` 和 API 契约为准。
2. 产品阶段以 ADR 0001 和路线图为准，WebGL 实现仍是 `DEFERRED`。
3. WebGL2 学习方向以 ADR 0003、本文和 `docs/support-matrix/webgl2-learning.toml` 为准。本文描述的后续步骤都还没有做。

## 目录草图

草图沿用架构规划第 6 节的 workspace 风格。本变更只添加标了“现在”的路径；其余是以后的边界，避免先拆出一圈空模块。

```text
crates/glgpu-webgl/                         # 现在：学习轨道骨架，零依赖，全部 NotStarted
  Cargo.toml
  src/lib.rs                                # WebGl2LearningTrack、后端枚举、拒绝创建上下文
docs/adr/0003-parallel-webgl2-learning-track.md
docs/webgl2-learning-track.zh-CN.md         # 本文
docs/support-matrix/webgl2-learning.toml    # 现在：conformant = false，能力全部 not_started
examples/webgl2-triangle/README.md          # 现在：占位说明，没有可运行 demo

# 以后才考虑，本变更不创建这些文件：
crates/glgpu-webgl/src/state.rs             # mutable state 子集；产品 gate 仍是 P7b
crates/glgpu-shader/                       # ESSL 3.00 前端应留在 shader crate，而不是塞进 WebGL 对象层
examples/webgl2-triangle/clear.essl         # 真正的 CPU 三角形样例出现之后再添加
```

`glgpu-webgl` 不依赖 `glgpu-webgpu`、`web-sys` 或 `js-sys`。它现在也不依赖 `glgpu-core`：在没有 draw 行为之前，先保持零依赖，避免骨架假装已经接到 framebuffer。以后的垂直切片再显式依赖公共内核。

`glgpu-web` 的调试页继续只加载 WebGPU P0。不要在那一页上增加 WebGL2 按钮，除非单独的变更定义了可观察行为。

## 骨架现在保证什么

`WebGl2LearningTrack` 可以构造，但不分配 GL 对象：

- `backend()` 只有 `CpuFromScratch`；
- `conformant()` 为 `false`；
- `create_context()` 返回 `NotStarted`；
- `ALLOWS_HOST_WEBGL`、`ALLOWS_HOST_WEBGPU`、`ALLOWS_NAVIGATOR_GPU`、`ALLOWS_WEBGL_RENDERING_CONTEXT` 都是 `false`；
- `TrackCapability::ALL` 里每一项的状态都是 `NotStarted`。

这些不变量由 `cargo test -p glgpu-webgl` 锁定。它们证明骨架没有偷偷开始实现，不能被说成 WebGL2 已经能画。

## 建议的后续实现顺序

仍不要在同一次改动里把整栈做完。每一步都要更新 `webgl2-learning.toml`，并补能失败的测试。

1. 冻结学习用的规范基线：Khronos WebGL 2.0 的一个有日期版本，以及 GLSL ES 3.00。写明只用于阅读和子集实现，不是 conformance。不要改 `[deferred.webgl].production_phase`。
2. 设计 Rust-native 类型，名称保持项目风格，例如以后的 `CpuWebGl2Subset`。错误继续结构化。不要生成 WebIDL façade。
3. 做第一条 CPU 垂直切片：清屏和一个三角形进入现有 framebuffer，再显式 present。同步写明与 P0 的坐标差异（WebGL 左下原点、clip Z `-w..w`，WebGPU / 当前 P0 framebuffer 是左上原点）。没有 `RasterConvention` 测试之前，不要把 draw 标成已实现。
4. 垂直切片稳定后，再加一小块 mutable state，并记录每次 draw 冻结了哪些状态。
5. 单独做 ESSL 3.00 的词法、语法和类型检查子集。不要把 Naga 的 GLSL 440 前端当作 ESSL。
6. 最后才讨论 JavaScript 形状的 adapter。即使做了，对象也不是浏览器的 `WebGL2RenderingContext`，页面文案继续保留 `NOT WEBGL CONFORMANT`。

WebGPU 主线的下一程不变：项目自有 shader IR、WGSL lowering 和 CPU 执行。学习轨道不取代那条工作。

## 相关文档

- [ADR 0001：WebGPU-first，WebGL 产品实现延后](adr/0001-webgpu-first.md)
- [ADR 0003：本轨道的决策](adr/0003-parallel-webgl2-learning-track.md)
- [P0 API 契约](api-reference.zh-CN.md)（不覆盖 `glgpu-webgl`）
- [实施路线图](roadmap.zh-CN.md)
- [WebGL2 学习支持矩阵](support-matrix/webgl2-learning.toml)
- [示例占位](../examples/webgl2-triangle/README.md)
