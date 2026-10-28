# GLGPU P0 中文 API 与行为契约

> 文档版本：P0 / `0.1.0`  
> 核对日期：2026-08-22  
> 适用源码：当前 workspace 中的 `glgpu-core`、`glgpu-trace`、`glgpu-raster`、`glgpu-shader`、`glgpu-webgpu`、`glgpu-canvas2d`  
> `glgpu-webgl` 不属于本文。它是独立的 webgl-mock，有自己的 clear、WebGL clip/窗口、shader 三角形 demo、ESSL 子集和解释器。`glgpu-webgl-web` 只是它的 demo 页壳，当前在主线程跑单线程 Wasm，并用 Canvas 2D `putImageData` 直接交到页面 canvas。目标是先写入 Worker 离屏缓冲区，再 present 到可见 canvas。两者都不是原生 WebGL façade，也不链接 `WebGLRenderingContext` 或 `navigator.gpu`。见 [webgl-mock](webgl-mock.zh-CN.md)。

本文是当前 P0 实现的逐项 API 参考和可观察行为契约。长期设计、后续阶段和方案取舍见[架构规划](architecture-plan.zh-CN.md)，机器可读的能力声明见 [P0 支持矩阵](support-matrix/webgpu-p0.toml)，构建模式见 [Wasm 构建 ADR](adr/0002-wasm-build-profiles.md)。当规划文档与当前源码行为不一致时，本文只描述已经落地的 P0；最终仍以锁定版本的源码和测试为准。

## 1. 必须先理解的 P0 边界

### 1.1 它不是标准 WebGPU 实现

`glgpu-webgpu` 暴露的是项目自有、Rust-native 的 `CpuGpuSubset`，不是浏览器的 `GPUAdapter`，也不声称通过 WebGPU CTS。当前实现：

- 不安装或替换 `navigator.gpu`；
- 不提供 WebIDL 对象、JavaScript Promise、错误作用域或标准异步 queue timeline；
- 不提供标准 `GPUBuffer`、texture、sampler、bind group、pipeline、render pass 或 compute pass；
- 不模拟真实 GPU 的执行时序；
- 只接受已经变换到 framebuffer 坐标的 CPU 三角形；
- 每个命令缓冲只支持一次清屏、若干有序三角形绘制，以及最多一个待追踪像素；
- `submit` 在创建它的 owner 线程上同步完成。

因此，P0 API 的 WebGPU 风格仅用于逐步建立可调试的 CPU 后端，不能把现有 WebGPU 程序原样指向它。

### 1.2 WGSL 只解析和校验，不执行

`glgpu-shader::parse_and_validate_wgsl` 和 `CpuDevice::create_shader_module` 会使用 Naga 解析、校验 WGSL，并提取 entry point 元数据。P0 **没有 WGSL lowering、字节码或解释器**。传给 `create_shader_module` 的 vertex、fragment 或 compute 函数不会参与 `draw_triangle`，也不能被 dispatch。

当前三角形的位置、深度和颜色全部直接来自 `glgpu_core::Triangle`。支持矩阵将 `WgslParseAndValidate` 标记为 `Supported`，将 `WgslExecution` 和 `Compute` 标记为 `Planned`。

### 1.3 Pixel history 只记录 raster-stage

P0 的 pixel history 是单个 framebuffer 像素的光栅阶段历史，不是完整的 WebGPU pipeline trace。它记录：

- 已准备成功、且光栅包围盒到达所选像素所在 sample 的 primitive；
- coverage 失败、严格小于深度测试失败或最终写入；
- barycentric、候选深度和写入前后颜色等光栅信息。

它不记录 shader invocation、vertex fetch、装配、裁剪、bind group、纹理采样、混合或呈现事件；退化三角形、完全在 framebuffer 外的三角形，以及其他在进入 sample 光栅化前就被剔除的 primitive 也没有独立事件。

### 1.4 坐标、像素和颜色约定

- framebuffer 原点位于左上角；`x` 向右，`y` 向下。
- 整数坐标表示像素边角，像素中心采样点是 `(x + 0.5, y + 0.5)`。
- color/depth plane 均采用 row-major 顺序，索引为 `y * width + x`。
- `Rgba8` 是非预乘（unpremultiplied）的 8-bit RGBA。
- P0 只有内部 RGBA8 color plane 和 `f32` depth plane；后者不是 WebGPU depth texture format。
- 没有 scissor、viewport、MSAA、blend、perspective-correct varying 或纹理采样。

## 2. Crate 和 feature 总览

| Crate | 职责 | 默认 feature | 可选 feature / 平台门控 |
| --- | --- | --- | --- |
| [`glgpu-core`](../crates/glgpu-core/src/lib.rs) | DOM-free 基础数据和 framebuffer | 无 | 无 |
| [`glgpu-trace`](../crates/glgpu-trace/src/lib.rs) | 单像素 raster history 记录类型 | 无 | 无 |
| [`glgpu-raster`](../crates/glgpu-raster/src/lib.rs) | 16×16 tile CPU 三角形光栅器 | 空 | `parallel` 启用 Rayon；`rayon` 是指向 `parallel` 的兼容别名 |
| [`glgpu-shader`](../crates/glgpu-shader/src/lib.rs) | WGSL 解析、校验和稳定诊断边界 | 无 | 无；内部使用锁定的 Naga `30.0.1` + `wgsl-in` |
| [`glgpu-webgpu`](../crates/glgpu-webgpu/src/lib.rs) | 非标准 P0 WebGPU-style Rust façade | 空 | `parallel` 向下启用 `glgpu-raster/parallel` |
| [`glgpu-canvas2d`](../crates/glgpu-canvas2d/src/lib.rs) | RGBA8 校验和 Canvas 2D 呈现边界 | 无 | `present_rgba8` 仅在 `wasm32` 编译；atomics 构建采用 MT staging 路径 |
| [`glgpu-web`](../crates/glgpu-web/src/lib.rs) | 仓库内调试页应用壳，不作为可复用图形 API | 空 | `parallel` 导出 Rayon pool 初始化；其余浏览器 API 仅在 `wasm32` 编译 |

