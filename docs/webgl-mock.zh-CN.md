# webgl-mock

GLGPU 本质上是实验性的，目的是亲自把项目做一遍，抓住 WebGL 与 WebGPU 的核心想法，而不是做成完整的、可上生产的实现。webgl-mock 保持精简：保真度只要够理解 WebGL 的核心，不为求完备而过度设计。产品名称仍是 webgl-mock，不是原生 WebGL。

> 状态：CPU 子集。是模拟 WebGL，不是原生 WebGL，也不是合规实现。  
> 日期：2026-10-27  
> 决策：[ADR 0003](adr/0003-parallel-webgl-mock.md)  
> 产品顺序：不变。webgpu-mock 的 P0 仍是主线，WebGL 产品实现仍是路线图上的 P7a+。

## 术语

- **webgl-mock / 模拟 WebGL**：本文这条 CPU 实现。入口 `WebGlMock`，三角形 demo 的上下文是 `DemoContext`。
- **webgpu-mock / 模拟 WebGPU**：WebGPU 主线的 CPU 实现。标识保持 `CpuGpuSubset — NOT WEBGPU CONFORMANT`。
- **原生 WebGL / 原生 WebGPU**：浏览器内建 API。本栈不调用它们。

## 现有实现与未来设计

下表只写仓库里核对过的代码，不把决定写成已经落地。

| 设计点 | 状态 | 目前代码 |
| --- | --- | --- |
| 产品名 webgl-mock / webgpu-mock；三角形页是 demo | 现有 | `WebGlMock`、`DemoContext`，标识 `webgl-mock — NOT WEBGL CONFORMANT`。webgpu 标识仍是 `CpuGpuSubset — NOT WEBGPU CONFORMANT` |
| RGBA8 clear、WebGL clip、ESSL→bytecode→CPU 解释器、shader 三角形 | 现有 | `DemoContext` 持有 `Vec<Rgba8>`，`clear` 与 `draw_shader_triangle` 在 `glgpu-webgl`。没有 `drawArrays`，也没有 present 方法 |
| demo 页在主线程把像素贴到页面 canvas | 现有 | `glgpu-webgl-web` 在主线程调用 `get_context("2d")` 和 `put_image_data`。像素来自 `to_rgba8_bytes()` |
| 不调用原生 WebGL / 原生 WebGPU，也不使用 OffscreenCanvas | 现有 | 页面只拿 Canvas 2D。代码里没有 OffscreenCanvas，也没有 `getContext("webgl")` |
| Worker + Wasm 后端；主线程只做 Web API façade | 未来 | 这是未来的设计，目前还未实现。目前 webgl 页的 Wasm 跑在主线程 |
| SharedArrayBuffer 双缓冲 + Atomics；back 槽是逻辑绘制缓冲 | 未来 | 这是未来的设计，目前还未实现。目前绘制缓冲是 `DemoContext` 里的 `Vec<Rgba8>`，不是 SAB 的槽 |
| 三缓冲不是默认 | 未来约束 | 目前既没有双槽，也没有第三块呈现缓冲 |
| 实验性、亲自实现以抓住核心 | 原则 | 写在文档里，不是某个代码开关 |

## 线程模型

下面 1–5 是未来的设计，目前还未实现。目前是用主线程上的 `DemoContext`（`Vec<Rgba8>`）光栅，再 `to_rgba8_bytes` 后用 Canvas 2D `putImageData` 交到页面 canvas。

已决定（2026-10-28）：呈现使用一块 **SharedArrayBuffer 双缓冲**。不使用 OffscreenCanvas。此前方案 C 的 OffscreenCanvas blit 已撤销。浏览器放进原生 WebGL / 原生 WebGPU 内部的工作，放在 **Worker + Wasm 后端**。主线程暴露 Web API façade。这次只锁定方案，不实现 Worker 后端。

1. 同一块 SharedArrayBuffer 里有两个像素槽。front 给主线程呈现，back 给 Worker 光栅。交接用 Atomics（帧序号，以及哪一块可读）。不把这唯一的缓冲 transfer 走。
2. 逻辑绘制缓冲就是当前可写的 back 槽，Wasm 视图映射到这个槽。它是光栅结果、framebuffer hash、单元测试和像素调试的权威来源，对应原生 WebGL 的 drawing buffer。不为每一帧再保留一份独立的全帧逻辑 framebuffer 再拷进 SharedArrayBuffer。以后若要离屏 FBO 或非呈现尺寸，才可能另留。
3. 三缓冲不是默认，只有测到呈现停顿才考虑。
4. 主线程用 Canvas 2D 呈现 front 槽（`putImageData` / `ImageData`）。`ImageData` 不能直接接收共享视图时，只从当前 front 槽复制，这不是另一份逻辑绘制缓冲。页面 canvas 只负责呈现。管线不把可见页面 canvas 当作 GL 绘制目标。没有 OffscreenCanvas blit。
5. 高效通信的主路径是这块双缓冲 SharedArrayBuffer 和 Atomics。若以后需要一份副本快照，可以用零拷贝转移；那不是主路径。像素载荷不用拷贝式 `postMessage`。

