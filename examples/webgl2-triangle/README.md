# WebGL2 三角形学习切片

[`p0_vertex.essl`](p0_vertex.essl) 是这条切片的顶点着色器。`glgpu-webgl` 把它解析成 AST，降低成自有 bytecode，再对三个顶点各跑一次 CPU 解释器。解释器写出 `v_clip_*` 和 `v_color_*`。光栅器按 WebGL 的 clip 体积 `-w <= x, y, z <= w` 裁剪，透视除法之后窗口原点在左下。标识保持 `CpuWebGl2LearningTrack — NOT WEBGL CONFORMANT`。

Framebuffer 第 0 行仍是图像上方，用来和 WebGPU P0 三角形保持同一画面朝向。存储坐标是 `storage_y = height - window_y`。这是过渡布局，不是 WebGL 的窗口原点。`drawArrays` 还没有。子集没有 `vec4` 和 `gl_Position`，所以 clip 位置用四个 float 输出表示。

[`float_local.essl`](float_local.essl) 仍是更小的解释器夹具，只覆盖 `load_const` 和 `move`。它不产生三角形。

光照旋转立方体还没有做。ESSL 的 accept/reject 参照是 Khronos glslang 16.6.0，当前测试不调用它。说明见 [WebGL2 学习轨道](../../docs/webgl2-learning-track.zh-CN.md)。