整个 workspace 使用 Rust 2024 edition，最低 Rust 版本为 1.93。上述 crate 当前均为 `publish = false`，P0 的公开 Rust 类型是项目边界，不等于已经承诺长期 SemVer 稳定。

依赖方向为：

```text
glgpu-core ───────────────┐
    │                     │
    └─> glgpu-trace ──────┤
                          ├─> glgpu-raster ──┐
glgpu-shader ─────────────┴─────────────────> glgpu-webgpu

RGBA8 bytes ────────────────────────────────> glgpu-canvas2d ──> Canvas 2D

glgpu-core/raster/webgpu/canvas2d ──────────> glgpu-web ───────> DOM + 调试页
```

`glgpu-core`、`glgpu-trace` 和 `glgpu-raster` 禁止 crate 内使用 `unsafe`。核心光栅和 trace 不依赖 DOM，可在 native 测试和 Wasm 中复用。

## 3. `glgpu-core`

### 3.1 `Extent2d`

```rust
pub struct Extent2d {
    pub width: u32,
    pub height: u32,
}
```

二维像素尺寸。允许构造零宽或零高的 extent；是否允许零尺寸由上层 API 决定：低层 `Framebuffer` 和 `render` 可以处理零像素，`CpuAdapter::request_device` 则拒绝任一维为零。

| API | 契约 |
| --- | --- |
| `Extent2d::new(width, height)` | 常量构造器，不做尺寸校验。 |
| `pixel_count(self)` | 以 `u64` 检查 `width * height`，再转换为当前平台的 `usize`。无法表示时返回 `CoreError::SizeOverflow`。 |
| `contains(self, pixel)` | 半开区间检查：`pixel.x < width && pixel.y < height`。 |

该类型实现 `Copy`、值相等和 hash。

### 3.2 `PixelCoord`

```rust
pub struct PixelCoord {
    pub x: u32,
    pub y: u32,
}
```

`PixelCoord::new(x, y)` 只构造坐标，不绑定 extent，也不单独做边界检查。坐标原点为 framebuffer 左上角。该类型实现 `Copy`、值相等和 hash。

### 3.3 `Rgba8`

```rust
#[repr(C)]
pub struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}
```

这是非预乘 RGBA；`#[repr(C)]` 固定字段顺序，但对外部字节输出仍应使用显式转换 API。

| API | 契约 |
| --- | --- |
| `Rgba8::TRANSPARENT` | `(0, 0, 0, 0)`。 |
| `Rgba8::BLACK` | `(0, 0, 0, 255)`。 |
| `Rgba8::WHITE` | `(255, 255, 255, 255)`。 |
| `Rgba8::new(r, g, b, a)` | 常量构造器。 |
| `to_array(self)` | 返回 `[r, g, b, a]`。 |
| `From<[u8; 4]> for Rgba8` | 按 RGBA 顺序构造。 |
| `From<Rgba8> for [u8; 4]` | 等价于 `to_array`。 |

### 3.4 `ScreenVertex`

```rust
pub struct ScreenVertex {
    pub x: f32,
    pub y: f32,
    pub depth: f32,
    pub color: Rgba8,
}
```

`ScreenVertex::new(x, y, depth, color)` 不立即校验。光栅器要求 `x`、`y`、`depth` 都是有限值，并要求 `x/y` 能进入内部定点范围，否则在 `render` 时返回 `RasterError`。低层光栅 API不把 vertex depth 限制在 `0..=1`。

位置已经是 framebuffer pixel coordinate，不经过 vertex shader、homogeneous divide、viewport transform 或规范裁剪。

### 3.5 `Triangle`

```rust
pub struct Triangle {
    pub primitive_id: u64,
    pub vertices: [ScreenVertex; 3],
}
```

| API | 契约 |
| --- | --- |
| `Triangle::new(primitive_id, vertices)` | 从三顶点数组构造。 |
| `Triangle::from_vertices(primitive_id, v0, v1, v2)` | 便捷构造器，提交顶点顺序保持为 `[v0, v1, v2]`。 |

输入 slice 中的位置才是稳定的 primitive 顺序。`primitive_id` 只供诊断使用，可重复，不参与覆盖或排序。

### 3.6 `Framebuffer`

```rust
pub struct Framebuffer { /* planes are private */ }
```

一个拥有 color/depth plane 的 framebuffer。两个 plane 的长度始终与 extent 的像素数一致；公开 API 不允许单独改变某一 plane 的长度。

| API | 返回 / 行为 |
| --- | --- |
| `Framebuffer::new(extent, clear_color, clear_depth)` | 分配两个 plane 并分别填充清屏值。尺寸不可表示时返回 `SizeOverflow`；reserve 失败返回 `AllocationFailed`。这里不校验 `clear_depth` 是否有限或位于某范围。 |
| `Framebuffer::from_planes(extent, color, depth)` | 接管两个 `Vec`；任一长度不等于 `extent.pixel_count()` 时返回包含三种长度的 `PlaneLengthMismatch`。 |
| `extent(&self)` | 返回 extent 的副本。 |
| `color(&self)` | 借用完整 row-major color plane。 |
| `depth(&self)` | 借用完整 row-major depth plane。 |
| `color_at(pixel)` | 越界或索引计算失败时为 `None`，否则返回颜色副本。 |
| `depth_at(pixel)` | 越界或索引计算失败时为 `None`，否则返回深度副本。 |
| `clear(color, depth)` | 同步填充两个已有 plane，不重新分配；不校验 depth。 |
| `stable_hash64()` | 对 width、height、所有 RGBA 字节、所有 depth 的精确 `f32::to_bits()` 以小端字节执行稳定 FNV-1a 64-bit hash。 |

`stable_hash64` 用于确定性回归，不是密码学 hash，也不承诺抗碰撞。它区分不同的浮点 bit pattern，例如 `0.0`/`-0.0`；不会对浮点值做归一化。

