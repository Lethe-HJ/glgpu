# GLGPU

GLGPU 在 CPU 上做模拟 WebGPU（webgpu-mock）和模拟 WebGL（webgl-mock），不是浏览器里的原生 WebGPU / 原生 WebGL。主线是 webgpu-mock：使用 Rust/WebAssembly 的确定性参考渲染器与图形调试器。渲染结果写入 RGBA8 framebuffer，最终只通过 Canvas 2D `ImageData` / `putImageData` 呈现。Canvas 2D 只是呈现器。目标线程模型是主线程 Web API façade 用高效消息调用 Worker + Wasm 后端；WebGL demo 页当前仍是主线程上的单线程 Wasm。WebGL 产品实现仍等到公共渲染内核、shader 执行和调试模型稳定之后。页面上的三角形是 demo。

> 当前状态：**P0 垂直切片已可运行，但项目不符合 WebGPU 标准，也不是 `navigator.gpu` 的替代实现。**

## 当前已经实现

- 16×16 tile 的 CPU 三角形光栅化；
- 8-bit subpixel 定点边函数与 top-left coverage 规则；
- RGBA8 颜色插值、内部 `f32` depth plane 和 strict-less depth test；
- 顺序执行器与 Rayon tile 并行执行器；
- 单个选定像素的 raster-stage coverage/depth/color history；
- Rust-native 非标准 `CpuGpuSubset`、同步 `ImmediateMainOwnerQueue` 和结构化 `Unsupported`；
- 可编辑的 WGSL 调试区、实时语法高亮、本机保存、Naga 核心语法/语义校验及源码位置诊断；
- Rust/Wasm 创建页面、样式、事件、Canvas 和调试 UI；
- stable 单线程 Wasm 包；
- `wasm-bindgen-rayon` + SharedArrayBuffer 多线程 Wasm 包；
- 带 COOP/COEP、正确 Wasm MIME 和路径越界防护的本地静态服务器；
- ST/MT framebuffer 与稳定 hash 一致性测试。

WGSL 当前只做核心语法/语义校验，demo 三角形仍由已经变换到 framebuffer 坐标的 Rust 顶点驱动。shader 执行、buffer、bind group、texture、sampler、异步 queue、compute 和标准 JavaScript WebGPU façade 尚未实现。

## 快速开始

项目锁定 Rust `1.93.0`，`rust-toolchain.toml` 会选择正确的 stable 工具链。首先安装锁定版本的构建工具：

```bash
cargo install wasm-pack --version 0.14.0 --locked
```

然后执行：

```bash
# 原生测试与静态检查
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings

# stable 单线程浏览器包
cargo xtask build-web --release

# 固定 nightly 的共享内存/Rayon 浏览器包
cargo xtask build-web-mt

# webgl-mock demo 页的独立 Wasm 包（不进入 webgpu-mock 的 ST/MT 包）
cargo xtask build-web --release --crate crates/glgpu-webgl-web --out-dir web/pkg-webgl --out-name glgpu_webgl_web

# 带跨源隔离响应头的本地服务器
cargo xtask serve
```

打开：

- webgpu-mock ST：`http://127.0.0.1:8080/`
- webgpu-mock MT：`http://127.0.0.1:8080/?threads=1`
- webgl-mock demo 页：`http://127.0.0.1:8080/webgl.html`（需先构建 `web/pkg-webgl`；当前是主线程单线程 Wasm）

## GitHub Pages

推送到 `main` 的 [pages workflow](.github/workflows/pages.yml) 会构建 webgpu-mock ST、webgpu-mock MT 和 webgl-mock demo 页三个包，并把 `web/` 发布到 GitHub Pages。本仓库的项目站点是：

```text
https://Lethe-HJ.github.io/glgpu/
```

启用一次：仓库 **Settings → Pages → Build and deployment → Source** 选择 **GitHub Actions**。首次如果部署作业提示 Pages 尚未启用，保存该设置后重跑 `pages` workflow。

