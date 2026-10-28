# WebGL2 学习轨道

> 状态：学习切片。不是 WebGL 实现，也不是合规实现。  
> 日期：2026-10-27  
> 决策：[ADR 0003](adr/0003-parallel-webgl2-learning-track.md)  
> 产品顺序：不变。WebGPU P0 仍是主线，WebGL 产品实现仍是路线图上的 P7a+。

## 这条轨道要学什么

GLGPU 要在 CPU 上把图形 API 从 0 做到 1，用来学习规范，而不是把浏览器已经提供的 GPU API 包一层。WebGPU 风格的路径已经有 P0 三角形。WebGL2 轨道按同一个目标并行推进，并且是**独立栈**：代码只在 `crates/glgpu-webgl`，不依赖 `glgpu-core`、`glgpu-raster`、`glgpu-shader`、`glgpu-webgpu` 或其他 WebGPU 轨道 crate。

执行发生在本 crate 的 CPU 代码里。它不调用浏览器原生 WebGL，也不调用浏览器原生 WebGPU：

- 不使用 `WebGLRenderingContext` / `WebGL2RenderingContext`；
- 不包装 `canvas.getContext("webgl2")`；
- 不使用 `navigator.gpu`；
- 不把宿主 WebGL / WebGPU 绘制当作后端。

宿主 GPU 即使以后出现，也只能作为测试 oracle。标识字符串保持 `CpuWebGl2LearningTrack — NOT WEBGL CONFORMANT`。

早期画面目标与 WebGPU P0 相同：清屏加一个彩色三角形。光照旋转立方体晚于这个切片。三角形可以由 CPU 解释器的顶点输出驱动。Clip 体积是 WebGL 的 `-w <= x, y, z <= w`，窗口 y 向上、原点在左下。Framebuffer 第 0 行仍是图像上方，这样 P0 画面不用在呈现时再翻转；换算是 `storage_y = height - window_y`。这是过渡存储，不是 WebGL 窗口原点。`drawArrays` 仍然是 `NotStarted`。

## 规范基线

阅读基线写在 [`spec-lock.toml`](../spec-lock.toml) 的 `[learning_track.webgl2.baseline]`，不是合规声明：

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

## 现在已经落地的切片

`WebGl2LearningTrack::create_context` 得到一块 CPU RGBA8 framebuffer。`clear` 覆盖全部像素。`draw_screen_space_triangle` 接收已经落在存储坐标里的三角形，y 向下，用采样点中心的重心坐标覆盖。`draw_clip_space_triangle` 先按 WebGL 体积裁剪，再做透视除法和左下窗口，然后写入这块 framebuffer。`draw_shader_triangle` 对三个顶点各执行一次解释器，读取 `v_clip_*` 和 `v_color_*`。`p0_milestone_triangle` 与这条 shader 路径使用同一套投影，画面朝向与 WebGPU P0 demo 对齐。像素覆盖规则是本 crate 自己的，不保证与 `glgpu-raster` 逐像素相同。没有深度缓冲。

ESSL 子集可以解析两个夹具：

- [`examples/webgl2-triangle/float_local.essl`](../examples/webgl2-triangle/float_local.essl)：`load_const` 和 `move`。它不产生三角形。
- [`examples/webgl2-triangle/p0_vertex.essl`](../examples/webgl2-triangle/p0_vertex.essl)：一个 uniform 标量乘法，加上 `in` / `out` float，写出 clip 位置和颜色。

子集目前是：

- `#version 300 es`
- `uniform` / `in` / `out` 的 `float`
- `void` 函数
- `float` 局部变量、赋值，以及一次 `*`

没有 `vec4`，也没有 `gl_Position`。接口变量会进入 bytecode。解释器可以读 uniform 和 `in`，可以写 `out`。每条指令带源码 span；每一步记录局部变量，以及 uniform、input、output 的值。单步调试器以后读这些记录；现在没有暂停控制，也没有调试 UI。`out` 不能读。未绑定的 uniform 在执行时报错，而不是变成未定义值。

支持矩阵见 [`webgl2-learning.toml`](support-matrix/webgl2-learning.toml)。`require` 只对 `Supported` 成功。Partial 行有对应方法，但不是 WebGL 入口。

## 目录

```text
crates/glgpu-webgl/                         # 独立栈，零外部依赖，不依赖 WebGPU 轨道 crate
  src/lib.rs                                # 能力矩阵、NOT WEBGL CONFORMANT
  src/framebuffer.rs                        # RGBA8 clear；第 0 行是图像上方
  src/clip.rs                               # WebGL clip 体积、左下窗口、过渡存储的 y 换算
  src/raster.rs                             # 存储坐标三角形，以及 clip / shader 绘制
  src/essl.rs                               # 子集词法、语法、AST、源码 span
  src/vm.rs                                 # bytecode、lowering、CPU 解释器、步骤记录
docs/adr/0003-parallel-webgl2-learning-track.md
docs/webgl2-learning-track.zh-CN.md         # 本文
docs/support-matrix/webgl2-learning.toml
examples/webgl2-triangle/p0_vertex.essl     # 驱动三角形的顶点着色器
examples/webgl2-triangle/float_local.essl   # 只含 load_const / move 的解释器夹具
```

`glgpu-web` 的调试页继续只加载 WebGPU P0。不要在那一页上增加 WebGL2 按钮。

## 下一步

1. 在 native CI 里调用 glslang `16.6.0`，只做 accept/reject 对照，不把它编进 Wasm。
2. 补 `vec4` / `gl_Position`、透视正确的 varying，以及深度测试。立方体和光照放在这之后。
3. 调试器 UI。span、逐步局部变量和接口快照已经有了。
4. mutable state、`drawArrays`、`readPixels` 和 Canvas 2D present。呈现时把存储第 0 行放在图像上方，不要再翻一次 y。
5. JavaScript adapter 放在最后，并且不是浏览器原生 `WebGL2RenderingContext`。

WebGPU 主线的下一程不变：项目自有 shader IR 和 WGSL 的 CPU 执行。两条轨道不共享 crate。

## 相关文档

- [ADR 0001：WebGPU-first，WebGL 产品实现延后](adr/0001-webgpu-first.md)
- [ADR 0003：本轨道的决策与补充](adr/0003-parallel-webgl2-learning-track.md)
- [P0 API 契约](api-reference.zh-CN.md)（不覆盖 `glgpu-webgl`）
- [实施路线图](roadmap.zh-CN.md)
- [WebGL2 学习支持矩阵](support-matrix/webgl2-learning.toml)
- [示例说明](../examples/webgl2-triangle/README.md)