### 3.7 `CoreError`

| 变体 | 触发条件 |
| --- | --- |
| `SizeOverflow` | framebuffer 像素数不能表示为本平台 `usize`。 |
| `AllocationFailed` | color 或 depth plane 无法 reserve。 |
| `PlaneLengthMismatch { expected, color, depth }` | `from_planes` 收到的 plane 长度与 extent 不一致。 |

`CoreError` 实现 `Display` 和 `std::error::Error`。

## 4. `glgpu-trace`

这些类型本身是普通记录容器；只有由 `glgpu-raster` 生成时，才同时获得下面描述的提交顺序和光栅语义。

### 4.1 `PixelEventOutcome`

| 变体 | 光栅器中的含义 |
| --- | --- |
| `OutsideCoverage` | primitive 的准备和包围盒阶段到达该 sample，但定点 edge/top-left coverage 判定失败。此时没有 barycentric 或候选深度。 |
| `DepthRejected` | sample 被覆盖，但候选深度不严格小于当前深度；颜色和深度保持不变。 |
| `Written` | sample 被覆盖且通过严格小于深度测试，新的颜色和深度已写入。 |

### 4.2 `PixelHistoryEvent`

```rust
pub struct PixelHistoryEvent {
    pub triangle_index: u32,
    pub primitive_id: u64,
    pub outcome: PixelEventOutcome,
    pub barycentric: Option<[f32; 3]>,
    pub candidate_depth: Option<f32>,
    pub previous_depth: f32,
    pub color_before: Rgba8,
    pub color_after: Rgba8,
}
```

- `triangle_index` 是三角形在提交 slice 中的位置，不是 `primitive_id`。
- `primitive_id` 原样来自调用方，可重复。
- coverage 成功时，`barycentric` 按调用方提交的三顶点顺序排列；即使内部为统一 winding 交换过顶点，也会在 trace 中恢复。
- `OutsideCoverage` 的 `barycentric` 和 `candidate_depth` 均为 `None`。
- `DepthRejected` 和 `Written` 的这两项均为 `Some`。
- `previous_depth` 和 `color_before` 是处理该 primitive 前的像素值。
- `color_after` 在拒绝时等于 `color_before`，在写入时为插值后的新颜色。

### 4.3 `PixelHistory`

```rust
pub struct PixelHistory {
    pub pixel: PixelCoord,
    pub events: Vec<PixelHistoryEvent>,
    pub final_color: Rgba8,
    pub final_depth: f32,
}
```

| API | 契约 |
| --- | --- |
| `PixelHistory::new(pixel, clear_color, clear_depth)` | 创建空事件列表，并把 final 值暂设为 clear 值；不检查 pixel 是否属于某 extent。 |
| `push(event)` | 在尾部追加事件，不自行验证顺序或字段一致性。 |
| `finish(final_color, final_depth)` | 覆盖最终值，不锁定记录；之后仍可继续调用 `push` 或 `finish`。 |

光栅器生成的 `events` 按提交 triangle 顺序排列。所选像素即使没有任何 triangle 事件，也会得到空 history 和 clear 后的 final 值。

## 5. `glgpu-raster`

`glgpu-raster` 重新导出以下类型，调用方可以直接从该 crate 引入：

```rust
pub use glgpu_core::{Extent2d, Framebuffer, PixelCoord, Rgba8, ScreenVertex, Triangle};
pub use glgpu_trace::{PixelEventOutcome, PixelHistory, PixelHistoryEvent};
```

### 5.1 Feature 和常量

- `default = []`：默认是顺序 tile executor。
- `parallel`：启用 Rayon，并允许 `RenderOptions::parallel = true`。
- `rayon`：兼容别名，内部转到 `parallel`；新代码应使用 `parallel`。
- `TILE_SIZE: u32 = 16`：tile 固定为 16×16 像素。

只编译 `parallel` feature 不会强制每次并行；实际执行器由每次调用的 `RenderOptions::parallel` 决定。反之，如果运行时请求并行但编译时没有 feature，`render` 明确返回 `ParallelFeatureDisabled`，不会静默降级。

### 5.2 `RenderOptions`

```rust
pub struct RenderOptions {
    pub clear_color: Rgba8,
    pub clear_depth: f32,
    pub selected_pixel: Option<PixelCoord>,
    pub parallel: bool,
}
```

默认值为黑色、depth `1.0`、不追踪像素、顺序执行。`clear_depth` 必须有限，但低层 raster API 不要求它位于 `0..=1`。`selected_pixel` 若存在必须位于 extent 内。

### 5.3 `Stats`

所有计数器为 `u64`，tile 汇总使用饱和加法，极端情况下停留在 `u64::MAX` 而不回绕。

| 字段 | 含义 |
| --- | --- |
| `triangles_submitted` | 输入 slice 长度。 |
| `triangles_rasterized` | 成功准备并进入 binning 的三角形数；不含退化、完全在 framebuffer 外或零 extent 下被跳过的三角形。 |
| `tiles_total` | `ceil(width/16) * ceil(height/16)`。 |
| `nonempty_tiles` | 至少有一个 triangle bin reference 的 tile 数。 |
| `tile_triangle_refs` | 所有 tile bin 中 triangle 引用数量总和；跨 tile 的三角形会出现多次。 |
| `samples_tested` | triangle 的像素包围盒与 tile 交集内执行 coverage test 的 sample 次数。 |
| `samples_covered` | 通过 edge/top-left coverage test 的次数。 |
| `fragments_written` | 通过深度测试并写入的次数。 |
| `fragments_depth_rejected` | coverage 成功但深度测试失败的次数。 |

这些是 CPU raster 工作量计数，不是硬件 GPU performance counter。

### 5.4 `RenderOutput`

```rust
pub struct RenderOutput {
    pub framebuffer: Framebuffer,
    pub stats: Stats,
    pub pixel_history: Option<PixelHistory>,
}
```