- `/` 启动 webgpu-mock ST，不注册 service worker。顶栏的 WebGL 链到 `webgl.html`，点击后整页重载。
- `/webgl.html` 启动独立的 webgl-mock demo 页 Wasm，当前跑在主线程。它不加载 webgpu-mock 包，也不注册 `coop-coep.js`。
- `/?threads=1` 请求 WebGPU MT。GitHub Pages 不能配置自定义响应头，因此这个入口用同源 [`web/coop-coep.js`](web/coop-coep.js) 补上与 `cargo xtask serve` 相同的 `COOP: same-origin` 和 `COEP: require-corp`，然后自动刷新一次。
- 浏览器不接受 service worker 合成的隔离头时，现有 loader 回退到 ST。硬刷新会绕过 service worker，可能再刷新一次。
- 页面使用相对 URL。`index.html` 与 `webgl.html` 在本地 `/` 和项目站点 `/glgpu/` 下都能解析。WebGL 页是 `https://Lethe-HJ.github.io/glgpu/webgl.html`。不要加 `<base>`。
- `cargo xtask serve` 仍由服务器发送真实隔离头。页面已经 `crossOriginIsolated` 时不会注册 service worker，本地 ST/MT 流程不变。

`curl -I https://Lethe-HJ.github.io/glgpu/` 看不到 COOP/COEP。这是 Pages 的平台限制，不是工作流漏了响应头。隔离发生在 `?threads=1` 的第二次文档导航里。本地验收继续用 `cargo xtask serve`。

MT loader 会检查 `crossOriginIsolated`、`SharedArrayBuffer`、共享 `WebAssembly.Memory` 和逻辑 CPU 数。条件不满足或初始化失败时会加载独立的 ST 包。线程池为 Wasm 实例级全局池，预留一个逻辑 CPU 给 owner，最多创建 8 个 Rayon worker。

MT framebuffer 位于 SharedArrayBuffer-backed Wasm 内存中，而 `ImageData` 不接受 shared typed array。因此 MT 呈现必须复制到普通 `Uint8ClampedArray`；ST 保留直接构造路径。这是明确的性能成本，不宣称零拷贝。

完整环境准备、命令参数、部署响应头和故障排查见[开发指南](docs/development-guide.zh-CN.md)。

## 调试页怎么用

1. 页面启动后会校验编辑器中的 WGSL，并立即在 CPU 上绘制一帧彩色三角形。
2. 编辑 WGSL 时会实时更新语法高亮和行号；点击“保存”或按 `Ctrl/⌘ + S` 会写入当前浏览器的本地存储，下次打开自动恢复。
3. 点击“编译”进行语法/语义校验，成功时列出入口点，失败时显示诊断类别、行列、源码片段和 caret。
4. 点击“运行”或按 `Ctrl/⌘ + Enter` 会先校验当前源码；校验成功后读取示例中的 `positions` / `colors` 常量并重绘 CPU Canvas，失败则保留上一帧。
5. 顶部 capability 行会显示实际加载的是 `ST` 还是 `MT`，指标行显示 submission serial、CPU 执行/呈现耗时、framebuffer hash、执行器和 WGSL 校验范围。
6. 点击 Canvas 中任意位置，会重新提交同一 command buffer，并记录该像素的 raster-stage history；“追踪中心像素”恢复追踪 framebuffer 中心像素。

当前 P0 会编译和校验完整 WGSL，但 CPU 预览只读取内置 triangle 示例的三个 `vec2f` position 与三个 `vec3f` color 十进制常量；修改这些值会改变 Canvas。任意 vertex/fragment 控制流仍不会执行，真正的 WGSL CPU 执行属于 P1 的 IR/bytecode/VM 工作。

页面上的 `CpuGpuSubset — NOT WEBGPU CONFORMANT` 是产品契约的一部分，不应移除或改成标准 WebGPU 声明。

## Workspace 结构