这是未来的设计，目前还未实现。目前 `/webgl.html` 是主线程上的单线程 Wasm：在主线程运行 webgl-mock，再用 Canvas 2D `putImageData` 把 `Vec<u8>` 直接交到页面 canvas。还没有 Web API façade，也没有这块双缓冲 SharedArrayBuffer。

## 这条栈做什么

GLGPU 在 CPU 上做 webgpu-mock 和 webgl-mock，不把浏览器已经提供的原生 GPU API 包一层。webgpu-mock 已经有 P0 三角形 demo。webgl-mock 按同一个方向并行推进，并且是**独立栈**：代码只在 `crates/glgpu-webgl`，不依赖 `glgpu-core`、`glgpu-raster`、`glgpu-shader`、`glgpu-webgpu` 或其他 webgpu-mock crate。

执行发生在本 crate 的 CPU 代码里。它不调用原生 WebGL，也不调用原生 WebGPU：

- 不使用 `WebGLRenderingContext` / `WebGL2RenderingContext`；
- 不包装 `canvas.getContext("webgl2")`；
- 不使用 `navigator.gpu`；
- 不把宿主 WebGL / WebGPU 绘制当作后端。

宿主 GPU 即使以后出现，也只能作为测试 oracle。标识字符串保持 `webgl-mock — NOT WEBGL CONFORMANT`。`WebGlMock::create_context` 的类型是 `DemoContext`。

早期画面与 WebGPU P0 相同：清屏加一个彩色三角形 demo。Demo 页已经能编辑并运行这个三角形。光照旋转立方体晚于这个子集。三角形可以由 CPU 解释器的顶点输出驱动。Clip 体积是 WebGL 的 `-w <= x, y, z <= w`，窗口 y 向上、原点在左下。Framebuffer 第 0 行仍是图像上方，这样 P0 画面不用在呈现时再翻转；换算是 `storage_y = height - window_y`。这是过渡存储，不是 WebGL 窗口原点。`drawArrays` 仍然是 `NotStarted`。

## 规范基线

阅读基线写在 [`spec-lock.toml`](../spec-lock.toml) 的 `[webgl_mock.baseline]`，不是合规声明：

| 对象 | 基线 |
| --- | --- |
| WebGL 2 | Khronos WebGL 2.0.0，2017-04-11，<https://registry.khronos.org/webgl/specs/2.0.0/> |
| 编辑草案 | <https://registry.khronos.org/webgl/specs/latest/2.0/>，只作跟踪，不替代 2.0.0 |
| ESSL | OpenGL ES Shading Language 3.00.6，2016-01-29 |
| accept/reject 参照 | Khronos glslang `16.6.0` |

glslang 用来判断一份 ESSL 应该被接受还是拒绝。它不链接进 `glgpu-webgl`，当前测试也不调用它。Naga 的 desktop GLSL 前端不是 ESSL，不能拿来决定子集是否合法。

编译路径是：

```text
ESSL 源码 → AST → 自有 bytecode IR → CPU 解释器
```

第一阶段不做机器码 JIT。每条指令带源码 span，解释器在每步之后记录局部变量快照。单步调试器以后读这些记录；现在没有暂停控制，也没有调试 UI。

## 现在已经落地的 CPU 子集

`WebGlMock::create_context` 得到一块 CPU RGBA8 framebuffer，类型是 `DemoContext`。`clear` 覆盖全部像素。`draw_screen_space_triangle` 接收已经落在存储坐标里的三角形，y 向下，用采样点中心的重心坐标覆盖。`draw_clip_space_triangle` 先按 WebGL 体积裁剪，再做透视除法和左下窗口，然后写入这块 framebuffer。`draw_shader_triangle` 对三个顶点各执行一次解释器，读取 `v_clip_*` 和 `v_color_*`。`p0_milestone_triangle` 与这条 shader 路径使用同一套投影，画面朝向与 WebGPU P0 demo 对齐。像素覆盖规则是本 crate 自己的，不保证与 `glgpu-raster` 逐像素相同。没有深度缓冲。

页面上的三角形是 demo，不是一套单独命名的上下文产品。`DemoContext` 没有 present 方法。

ESSL 子集可以解析两个夹具：

- [`examples/webgl2-triangle/float_local.essl`](../examples/webgl2-triangle/float_local.essl)：`load_const` 和 `move`。它不产生三角形。
- [`examples/webgl2-triangle/p0_vertex.essl`](../examples/webgl2-triangle/p0_vertex.essl)：一个 uniform 标量乘法，加上 `in` / `out` float，写出 clip 位置和颜色。