`stable_hash64(&self)` 直接返回 `framebuffer.stable_hash64()`；stats 和 trace 不进入 hash。若 `selected_pixel` 为 `None`，`pixel_history` 为 `None`；合法选择了像素时，即便没有三角形事件，也会生成 `Some`。

### 5.5 `render`

```rust
pub fn render(
    extent: Extent2d,
    triangles: &[Triangle],
    options: RenderOptions,
) -> Result<RenderOutput, RasterError>
```

可观察执行顺序：

1. 校验 clear depth 和 selected pixel。
2. 建立 16×16 tile grid。
3. 按输入顺序校验并准备 triangle，坐标量化为 8-bit subpixel 定点数。
4. 忽略退化或完全离开 framebuffer 的 triangle；接受顺/逆时针 winding，内部统一方向。
5. 按提交顺序把 triangle 放入相交 tile 的 bin。
6. 每个 tile 独立处理，tile 内严格保持 triangle 顺序。
7. 对 `(x + 0.5, y + 0.5)` 使用定点 edge function 和 top-left ownership rule。
8. 对覆盖 sample 计算 barycentric；depth 以 `f64` 中间值线性插值后转为 `f32`，颜色四通道分别线性插值、四舍五入并 clamp 到 `0..=255`。
9. 使用 `candidate_depth < previous_depth` 的严格小于测试。深度相等时，先提交并已写入的 triangle 保留。
10. 以稳定 tile 顺序合并 framebuffer、stats 和可选 history。

P0 没有 perspective correction、blend 或 alpha compositing。插值出的 RGBA 会直接覆盖目标颜色；alpha 只是和 RGB 一样被插值的一个通道。

#### Coverage 和定点契约

- edge coverage 使用每像素 8-bit subpixel（1/256 pixel）定点坐标；输入 `f32` 位置会量化到该网格。
- 共享边使用 top-left rule，所以相邻三角形在相同量化边界上不会同时拥有边，也不会因 ownership rule 产生裂缝。
- 超出内部安全定点范围的位置会报错，不会溢出后继续绘制。
- `x`、`y` 或 depth 非有限时整个 `render` 失败，而不是只丢弃该 triangle。

### 5.6 顺序与并行确定性

并行粒度只在 tile 之间。每个 worker 生成私有 `TileResult`，同一 tile 内 primitive 顺序不变；之后再按 tile index 合并。因此在同一 target/toolchain 和相同输入下，顺序与 Rayon 路径的 framebuffer、stats 和 pixel history 设计为逐值一致，现有测试也逐项比较三者和稳定 hash。

这不应被扩大解释为“模拟任意 GPU 的浮点结果”：插值仍使用 host/Wasm 浮点，`stable_hash64` 记录精确 depth bit pattern。跨不同编译器、target 或未来算法版本的 hash 兼容性需要单独版本化，P0 没有长期 capture/hash 格式承诺。

在浏览器 MT 包中，Rayon worker pool 的创建由上层 `wasm-bindgen-rayon` 启动流程负责；`glgpu-raster` 自身不会初始化浏览器 worker，也不会检查 COOP/COEP 响应头。

### 5.7 `RasterError`

| 变体 | 触发条件 |
| --- | --- |
| `Core(CoreError)` | framebuffer 尺寸、plane 或核心分配错误。它是 `Error::source()`。 |
| `SizeOverflow` | tile grid / bin 索引尺寸溢出。 |
| `AllocationFailed` | triangle、bin、tile、color 或 depth 等 raster 分配失败。 |
| `InvalidClearDepth` | `RenderOptions::clear_depth` 非有限。 |
| `InvalidVertex { triangle_index, vertex_index }` | 某顶点的 `x`、`y` 或 `depth` 非有限；两个索引均按提交顺序。 |
| `CoordinateOutOfRange { triangle_index, vertex_index }` | 某顶点 `x/y` 无法安全转换到内部定点范围。 |
| `TooManyTriangles { count }` | 输入位置无法表示为 history 使用的 `u32 triangle_index`。 |
| `SelectedPixelOutOfBounds { pixel, extent }` | 请求追踪的像素不属于 extent。 |
| `ParallelFeatureDisabled` | options 请求并行，但 crate 未编译 `parallel` feature。 |

`RasterError` 实现 `Display`、`Error`，并实现 `From<CoreError>`。

## 6. `glgpu-shader`

Naga module 和 validation IR 都保存在私有字段中，没有泄漏到公开类型。这样后续可替换 lowering 或增加项目自有字节码，而无需让 capture/API 依赖 Naga IR 布局。

### 6.1 `ShaderStage`

```rust
pub enum ShaderStage { Vertex, Fragment, Compute }
```

三种 WGSL entry point stage。它实现 `Copy`、全序、值相等和 hash。

### 6.2 `SourceSpan`

```rust
pub struct SourceSpan {
    pub offset: u32,
    pub length: u32,
    pub line: u32,
    pub column: u32,
}
```

`offset`/`length` 是 UTF-8 byte offset；`line`/`column` 从 1 开始。某些诊断可能没有可定位 span，此时上层 `ShaderDiagnostic::span` 是 `None`。

### 6.3 `ShaderDiagnosticKind` 和 `ShaderDiagnostic`

`ShaderDiagnosticKind` 有三个变体：

- `Parse`：WGSL 语法/词法解析失败；
- `Validation`：模块成功解析，但 Naga 核心验证失败；
- `Unsupported`：出现项目稳定边界无法映射的 shader stage。当前 WGSL 通常不会产生 Naga 的其他 stage，但代码显式拒绝以防依赖升级改变行为。

```rust
pub struct ShaderDiagnostic {
    pub kind: ShaderDiagnosticKind,
    pub message: String,
    pub source_message: String,
    pub span: Option<SourceSpan>,
}
```

- `message` 是无终端格式的简短信息。
- `source_message` 包含 source label、相关源代码行和 caret/label；`Display` 输出此字段。
- 该类型实现 `std::error::Error`，但没有下层 `source()` 链。