| 路径 | 职责 |
| --- | --- |
| `crates/glgpu-core` | DOM-free 几何、颜色、framebuffer 和稳定 hash |
| `crates/glgpu-trace` | P0 像素追踪记录 |
| `crates/glgpu-raster` | 确定性 tile 光栅器与可选 Rayon executor |
| `crates/glgpu-shader` | Naga WGSL parse/validate 边界；不暴露 Naga IR |
| `crates/glgpu-webgpu` | webgpu-mock：非标准 WebGPU 风格 P0 子集。标识保持 `CpuGpuSubset — NOT WEBGPU CONFORMANT` |
| `crates/glgpu-webgl` | webgl-mock：clear、WebGL clip/窗口、shader 驱动的三角形 demo、ESSL 子集和 CPU 解释器；不是原生 WebGL，也不是合规实现 |
| `crates/glgpu-webgl-web` | webgl-mock demo 页的 Wasm 壳；当前在主线程跑单线程 Wasm，只用 Canvas 2D 呈现 |
| `crates/glgpu-canvas2d` | RGBA8 校验与 Canvas 2D 呈现边界 |
| `crates/glgpu-web` | WebGPU Rust/Wasm 调试页与交互 |
| `xtask` | ST/MT 构建和本地隔离服务器 |
| `web` | 最小 HTML loader、Pages 隔离 worker；生成包位于被忽略的 `pkg*` 目录 |
| `.github/workflows/pages.yml` | 构建 ST/MT 并发布到 GitHub Pages |
| `examples/triangle` | P0 WGSL 校验样例 |
| `docs` | 架构、API、开发、测试、ADR 和支持矩阵 |

## 文档

- [文档索引与状态](docs/README.zh-CN.md)
- [完整架构与阶段规划](docs/architecture-plan.zh-CN.md)
- [P0 API 与行为契约](docs/api-reference.zh-CN.md)
- [开发、构建、部署与故障排查](docs/development-guide.zh-CN.md)
- [测试与验收](docs/testing-and-acceptance.zh-CN.md)
- [实施路线图](docs/roadmap.zh-CN.md)
- [P0 机器可读支持矩阵](docs/support-matrix/webgpu-p0.toml)
- [ADR 0001：WebGPU-first](docs/adr/0001-webgpu-first.md)
- [ADR 0002：ST/MT 双构建](docs/adr/0002-wasm-build-profiles.md)
- [ADR 0003：并行的 webgl-mock](docs/adr/0003-parallel-webgl-mock.md)
- [webgl-mock](docs/webgl-mock.zh-CN.md)
- [规范与依赖锁](spec-lock.toml)

## 正确性与确定性边界

- tile 可以并行，但每个 tile 内严格保留输入 triangle 顺序；
- equal-depth fragment 因 strict-less test 保留先提交的 primitive；
- framebuffer hash 包含 extent、RGBA8 字节和 depth 的精确 `f32::to_bits()`；
- 当前 hash 用于回归测试，不是密码学 hash 或长期 capture 格式；
- `PixelHistory` 只记录到达 raster sample 阶段的 prepared primitive，不包含退化或完全裁剪 primitive 的独立 cull event；
- CPU 耗时不能解释成真实 GPU 性能。

## 下一阶段

下一里程碑是项目自有 shader IR/字节码与 WGSL lowering，使 vertex/fragment shader 真正在 CPU 上执行；随后再加入资源模型、render pipeline、buffer、bind group、texture/sampler 和版本化 capture schema。异步 owner-worker queue 与 compute 是独立 gate，不会用当前同步 queue 冒充标准语义。

WebGL 的产品实现继续延后到 P7a+。当前主线不实现 ESSL parser、WebGL 状态机或 WebGL façade。

另有一条并行的 webgl-mock，见 [webgl-mock](docs/webgl-mock.zh-CN.md) 和 [ADR 0003](docs/adr/0003-parallel-webgl-mock.md)。它在 `glgpu-webgl` 里从零实现模拟 WebGL，不依赖原生 `WebGLRenderingContext` 或 `navigator.gpu`，也不依赖 webgpu-mock 的 crate。当前子集是 clear、WebGL clip 体积 `-w..w`、左下窗口，以及由 CPU 解释器输出驱动的三角形 demo。`/webgl.html` 当前用主线程上的单线程 Wasm 编辑并运行这个 demo，再用 Canvas 2D 呈现；目标后端是 Worker + Wasm，这次没有迁移。Framebuffer 第 0 行仍是过渡的图像上方。标识保持 `webgl-mock — NOT WEBGL CONFORMANT`。这不改变 P0 的 `CpuGpuSubset — NOT WEBGPU CONFORMANT` 契约，也不把 P7 标成产品已经开工。

## 许可证状态

仓库目前尚未选择许可证。在许可证文件加入前，不应默认把源码视为可再分发的开源包。
