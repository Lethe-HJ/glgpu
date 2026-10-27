# WebGL2 三角形学习切片

这里有两件还没有接在一起的事。

[`float_local.essl`](float_local.essl) 是 ESSL 子集夹具。`glgpu-webgl` 会把它解析成 AST，降低成自有 bytecode，再用 CPU 解释器执行 `float x = 1.0; float y = x;`。接口变量只被记录下来。这段 shader 不产生顶点，解释器也没有机器码 JIT。

彩色三角形由 `p0_milestone_triangle` 在 CPU 上直接光栅化。三个 NDC 位置和颜色与 WebGPU P0 demo 相同，映射到本 crate 自己的 framebuffer，原点在左上。它不是 `drawArrays`，不调用浏览器 `WebGL2RenderingContext` 或 `navigator.gpu`，也不保证与 `glgpu-raster` 的像素一致。

光照旋转立方体、WebGL 左下原点和 clip 空间都还没有做。标识保持 `CpuWebGl2LearningTrack — NOT WEBGL CONFORMANT`。

说明见 [WebGL2 学习轨道](../../docs/webgl2-learning-track.zh-CN.md)。ESSL 的 accept/reject 参照是 Khronos glslang 16.6.0，当前测试不调用它。