### 6.4 `EntryPointInfo` 和 `ShaderModuleInfo`

```rust
pub struct EntryPointInfo {
    pub name: String,
    pub stage: ShaderStage,
    pub workgroup_size: [u32; 3],
}

pub struct ShaderModuleInfo {
    pub entry_points: Vec<EntryPointInfo>,
}
```

entry points 按 Naga module 中的顺序收集。vertex 和 fragment 的 `workgroup_size` 为 `[1, 1, 1]`；compute 保留 WGSL 声明值。

`ShaderModuleInfo::entry_point(stage, name)` 返回第一个 stage 和名称都匹配的借用；没有匹配项时为 `None`。这些类型没有 Serde 实现；源码所说的“serializable-in-spirit”只表示字段刻意保持为项目自有、普通数据。

### 6.5 `ValidatedShaderModule`

该类型拥有：原始 WGSL `String`、稳定 `ShaderModuleInfo`、私有 Naga module 和私有 validation info。

| API | 契约 |
| --- | --- |
| `source(&self)` | 借用原始、未重写的 WGSL 字符串。 |
| `info(&self)` | 借用提取后的 entry point 元数据。 |
| `into_info(self)` | 消耗 module，只取出元数据。原始源和私有 compiler IR 随之释放。 |

其自定义 `Debug` 只展示 source 长度和 info，并标为 non-exhaustive，不打印完整源或 Naga IR。

### 6.6 解析函数

```rust
pub fn parse_and_validate_wgsl(
    source: impl Into<String>,
) -> Result<ValidatedShaderModule, ShaderDiagnostic>

pub fn parse_and_validate_wgsl_with_label(
    label: &str,
    source: impl Into<String>,
) -> Result<ValidatedShaderModule, ShaderDiagnostic>
```

两者都会取得 source 所有权，执行 Naga WGSL parse，然后以 `ValidationFlags::all()` 和 `Capabilities::empty()` 校验。空 capability 集是 P0 profile 的刻意限制，例如 `f16` 等可选能力不会仅因 Naga 能解析就被声明为可用。

第一种函数等价于 label 为 `"wgsl"` 的第二种。label 只用于渲染诊断路径，不改变语言语义。

成功仅证明模块在当前 P0 校验 profile 下语法/语义有效；**不证明 GLGPU 可以执行它**，也不创建 render/compute pipeline。

## 7. `glgpu-webgpu`

该 crate 没有把 `Extent2d`、`Rgba8`、`Triangle` 等重新导出；调用方应从 `glgpu-core`（或 `glgpu-raster` 的 re-export）引入这些数据类型。

### 7.1 能力和支持矩阵

#### `Capability`

完整枚举及当前 P0 状态如下。`Planned` 和 `Unsupported` 当前都意味着 `supports == false`、`require` 返回错误；区别只是路线图意图。

| Capability | 默认构建 | `parallel` 构建 | 说明 |
| --- | --- | --- | --- |
| `Rgba8DepthClear` | Supported | Supported | 每个 command buffer 开头一次 RGBA8/depth clear。 |
| `ScreenSpaceTriangleDraw` | Supported | Supported | 按顺序绘制预变换 CPU triangle。 |
| `SelectedPixelHistory` | Supported | Supported | 每个 command buffer 最多一个 raster-stage 像素；不含 pre-raster cull。 |
| `SynchronousImmediateSubmit` | Supported | Supported | owner 线程同步提交。 |
| `SynchronousColorReadback` | Supported | Supported | 借用 `Rgba8` 或拷贝 RGBA bytes。 |
| `DeterministicTileParallelism` | Unsupported | Supported | 状态由编译期 `cfg(feature = "parallel")` 决定。 |
| `WgslParseAndValidate` | Supported | Supported | Naga parse/validate，项目自有元数据边界。 |
| `WgslExecution` | Planned | Planned | P0 不执行 WGSL。 |
| `Compute` | Planned | Planned | P0 没有 dispatch/workgroup executor。 |
| `AsynchronousQueue` | Planned | Planned | P0 queue 同步。 |
| `JavaScriptFacade` | Planned | Planned | 没有 WebIDL/Promise/标准 JS objects。 |
| `StandardWebGpuAdapter` | Unsupported | Unsupported | 不满足 WebGPU core adapter/device 能力保证。 |

#### 支持类型

```rust
pub enum SupportStatus { Supported, Planned, Unsupported }

pub struct SupportEntry {
    pub capability: Capability,
    pub status: SupportStatus,
    pub detail: &'static str,
}

pub struct SupportMatrix;
```

`SupportMatrix` 是零大小、不可变 view，并实现 `Default`：

| API | 契约 |
| --- | --- |
| `entries(self)` | 返回静态完整表。 |
| `get(self, capability)` | 返回该能力的静态 entry。所有公开枚举值都有 entry；若源码维护失配会 panic，调用方输入本身不会造成缺项。 |
| `supports(self, capability)` | 仅当状态严格为 `Supported` 时为 `true`。 |
| `require(self, capability)` | Supported 返回 `Ok(())`；Planned/Unsupported 返回结构化 `Unsupported`。 |

```rust
pub struct Unsupported {
    pub capability: Capability,
    pub status: SupportStatus,
    pub reason: &'static str,
}
```

`Unsupported` 实现 `Display` 和 `Error`，保留能力、状态和静态原因，便于 UI 不解析错误字符串就显示子集缺口。

### 7.2 Provider、adapter 和 device

#### `CpuGpuSubset`

零成本的 P0 provider root，实现 `Copy` 和 `Default`。

| API | 契约 |
| --- | --- |
| `CpuGpuSubset::new()` | 创建指向当前编译能力表的 provider。 |
| `support_matrix(&self)` | 返回 `SupportMatrix` 副本。 |
| `request_adapter(&self)` | **同步、不会失败**地返回 `CpuAdapter`；它不是标准 WebGPU 的异步 adapter 请求。 |