子集目前是：

- `#version 300 es`
- `uniform` / `in` / `out` 的 `float`
- `void` 函数
- `float` 局部变量、赋值，以及一次 `*`

没有 `vec4`，也没有 `gl_Position`。接口变量会进入 bytecode。解释器可以读 uniform 和 `in`，可以写 `out`。每条指令带源码 span；每一步记录局部变量，以及 uniform、input、output 的值。Demo 页会整段运行这段记录所在的解释器，但没有单步暂停。`out` 不能读。未绑定的 uniform 在执行时报错，而不是变成未定义值。

支持矩阵见 [`webgl-mock.toml`](support-matrix/webgl-mock.toml)。`require` 只对 `Supported` 成功。Partial 行有对应方法，但不是 WebGL 入口。

## 目录

```text
crates/glgpu-webgl/                         # webgl-mock，零外部依赖，不依赖 webgpu-mock crate
  src/lib.rs                                # 能力矩阵、NOT WEBGL CONFORMANT
  src/framebuffer.rs                        # RGBA8 clear；第 0 行是图像上方；DemoContext
  src/clip.rs                               # WebGL clip 体积、左下窗口、过渡存储的 y 换算
  src/raster.rs                             # 存储坐标三角形，以及 clip / shader 绘制
  src/essl.rs                               # 子集词法、语法、AST、源码 span
  src/vm.rs                                 # bytecode、lowering、CPU 解释器、步骤记录
crates/glgpu-webgl-web/                     # webgl-mock demo 页；当前主线程单线程 Wasm；只依赖 glgpu-webgl；不是原生 WebGL façade
docs/adr/0003-parallel-webgl-mock.md
docs/webgl-mock.zh-CN.md                    # 本文
docs/support-matrix/webgl-mock.toml
examples/webgl2-triangle/p0_vertex.essl     # 驱动三角形 demo 的顶点着色器
examples/webgl2-triangle/float_local.essl   # 只含 load_const / move 的解释器夹具
web/webgl.html                              # webgl-mock demo 页 loader；当前主线程
web/index.html                              # webgpu-mock 调试页 loader；顶栏链到 webgl.html
```

站点顶栏是 `WebGPU | WebGL`。点另一侧会整页重载，默认仍是 WebGPU。

- `web/index.html` 只启动 `glgpu-web` 的 WGSL / P0 调试页。
- `web/webgl.html` 只启动 `crates/glgpu-webgl-web`。那个壳只依赖 `glgpu-webgl`，编辑并编译当前 ESSL 子集，再用 CPU 解释器画三角形 demo。画面经 Canvas 2D `putImageData` 呈现，不调用浏览器 WebGL 或 WebGPU。
- 两条 Wasm 包互不加载。`glgpu-web` 里没有 WebGL 渲染按钮。

本地：

```bash
cargo xtask build-web --release
cargo xtask build-web --release --crate crates/glgpu-webgl-web --out-dir web/pkg-webgl --out-name glgpu_webgl_web
cargo xtask serve
```

打开 `/` 与 `/webgl.html`。GitHub Pages 项目站点对应 `/glgpu/` 与 `/glgpu/webgl.html`。链接都是相对 URL，不要加 `<base>`。

## 下一步

1. 在 native CI 里调用 glslang `16.6.0`，只做 accept/reject 对照，不把它编进 Wasm。
2. 补 `vec4` / `gl_Position`、透视正确的 varying，以及深度测试。立方体和光照放在这之后。
3. 单步调试器 UI。调试页会整段运行，但没有暂停。span、逐步局部变量和接口快照已经有了。
4. mutable state、`drawArrays`、`readPixels`，以及 `DemoContext` 自己的 present 方法。调试页已经把存储第 0 行直接交给 Canvas 2D，不再翻一次 y。
5. JavaScript adapter 放在最后，并且不是浏览器原生 `WebGL2RenderingContext`。

WebGPU 主线的下一程不变：项目自有 shader IR 和 WGSL 的 CPU 执行。两条栈不共享 crate。

## 相关文档

- [ADR 0001：WebGPU-first，WebGL 产品实现延后](adr/0001-webgpu-first.md)
- [ADR 0003：这条 CPU 栈的决策与补充](adr/0003-parallel-webgl-mock.md)
- [P0 API 契约](api-reference.zh-CN.md)（不覆盖 `glgpu-webgl`）
- [实施路线图](roadmap.zh-CN.md)
- [webgl-mock 支持矩阵](support-matrix/webgl-mock.toml)
- [三角形 demo 说明](../examples/webgl2-triangle/README.md)
