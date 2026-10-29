# webgl-mock

GLGPU 本质上是实验性的，目的是亲自把项目做一遍，抓住 WebGL 与 WebGPU 的核心想法，而不是做成完整的、可上生产的实现。webgl-mock 保持精简：保真度只要够理解 WebGL 的核心，不为求完备而过度设计。产品名称仍是 webgl-mock，不是原生 WebGL。

> 状态：CPU 子集。是模拟 WebGL，不是原生 WebGL，也不是合规实现。  
> 日期：2026-10-27  
> 决策：[ADR 0003](adr/0003-parallel-webgl-mock.md)  
> 仓库位置：webgl-mock 与 webgpu-mock 是对等的两条 CPU 实现，不是主从。

## 仓库位置

```text
demo 层（对照）
  原生 WebGL demo          原生 WebGPU demo
  未来：并排打开，用来对画面。目前仓库里没有这两页。

mock 层（对等）
  webgl-mock               webgpu-mock
  现有：crates/glgpu-webgl  现有：crates/glgpu-webgpu
  页：web/webgl.html        页：web/index.html
```

webgl-mock 与 webgpu-mock 是并行的两种图形技术，不是一条主线、一条附页。原生 WebGL / 原生 WebGPU 只放在 demo 层，给同一画面做对照。例如 webgl-mock 的彩色三角形看不准时，打开对应的原生 demo 比画面。原生 API 不是 mock crate 里面的绘制后端。

这是未来的设计，目前还未实现：仓库里还没有原生 WebGL 或原生 WebGPU 的对照页。目前顶栏的 WebGL / WebGPU 都是 mock 页。

## 术语

- **webgl-mock / 模拟 WebGL**：与 webgpu-mock 对等的 CPU 实现。入口 `WebGlMock`，三角形 demo 的上下文是 `DemoContext`。
- **webgpu-mock / 模拟 WebGPU**：与 webgl-mock 对等的 CPU 实现。标识保持 `CpuGpuSubset — NOT WEBGPU CONFORMANT`。
- **原生 WebGL / 原生 WebGPU**：只在 demo 层做并排对照。不是 mock crate 的绘制后端。对照页目前还没有。

## 现有实现与未来设计

下表只写仓库里核对过的代码，不把决定写成已经落地。

| 设计点 | 状态 | 目前代码 |
| --- | --- | --- |
| 产品名 webgl-mock / webgpu-mock；三角形页是 demo | 现有 | `WebGlMock`、`DemoContext`，标识 `webgl-mock — NOT WEBGL CONFORMANT`。webgpu 标识仍是 `CpuGpuSubset — NOT WEBGPU CONFORMANT` |
| RGBA8 clear、WebGL clip、ESSL→bytecode→CPU 解释器、shader 三角形 | 现有 | `DemoContext` 持有 `Vec<Rgba8>`，`clear` 与 `draw_shader_triangle` 在 `glgpu-webgl`。没有 `drawArrays`，也没有 present 方法 |
| demo 页把像素贴到页面 canvas | 现有 | 跨源隔离时，主线程从 front 槽复制再 `putImageData`。未隔离时，`glgpu-webgl-web` 仍用 `to_rgba8_bytes()`。`DemoContext` 没有 present 方法 |
| mock crate 自己用 CPU 画，不用原生 API 当绘制后端 | 现有 | `glgpu-webgl` 不调用 `getContext("webgl")` 或 `navigator.gpu` |
| demo 层的原生 WebGL / 原生 WebGPU 对照页 | 未来 | 这是未来的设计，目前还未实现。目前没有并排的原生三角形页 |
| 不使用 OffscreenCanvas | 现有 | 代码里没有 OffscreenCanvas |
| Worker + Wasm 后端；主线程做编辑器 façade | 现有，范围有限 | 这是现有设计，代码中已实现：`glgpu-webgl-worker` 在隔离页面里光栅。未隔离时绘制仍在主线程。这不是完整的 WebGL JavaScript façade，也不是 `drawArrays` |
| SharedArrayBuffer 双缓冲 + Atomics；back 槽是逻辑绘制缓冲 | 现有，范围有限 | 这是现有设计，代码中已实现：back 槽就是 `DemoContext` 的像素。命令字节在同一块缓冲里。未隔离时没有这块缓冲 |
| 三缓冲不是默认 | 现有约束 | 代码是双槽，没有第三块呈现缓冲 |
| 实验性、亲自实现以抓住核心 | 原则 | 写在文档里，不是某个代码开关 |

## 线程模型

下面 1–5 是现有设计，代码中已经在跨源隔离的 demo 页这样做。未隔离时仍用主线程上的 `DemoContext`（`Vec<Rgba8>`）光栅，再 `to_rgba8_bytes` 后用 Canvas 2D `putImageData`。

已决定（2026-10-28），并在 2026-10-29 的 demo 页落地：呈现使用一块 **SharedArrayBuffer 双缓冲**。不使用 OffscreenCanvas。此前方案 C 的 OffscreenCanvas blit 已撤销。浏览器放进原生 WebGL / 原生 WebGPU 内部的工作，放在 **Worker + Wasm 后端**。主线程暴露编辑器 façade。这不是完整的 WebGL JavaScript façade。