#### `CpuAdapter`

| API | 契约 |
| --- | --- |
| `support_matrix(&self)` | 返回同一构建的能力 view。 |
| `request_device(descriptor)` | 同步创建 device；校验 extent 非零、像素数可表示，并检查请求的 parallel 能力。 |

`request_device` 不分配 framebuffer；真正的 raster 和输出分配发生在 `submit`。

#### `DeviceDescriptor`

```rust
pub struct DeviceDescriptor {
    pub extent: Extent2d,
    pub parallel: bool,
}
```

`DeviceDescriptor::new(extent)` 默认 `parallel = false`；`with_parallel(bool)` 消耗并返回修改后的 descriptor，适合 builder 链。parallel 为 `true` 但 crate 未启用同名 feature 时，device 请求返回 `CpuGpuError::Unsupported`。

#### `CpuDevice`

| API | 契约 |
| --- | --- |
| `descriptor(&self)` | 返回创建 device 时 descriptor 的副本。 |
| `support_matrix(&self)` | 返回构建能力 view。 |
| `create_command_encoder(&self)` | 创建绑定 device extent 的空 encoder。不会借用 device，可在之后通过 `queue_mut` 提交完成的 buffer。 |
| `create_shader_module(source)` | 调用 `parse_and_validate_wgsl`；返回的 module 不连接到任何 draw，也不会执行。错误类型为独立的 `ShaderDiagnostic`。 |
| `queue(&self)` | 只读借用 queue，用于 serial、readback 和 history。 |
| `queue_mut(&mut self)` | 可变借用 queue，用于 `submit`。 |

device 内含 owner queue，所以和 queue 一样不是跨线程传输对象。

### 7.3 Command encoding

#### `CommandEncoder`

encoder 的 extent、clear、triangle 列表和 selected pixel 都是私有状态。

| API | 成功条件与状态变化 |
| --- | --- |
| `clear(color, depth)` | 只能调用一次，且必须发生在首个 draw 前；depth 必须有限且在闭区间 `0..=1`。成功后记录唯一 clear。 |
| `draw_triangle(triangle)` | 必须已经 clear；按调用顺序追加 triangle。只做列表分配，不在 encoding 时校验顶点。 |
| `select_pixel(pixel)` | pixel 必须在 device extent 内；可在 clear 前后调用，也可调用多次，最后一次覆盖之前选择。P0 没有取消选择的方法。 |
| `finish(self)` | 消耗 encoder；必须已有 clear。成功返回不可变 `CommandBuffer`。clear-only buffer 合法。 |

`clear` 的 `0..=1` 约束比低层 `glgpu-raster::render` 更严格，是 façade 的 P0 命令契约。

#### `CommandBuffer`

命令缓冲按值交给 `submit`，不能提交后复用。

| API | 契约 |
| --- | --- |
| `triangle_count(&self)` | 返回记录的 triangle 数。 |
| `selected_pixel(&self)` | 返回最后选择的像素或 `None`。 |

#### `CommandEncodingError`

| 变体 | 触发条件 |
| --- | --- |
| `MissingClear` | 没有 clear 就 `finish`。 |
| `DuplicateClear` | 已成功记录 clear 后再次 clear。 |
| `ClearAfterDraw` | 防御性表示 draw 后才 clear；按当前 API，首个 draw 本身要求已有 clear，因此正常调用通常先遇到 `DrawBeforeClear` 或后续 `DuplicateClear`。 |
| `DrawBeforeClear` | clear 前 draw。 |
| `InvalidClearDepth` | clear depth 非有限或不在 `0..=1`。 |
| `SelectedPixelOutOfBounds { pixel, extent }` | 选择越界像素。 |
| `AllocationFailed` | triangle command list reserve 失败。 |

错误不会自动 poison encoder；是否继续使用取决于错误种类和当前已成功记录的状态。例如越界 `select_pixel` 不改变原选择。

### 7.4 `ImmediateMainOwnerQueue`

queue 通过 `PhantomData<Rc<()>>` 被刻意设为 `!Send + !Sync`，`CpuDevice` 因包含它也具有相同约束。这是 owner-thread API 边界；Rayon 只在一次 `submit` 内部并行执行独立 tile，并没有把 queue 移交给 worker。

#### `submit`

```rust
pub fn submit(
    &mut self,
    command_buffer: CommandBuffer,
) -> Result<SubmissionReport, CpuGpuError>
```

提交会在返回前完成整个 raster：

1. 读取下一 serial，serial 从 `1` 开始；
2. 检查 serial 能否递增；
3. 把 clear、triangle、selected pixel 和 device 的 parallel 选项传给低层 `render`；
4. 成功时以新输出替换上一次 completed submission，递增 serial，并返回 report；
5. 失败时不替换上一次成功输出，也不推进 serial。

它没有 Promise、fence、异步错误、多个 in-flight submission 或标准 queue timeline。

#### `SubmissionReport`

```rust
pub struct SubmissionReport {
    pub serial: u64,
    pub stable_hash64: u64,
    pub triangle_count: usize,
    pub selected_pixel_recorded: bool,
}
```

- `stable_hash64` 只覆盖 framebuffer dimensions/color/depth，不覆盖命令、stats 或 trace。
- `selected_pixel_recorded` 表示输出中是否存在 history；合法 device extent 下，命令缓冲选择过像素即会记录，即使事件列表为空。

#### Readback API

| API | 返回 / 生命周期 |
| --- | --- |
| `last_completed_serial()` | 尚无成功提交时为 `None`，否则为最新成功 serial。 |
| `readback_framebuffer()` | 借用最新 framebuffer；无成功提交时返回 `NoCompletedSubmission`。下一次成功 submit 会替换其所有者，因此 Rust 借用规则禁止同时持有借用并可变提交。 |
| `readback_color()` | 借用最新 framebuffer 的 row-major `[Rgba8]`。 |
| `readback_rgba8()` | 分配并返回紧密排列的 owned `Vec<u8>`，每像素 `[r,g,b,a]`；无提交或分配失败时返回错误。 |
| `selected_pixel_history()` | 无成功提交或最新提交未选择像素时为 `None`；否则借用最新 history。该方法不返回 `Result`。 |

Wasm MT 构建中的 `readback_rgba8` 仍位于 shared linear memory，不能直接消除 Canvas staging copy；该边界由 `glgpu-canvas2d` 处理。

### 7.5 `CpuGpuError`

| 变体 | 触发条件 / source chain |
| --- | --- |
| `Unsupported(Unsupported)` | 请求未编译或未实现能力，例如 default build 请求 parallel。下层是 `Error::source()`。 |
| `InvalidDescriptor(&'static str)` | device extent 有零维，或像素数对本平台过大。 |
| `NoCompletedSubmission` | readback 前还没有成功提交。 |
| `AllocationFailed` | RGBA byte readback 的长度计算或 reserve 失败。 |
| `SerialExhausted` | submission serial 无法从当前值递增。检查发生在 raster 前。 |
| `Raster(RasterError)` | submit 时低层 raster 失败；下层是 `Error::source()`。 |

`From<Unsupported>` 和 `From<RasterError>` 已实现。shader module 创建失败返回 `ShaderDiagnostic`，不是 `CpuGpuError`。

## 8. `glgpu-canvas2d`

### 8.1 `PresentError`

| 变体 | 触发条件 |
| --- | --- |
| `DimensionsOverflow { width, height }` | `width * height * 4` 无法表示为当前 target 的 `usize`。 |
| `InvalidPixelLength { expected, actual }` | byte slice 长度不严格等于一帧紧密 RGBA8。 |
| `CanvasOperation { operation, message }` | Wasm 中创建 `ImageData` 或调用 `putImageData` 抛出 JS 错误；operation 保存失败步骤。 |

该错误实现 `Clone`、值相等、`Display` 和 `Error`。

### 8.2 平台无关校验 API

```rust
pub fn rgba8_byte_len(width: u32, height: u32) -> Result<usize, PresentError>
```

返回一帧紧密 RGBA8 所需的精确字节数，所有乘法均检查溢出。任一维为零时结果为 `0`。

```rust
pub fn validate_rgba8_pixels(
    width: u32,
    height: u32,
    pixels: &[u8],
) -> Result<(), PresentError>
```

只检查长度严格相等，不检查颜色空间、alpha 内容或 canvas 尺寸。零尺寸配空 slice 在该函数中合法。两者均可在 native 测试使用。

### 8.3 Wasm-only `present_rgba8`

```rust
#[cfg(target_arch = "wasm32")]
pub fn present_rgba8(
    context: &web_sys::CanvasRenderingContext2d,
    width: u32,
    height: u32,
    pixels: &[u8],
) -> Result<(), PresentError>
```

行为顺序：

1. 先调用 `validate_rgba8_pixels`；长度错误时不接触浏览器 API。
2. 创建相同尺寸的 `ImageData`。
3. 调用 `context.put_image_data(&image_data, 0.0, 0.0)`，从 canvas 左上角原样呈现。

函数不设置 canvas 的 backing width/height，不缩放、不执行 CSS layout，也不清除输入帧之外的 canvas 区域。调用方应先保证 canvas backing dimensions 符合预期。

#### ST 路径

非 atomics Wasm 构建通过 `wasm_bindgen::Clamped(pixels)` 构造 `ImageData`。

#### MT 路径

启用 Wasm `atomics` 后，Rust slice 位于 `SharedArrayBuffer` 支持的 shared linear memory。浏览器 `ImageData` 构造器不能接收共享 typed array，因此函数先用 `Uint8ClampedArray::from(pixels)` 生成普通 JS `ArrayBuffer` 上的 presentation copy，再创建 `ImageData`。这是刻意且必要的每帧 staging copy，不应在性能测量中当成零拷贝。

JS 异常若可转成字符串就保存该字符串，否则保存其 debug 表示。

## 9. 端到端最小用法

### 9.1 直接调用低层 raster

```rust
use glgpu_raster::{
    render, Extent2d, PixelCoord, RenderOptions, Rgba8, ScreenVertex, Triangle,
};

let triangle = Triangle::from_vertices(
    7,
    ScreenVertex::new(1.0, 1.0, 0.5, Rgba8::new(255, 0, 0, 255)),
    ScreenVertex::new(7.0, 1.0, 0.5, Rgba8::new(0, 255, 0, 255)),
    ScreenVertex::new(1.0, 7.0, 0.5, Rgba8::new(0, 0, 255, 255)),
);

let output = render(
    Extent2d::new(8, 8),
    &[triangle],
    RenderOptions {
        selected_pixel: Some(PixelCoord::new(2, 2)),
        ..RenderOptions::default()
    },
)?;

let hash = output.stable_hash64();
let history = output.pixel_history.as_ref();
```

### 9.2 使用非标准 WebGPU-style façade

```rust
use glgpu_core::{Extent2d, PixelCoord, Rgba8, ScreenVertex, Triangle};
use glgpu_webgpu::{CpuGpuSubset, DeviceDescriptor};

let provider = CpuGpuSubset::new();
let adapter = provider.request_adapter();
let mut device = adapter.request_device(DeviceDescriptor::new(Extent2d::new(8, 8)))?;

let mut encoder = device.create_command_encoder();
encoder.clear(Rgba8::BLACK, 1.0)?;
encoder.select_pixel(PixelCoord::new(2, 2))?;
encoder.draw_triangle(Triangle::from_vertices(
    7,
    ScreenVertex::new(1.0, 1.0, 0.5, Rgba8::new(255, 0, 0, 255)),
    ScreenVertex::new(7.0, 1.0, 0.5, Rgba8::new(0, 255, 0, 255)),
    ScreenVertex::new(1.0, 7.0, 0.5, Rgba8::new(0, 0, 255, 255)),
))?;

let command_buffer = encoder.finish()?;
let report = device.queue_mut().submit(command_buffer)?;
let rgba_bytes = device.queue().readback_rgba8()?;
```