1. 同一块 SharedArrayBuffer 里有两个像素槽。front 给主线程呈现，back 给 Worker 光栅。交接用 Atomics（帧序号，以及哪一块可读）。不把这唯一的缓冲 transfer 走。
2. 逻辑绘制缓冲就是当前可写的 back 槽，Wasm 视图映射到这个槽。它是光栅结果、framebuffer hash、单元测试和像素调试的权威来源，对应原生 WebGL 的 drawing buffer。不为每一帧再保留一份独立的全帧逻辑 framebuffer 再拷进 SharedArrayBuffer。以后若要离屏 FBO 或非呈现尺寸，才可能另留。
3. 三缓冲不是默认，只有测到呈现停顿才考虑。
4. 主线程用 Canvas 2D 呈现 front 槽（`putImageData` / `ImageData`）。`ImageData` 不能直接接收共享视图时，只从当前 front 槽复制，这不是另一份逻辑绘制缓冲。页面 canvas 只负责呈现。管线不把可见页面 canvas 当作 GL 绘制目标。没有 OffscreenCanvas blit。
5. 高效通信的主路径是这块双缓冲 SharedArrayBuffer 和 Atomics。若以后需要一份副本快照，可以用零拷贝转移；那不是主路径。像素载荷不用拷贝式 `postMessage`。

这是现有设计，代码中已实现。`/webgl.html` 在 `crossOriginIsolated` 时启动 `web/webgl-worker.js`。ESSL 源码写入共享命令区，Worker 调用 `run_commanded_essl`，主线程用 Atomics 选择 front 槽再 `putImageData`。检查器里的 `DemoContext` 只是 front 槽的副本，不是第二份逻辑绘制缓冲。页面没有隔离头时，`glgpuQueueEssl` 不存在可用 Worker，绘制退回主线程。`drawArrays` 仍然是 `NotStarted`。

## 这条栈做什么

webgl-mock 与 webgpu-mock 对等。这条栈是独立 crate：代码只在 `crates/glgpu-webgl`，不依赖 `glgpu-core`、`glgpu-raster`、`glgpu-shader`、`glgpu-webgpu` 或其他 webgpu-mock crate。mock 自己在 CPU 上画，不把原生 WebGL 或原生 WebGPU 拿来当绘制后端。

执行发生在本 crate 的 CPU 代码里。原生对照放在 demo 层，不放进这个 crate：

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

页面上的三角形是 demo，不是一套单独命名的上下文产品。`DemoContext` 没有 present 方法。隔离页面里，这块 `DemoContext` 就是 Worker 的 back 槽。

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
  src/present.rs                            # 双缓冲槽、Atomics 交接、共享命令区
crates/glgpu-webgl-web/                     # webgl-mock 编辑器壳；未隔离时自己光栅
crates/glgpu-webgl-worker/                  # 隔离页面上的 webgl-mock Worker
docs/adr/0003-parallel-webgl-mock.md
docs/webgl-mock.zh-CN.md                    # 本文
docs/support-matrix/webgl-mock.toml
examples/webgl2-triangle/p0_vertex.essl     # 驱动三角形 demo 的顶点着色器
examples/webgl2-triangle/float_local.essl   # 只含 load_const / move 的解释器夹具
web/webgl.html                              # webgl-mock loader；隔离时启动 worker
web/webgl-worker.js                         # webgl-mock Worker 入口
web/present-slots.js                        # 主线程复制 front 槽并 putImageData
web/index.html                              # webgpu-mock 页 loader；与 webgl.html 对等
```

站点顶栏是 `WebGPU | WebGL`，两边都是 mock。点另一侧会整页重载。原生对照页还没有。

- `web/index.html` 启动 `glgpu-web` 的 WGSL / P0 调试页。隔离且没有 `?threads=1` 时，绘制在 `glgpu-webgpu-worker`。
- `web/webgl.html` 启动 `crates/glgpu-webgl-web`。那个壳只依赖 `glgpu-webgl`，编辑并编译当前 ESSL 子集。隔离时三角形由 `glgpu-webgl-worker` 的 CPU 解释器绘制，主线程呈现 front 槽。未隔离时壳自己绘制。画面经 Canvas 2D `putImageData` 呈现，不调用浏览器 WebGL 或 WebGPU。
- webgl-mock 与 webgpu-mock 的 Wasm 包互不加载。`glgpu-web` 里没有 WebGL 渲染按钮。

本地：

```bash
cargo xtask build-web --release
cargo xtask build-web --release --crate crates/glgpu-webgl-web --out-dir web/pkg-webgl --out-name glgpu_webgl_web
cargo xtask build-present-workers
cargo xtask serve
```

打开 `/` 与 `/webgl.html`。GitHub Pages 项目站点对应 `/glgpu/` 与 `/glgpu/webgl.html`。链接都是相对 URL，不要加 `<base>`。

## 下一步

1. 在 native CI 里调用 glslang `16.6.0`，只做 accept/reject 对照，不把它编进 Wasm。
2. 补 `vec4` / `gl_Position`、透视正确的 varying，以及深度测试。立方体和光照放在这之后。
3. 单步调试器 UI。调试页会整段运行，但没有暂停。span、逐步局部变量和接口快照已经有了。
4. mutable state、`drawArrays`、`readPixels`，以及 `DemoContext` 自己的 present 方法。调试页已经把存储第 0 行直接交给 Canvas 2D，不再翻一次 y。
5. JavaScript adapter 放在最后，并且不是浏览器原生 `WebGL2RenderingContext`。

两条 mock 不共享 crate。webgpu-mock 自己的下一程仍是项目自有 shader IR 和 WGSL 的 CPU 执行。

## 相关文档

- [ADR 0001：WebGPU-first，WebGL 产品实现延后](adr/0001-webgpu-first.md)
- [ADR 0003：这条 CPU 栈的决策与补充](adr/0003-parallel-webgl-mock.md)
- [P0 API 契约](api-reference.zh-CN.md)（不覆盖 `glgpu-webgl`）
- [实施路线图](roadmap.zh-CN.md)
- [webgl-mock 支持矩阵](support-matrix/webgl-mock.toml)
- [三角形 demo 说明](../examples/webgl2-triangle/README.md)