注意：上例没有创建或绑定 WGSL shader，因为 P0 draw 不执行 shader。

### 9.3 并行构建和运行时选择

依赖 façade 时启用：

```toml
[dependencies]
glgpu-webgpu = { path = "crates/glgpu-webgpu", features = ["parallel"] }
```

然后显式请求并行 device：

```rust
let descriptor = DeviceDescriptor::new(Extent2d::new(720, 480)).with_parallel(true);
let device = CpuGpuSubset::new()
    .request_adapter()
    .request_device(descriptor)?;
```

feature 和 descriptor 两个条件缺一不可。浏览器中还必须先成功初始化 `wasm-bindgen-rayon` worker pool；服务端需要满足 SharedArrayBuffer 所需的 cross-origin isolation。构建和启动的仓库级命令见[根 README](../README.md)和 [Wasm 构建 ADR](adr/0002-wasm-build-profiles.md)。

### 9.4 浏览器应用壳导出

`glgpu-web` 是当前仓库自用的 demo/debugger 入口，不属于承诺可复用的 `CpuGpuSubset` API：

- Wasm 构建导出 `boot(parallel: bool)`。它要求文档中存在 `#app`，随后由 Rust 创建样式、DOM、Canvas、事件和演示设备；`parallel = true` 还要求 crate 启用 `parallel` feature。
- Wasm + `parallel` 构建重新导出 `wasm_bindgen_rayon::init_thread_pool`，生成的 JavaScript 名为 `initThreadPool`。loader 必须先完成 `await initThreadPool(n)`，再调用 `boot(true)`；同一 Wasm 实例的 pool 只初始化一次。
- native 构建只暴露 `native_build_marker()`，用于证明应用壳在非 Wasm target 上仍可进入 workspace 检查；它不会创建页面或执行渲染。

ST/MT 包选择、预检、fallback 与 worker 数属于 [`web/index.html`](../web/index.html) 的宿主合同，而不是 `boot` 内部行为。应用壳变化应同步更新[开发指南](development-guide.zh-CN.md)和[测试与验收](testing-and-acceptance.zh-CN.md)。

## 10. 错误边界和失败原子性

```text
WGSL source
  └─> ShaderDiagnostic                 （parse/validation 独立错误通道）

DeviceDescriptor
  └─> CpuGpuError::InvalidDescriptor
  └─> CpuGpuError::Unsupported

CommandEncoder
  └─> CommandEncodingError             （record/finish 阶段）

ImmediateMainOwnerQueue::submit
  └─> CpuGpuError::Raster(RasterError)
        └─> RasterError::Core(CoreError)

RGBA byte frame
  └─> PresentError                     （长度或 Canvas 2D 阶段）
```

关键失败语义：

- command encoding 错误发生在 submit 前，不产生 framebuffer；
- raster submit 失败不会覆盖上一次 completed submission，也不会消耗 serial；
- readback allocation 失败不会破坏 queue 中已经完成的 framebuffer；
- Canvas presentation 失败不会改变 CPU framebuffer；
- WGSL 校验成功或失败都与当前 triangle command list 相互独立。

## 11. 确定性、线程与调试保证汇总

| 主题 | P0 保证 | P0 不保证 |
| --- | --- | --- |
| Primitive 顺序 | 使用输入 slice / `draw_triangle` 调用顺序；每 tile 不重排。 | 不模拟硬件 GPU 并行完成顺序。 |
| Coverage | 8-bit subpixel 定点 edge function + top-left rule。 | 不等同于所有 GPU 的具体硬件 raster 精度。 |
| Depth | `f64` 插值中间值转 `f32`；严格 `<`，相等保留先写入值。 | 不提供可配置 compare function、depth format 或 depth bias。 |
| Color | straight RGBA8 各通道线性插值、round/clamp、直接覆盖。 | 无 blend、sRGB conversion、dither 或 format conversion。 |
| ST/MT | tile 私有输出和稳定合并；同 target/build 下设计为 framebuffer/stats/history 一致。 | 不承诺跨未来版本、不同 target/toolchain 的 hash 永久相同。 |
| Queue | owner 线程同步完成；queue/device `!Send + !Sync`。 | 无 Promise、worker-owned queue、in-flight timeline。 |
| Trace | 所选一个像素的有序 raster coverage/depth/color 事件。 | 无完整 pipeline、shader 或 pre-raster cull trace。 |
| Hash | dimensions、color bytes、精确 depth bits 的稳定 FNV-1a。 | 非密码学、非 capture ID、stats/trace 不参与。 |
| Canvas | 精确长度 RGBA8，以 `putImageData(0, 0)` 呈现；MT 有普通 ArrayBuffer staging copy。 | 不调整 canvas 尺寸、不缩放、不承诺零拷贝。 |

## 12. 当前明确未提供的 API

以下名称或语义不应由调用方假定存在：

- 标准 `navigator.gpu`、`GPUAdapter`、`GPUDevice`、WebIDL validation 和 Promise；
- buffer 创建/映射、bind group、pipeline layout、shader pipeline linkage；
- texture、sampler、mipmap、LOD、derivative、storage resource；
- render pass attachment/load/store、viewport、scissor、blend、stencil、MSAA、query；
- WGSL 指令执行、compute dispatch、workgroup barrier 或原子操作；
- capture/replay 文件格式、跨版本 trace schema；
- 完整 pixel history 或 shader 单步；
- WebGL 1/2 façade 和完整 GLSL ES（`glgpu-webgl` 的 CPU 子集不属于本 P0 契约）；
- 与硬件 WebGPU 的性能或逐 bit 结果等价。

这些能力的计划和 gate 以[架构规划](architecture-plan.zh-CN.md)为准；在代码中查询当前运行构建的能力时，应优先使用 `CpuGpuSubset::support_matrix()`，不要根据 feature 名称或文档文字猜测。
