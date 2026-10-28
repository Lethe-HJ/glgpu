# GLGPU P0 开发、构建、运行与部署指南

> 适用状态：P0 垂直切片
>
> 工具链基线：Rust 1.93.0；多线程构建使用 `nightly-2025-11-15`
>
> Web 入口：`web/index.html`；页面、样式、事件和 Canvas 2D 呈现由 Rust/Wasm 创建

本文以仓库中现有的 `Cargo.toml`、`rust-toolchain.toml`、`.cargo/mt.toml`、
`xtask`、Web loader 和静态服务器为准。所有命令默认从仓库根目录执行。

P0 是一个可运行的 CPU 参考渲染垂直切片，但不是完整或合规的 WebGPU 实现。当前可演示确定性三角形光栅化、RGBA8 + depth、选定像素历史、WGSL
解析/验证、Canvas 2D 呈现，以及单线程（ST）与 Rayon 多线程（MT）执行。当前
WGSL 尚不执行，也没有标准 `navigator.gpu` façade、纹理、绑定组、compute 或异步
queue timeline；开发和部署文档不应把这些后续功能描述为已实现。

## 1. 最短可运行路径

首次进入仓库时先准备固定工具链和 ST 构建工具：

```bash
rustup toolchain install 1.93.0 --profile minimal --component clippy --component rustfmt --target wasm32-unknown-unknown
cargo install wasm-pack --version 0.14.0 --locked
```

构建、启动 ST 演示：

```bash
cargo xtask build-web --dev
cargo xtask serve
```

打开：

```text
http://127.0.0.1:8080/
```

需要验证 MT 时，再安装锁定的 nightly 和 `wasm-bindgen` CLI：

```bash
rustup toolchain install nightly-2025-11-15 --profile minimal --component rust-src --target wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.127 --locked
cargo xtask build-web-mt
```

保持 `cargo xtask serve` 运行，然后打开：

```text
http://127.0.0.1:8080/?threads=1
```

`?threads=1` 表示“请求 MT”，不是强制 MT。能力预检或初始化失败时，loader 会记录警告并加载独立的 ST 包。

## 2. 环境与工具链

### 2.1 基础要求

- `rustup` 管理的 Rust 工具链；MT 命令通过 `cargo +nightly-2025-11-15` 选择 nightly，因此 `cargo` 应来自 rustup proxy。
- 一个支持 ES modules、WebAssembly 和 Canvas 2D 的现代浏览器。
- ST 构建需要 `wasm-pack` 在 `PATH` 中。
- MT 构建需要固定 nightly、`rust-src`、nightly 的 `wasm32-unknown-unknown` target，以及精确版本的 `wasm-bindgen` CLI。
- 项目没有 Node、npm、TypeScript、React 或其他前端构建链要求。`index.html` 中仍保留浏览器加载 Wasm、选择 ST/MT 所必需的极薄 JavaScript loader。

workspace 当前锁定 `wasm-bindgen` 0.2.127、`wasm-bindgen-rayon` 1.3.0 和
Rayon 1.12.0；`spec-lock.toml` 还把 P0 验收使用的 wasm-pack 锁为 0.14.0。
`xtask build-web` 目前只检查 `wasm-pack` 可执行文件是否存在，不会自动校验它的
版本，因此开发者和 CI 需要人工核对结果严格为 `wasm-pack 0.14.0`：

```bash
wasm-pack --version
```

`build-web-mt` 则会自动在 `PATH` 和 wasm-pack cache 中查找并严格校验
`wasm-bindgen` 0.2.127。只有把 CLI 独立安装到 `PATH` 时，才可直接用
`wasm-bindgen --version` 人工查看；该命令在 cache-only 环境中不存在，并不表示
MT 构建一定不可用。

### 2.2 稳定工具链

根目录 `rust-toolchain.toml` 固定：

- channel：`1.93.0`；
- components：`clippy`、`rustfmt`；
- target：`wasm32-unknown-unknown`；
- profile：`minimal`。

在仓库目录执行普通 `cargo`、`rustc`、`rustfmt` 和 `clippy` 命令时，rustup 会选择该版本。可用以下命令核对：

```bash
rustup show active-toolchain
rustc --version
cargo --version
rustup target list --installed
```

### 2.3 多线程工具链

MT 构建不复用稳定版标准库。`cargo xtask build-web-mt` 固定执行等价于以下配置的构建：

```text
cargo +nightly-2025-11-15 --config .cargo/mt.toml build \
  -Z build-std=std,panic_abort \
  --target wasm32-unknown-unknown \
  --release \
  -p glgpu-web \
  --features parallel
```

`.cargo/mt.toml` 只在该命令中显式加载。它启用 atomics、bulk memory、共享/导入内存和 TLS 导出，并把 linker 的最大共享内存设为 1 GiB。这是地址空间上限，不代表启动时立即分配 1 GiB。

不要把这些 `rustflags` 放进普通 `.cargo/config.toml` 或全局环境变量；ST 与 MT 必须保持两个独立 Wasm 包，避免把稳定 ST 基线意外变成 shared-memory 模块。

核对 nightly 组件：

```bash
rustup component list --toolchain nightly-2025-11-15 --installed
rustup target list --toolchain nightly-2025-11-15 --installed
```

### 2.4 `wasm-bindgen` CLI 的精确版本和查找顺序

MT 后处理只接受输出严格为 `wasm-bindgen 0.2.127` 的 CLI。`xtask` 按以下顺序收集候选并逐个执行 `--version`：

1. `PATH` 中每个目录下的 `wasm-bindgen`；
2. `WASM_PACK_CACHE` 指向的 wasm-pack cache 根；
3. `$XDG_CACHE_HOME/.wasm-pack`；
4. `$HOME/.cache/.wasm-pack`；
5. macOS 的 `$HOME/Library/Caches/.wasm-pack`；
6. Windows 的 `%LOCALAPPDATA%/.wasm-pack`。

cache 内预期路径为：

```text
wasm-bindgen-cargo-install-0.2.127/wasm-bindgen
```

Windows 上二进制文件名带 `.exe` 后缀；`xtask` 会使用当前平台的可执行文件后缀。

直接安装精确版本最明确：

```bash
cargo install wasm-bindgen-cli --version 0.2.127 --locked
wasm-bindgen --version
```

如果不想单独安装，先成功运行一次 `cargo xtask build-web` 也可能让 wasm-pack 填充相应版本的 cache；MT 查找会使用该缓存。仅安装版本不同的 CLI 不满足 MT 的可复现要求。

## 3. 仓库目录与职责

```text
glgpu/
├── Cargo.toml                  # workspace、统一版本和依赖锁定
├── Cargo.lock
├── rust-toolchain.toml         # 稳定 Rust 1.93.0、wasm target、fmt/clippy
├── .cargo/
│   ├── config.toml             # cargo xtask alias、target-dir
│   └── mt.toml                 # 仅 MT 使用的 shared-memory flags
├── crates/
│   ├── glgpu-core/             # framebuffer、颜色、顶点、三角形等基础类型
│   ├── glgpu-trace/            # P0 selected-pixel history
│   ├── glgpu-raster/           # 16×16 tile 确定性 CPU 光栅器
│   ├── glgpu-shader/           # Naga WGSL parse/validate 和诊断边界
│   ├── glgpu-webgpu/           # Rust-native CpuGpuSubset、encoder、即时 queue
│   ├── glgpu-webgl/            # webgl-mock：clip、shader 三角形 demo；不依赖 webgpu-mock crate
│   ├── glgpu-webgl-web/        # WebGL 调试页 Wasm 壳；只依赖 glgpu-webgl
│   ├── glgpu-canvas2d/         # RGBA8 校验和 Canvas ImageData 呈现
│   └── glgpu-web/              # WebGPU wasm-bindgen 导出、Rust DOM/UI、P0 demo
├── examples/triangle/          # P0 WGSL 输入；当前只解析/验证，不执行
├── examples/webgl2-triangle/   # ESSL 子集夹具；p0_vertex.essl 驱动 CPU 三角形
├── docs/
│   ├── README.zh-CN.md         # 中文文档索引
│   ├── development-guide.zh-CN.md
│   ├── api-reference.zh-CN.md  # P0 Rust API 与行为契约
│   ├── testing-and-acceptance.zh-CN.md
│   ├── architecture-plan.zh-CN.md
│   ├── roadmap.zh-CN.md
│   ├── adr/                    # 已接受的架构决策
│   └── support-matrix/         # webgpu-mock 的 P0 矩阵，以及 webgl-mock 矩阵
├── web/
│   ├── index.html              # WebGPU ST/MT loader、顶栏和 #app mount point
│   ├── webgl.html              # webgl-mock demo 页 loader；当前主线程单线程 Wasm
│   ├── api-nav.css             # 顶栏样式；两条页面共用
│   ├── coop-coep.js            # 仅 Pages 的 WebGPU MT 入口用来补 COOP/COEP
│   ├── .nojekyll               # 避免分支式 Pages 走 Jekyll
│   ├── pkg/                    # WebGPU ST 生成物；gitignored
│   ├── pkg-mt/                 # WebGPU MT 生成物及 Rayon worker snippet；gitignored
│   └── pkg-webgl/              # WebGL demo 页生成物；gitignored
├── .github/workflows/pages.yml # 构建 ST/MT 并发布 web/ 到 GitHub Pages
└── xtask/                      # Rust 编写的构建编排和带隔离头的静态服务器
```

不要手工修改 `web/pkg`、`web/pkg-mt` 或 `web/pkg-webgl`。它们是可重建产物并被 `.gitignore` 忽略；源代码改动应落在 Rust crate、`web/index.html`、`web/webgl.html`、`web/api-nav.css`、`web/coop-coep.js` 或 `xtask` 中。

根 workspace 设置了 `default-members`，普通 `cargo test` 不会覆盖
`glgpu-canvas2d`、`glgpu-web` 和 `xtask`。`glgpu-webgl-web` 的 native 测试在默认成员里。提交前使用 `--workspace`。

## 4. 构建配置总览

| 配置 | 工具链 | 并行 feature | 共享内存 | 输出目录 | 用途 |
| --- | --- | --- | --- | --- | --- |
| Native | Rust 1.93.0 | 可选 | 否 | `target/` | 快速测试、确定性验证 |
| Web ST | Rust 1.93.0 + wasm-pack | 否 | 否 | `web/pkg/` | 必须支持的浏览器基线 |
| Web MT | `nightly-2025-11-15` + build-std + wasm-bindgen 0.2.127 | 是 | 是 | `web/pkg-mt/` | 可选 Rayon tile 执行器 |

ST 和 MT 包使用相同导出名 `glgpu_web`，但存在不同目录。loader 在实例化前选择目录；不能把 MT Wasm 当作 ST 包运行。

workspace 的 release profile 使用 `opt-level = 3`、thin LTO、单个 codegen unit 和
`panic = "abort"`。显式的 ST `--release` 与固定 MT release 构建都会使用该配置。

## 5. Native 开发与验证

### 5.1 提交前检查

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

这些是当前仓库可运行的主要 native gate。P0 尚未提供浏览器自动化测试套件，浏览器部分仍需执行后文的 ST/MT smoke test。

### 5.2 针对性命令

验证顺序与 Rayon tile 路径输出一致：

```bash
cargo test -p glgpu-raster --features parallel sequential_and_rayon_outputs_are_identical
```

验证 WebGPU P0 queue、shader 边界等 native 行为：

```bash
cargo test -p glgpu-webgpu -p glgpu-shader
```

验证 Canvas RGBA8 边界和 `xtask` 的命令、路径、MIME、隔离头逻辑：

```bash
cargo test -p glgpu-canvas2d -p xtask
```

仅检查稳定版 Web Wasm 编译：

```bash
cargo check --target wasm32-unknown-unknown -p glgpu-web
```

`glgpu-core`、`glgpu-trace`、`glgpu-raster`、`glgpu-shader` 和
`glgpu-webgpu` 不依赖 DOM；优先把语义和确定性测试写成 native test，浏览器层只保留真正需要 Web API 的集成验证。

## 6. 单线程 Web 构建

### 6.1 默认构建

```bash
cargo xtask build-web
```

`xtask` 实际向 wasm-pack 传入 workspace 绝对输出路径。若从仓库根手工写成等价命令，必须注意 wasm-pack 会相对 crate path 解析相对 `--out-dir`，因此可运行形式是：

```bash
wasm-pack build --target web \
  --out-dir ../../web/pkg \
  --out-name glgpu_web \
  crates/glgpu-web
```

输出至少包括 loader 使用的：

```text
web/pkg/glgpu_web.js
web/pkg/glgpu_web_bg.wasm
```

需要明确选择开发或发布 profile 时：

```bash
cargo xtask build-web --dev
cargo xtask build-web --release
```

日常快速迭代使用 `--dev`；部署前使用 `--release`。不传 profile 时沿用 wasm-pack 自身的默认 profile。

### 6.2 可覆盖参数

`build-web` 支持：

```text
cargo xtask build-web \
  [--crate <wasm-crate>] \
  [--out-dir <directory>] \
  [--out-name <name>] \
  [--dev|--release] \
  [-- <extra cargo build args>]
```

相对路径按 workspace 根目录解析。当前 `web/index.html` 固定导入
`./pkg/glgpu_web.js`，因此自定义输出目录或名称后，现有页面不会自动改用新路径。

ST 构建不要启用 `glgpu-web/parallel`。`parallel` 属于 MT profile，并依赖 shared-memory flags、nightly 标准库重建和线程池初始化。

## 7. 多线程 Web 构建

### 7.1 构建命令

```bash
cargo xtask build-web-mt
```

该命令有意不接受路径、profile 或额外 Cargo 参数，以保持 P0 shared-memory profile 可复现。它依次执行：

1. 按第 2.4 节的顺序查找并验证 `wasm-bindgen` CLI 0.2.127；
2. 使用 `nightly-2025-11-15`、`.cargo/mt.toml`、`-Z build-std=std,panic_abort` 和 `glgpu-web/parallel` 进行 release 构建；
3. 检查 `target/wasm32-unknown-unknown/release/glgpu_web.wasm`；
4. 执行 `wasm-bindgen --target web`，写入 `web/pkg-mt`。

MT 输出包括：

```text
web/pkg-mt/glgpu_web.js
web/pkg-mt/glgpu_web_bg.wasm
web/pkg-mt/snippets/wasm-bindgen-rayon-*/src/workerHelpers.no-bundler.js
```

部署 MT 时必须把整个 `pkg-mt` 目录作为同一版本发布，不能只复制 `.wasm` 和顶层 `.js` 而遗漏 worker snippet。

### 7.2 Canvas 2D 的 MT 边界

MT Wasm 的线性内存基于 `SharedArrayBuffer`。浏览器的 `ImageData` 构造器不能直接接受 shared typed array，因此 `glgpu-canvas2d` 在 atomics 构建中先复制到普通 JavaScript `ArrayBuffer`，再调用 `putImageData`。ST 保留直接路径。

如果 MT 页面出现 `ImageData`/shared typed array 相关异常，通常说明正在使用旧产物、部署时 JS/Wasm 版本混合，或代码绕开了 `present_rgba8`；这不是应通过关闭校验规避的限制。

## 8. 本地运行和静态服务器

### 8.1 默认运行

先至少生成 ST 包，然后启动：

```bash
cargo xtask serve
```

默认行为：

- 静态目录：`web/`；
- 监听地址：`127.0.0.1:8080`；
- 支持方法：GET、HEAD；
- `/` 映射到 `index.html`；
- 查询参数不参与文件路径解析；
- 禁止目录穿越和越过静态根的 symlink；
- 根据扩展名发送 JavaScript、Wasm、JSON、图片等 MIME；
- 所有成功和错误响应都发送隔离及安全相关响应头；
- `Cache-Control: no-store`，方便本地迭代避免旧包缓存。

换端口：

```bash
cargo xtask serve --bind 127.0.0.1:8081
```

服务器也可以监听所有网卡：

```bash
cargo xtask serve --bind 0.0.0.0:8080
```

这不会自动提供 HTTPS。其他设备通过普通局域网 HTTP 访问时，不应期待 MT 安全上下文成立；需要远程 MT 测试时，应在前面配置保留隔离头的 HTTPS 反向代理。

显式指定静态目录：

```bash
cargo xtask serve --dir web --bind 127.0.0.1:8080
```

查看命令总览：

```bash
cargo xtask help
```

该服务器是开发辅助工具：它是简单的同步静态服务器，不提供 TLS、压缩、访问控制、负载均衡或生产运维能力，不应直接作为公网生产服务器。

### 8.2 ST smoke test

1. 打开 `http://127.0.0.1:8080/`。
2. 页面应显示 720×480 Canvas、彩色三角形和像素历史面板。
3. metrics 应包含 `sequential tile executor` 和稳定 framebuffer hash。
4. 点击 Canvas 不同像素，页面应使用新的 selected pixel 重新编码并提交同一绘制场景，再更新 coverage/depth/color 历史。
5. 浏览器 Console 不应出现 `GLGPU boot failed`。

### 8.3 MT smoke test

1. 已生成 `web/pkg` 和 `web/pkg-mt`。
2. 打开 `http://127.0.0.1:8080/?threads=1`。
3. 真正进入 MT 时，metrics 应包含 `Rayon tile executor`。
4. 记录 ST 与 MT 页面显示的 hash；相同 P0 场景必须一致。
5. Console 不应出现 MT fallback 警告或 worker/module 加载错误。

如果页面显示 `sequential tile executor`，说明请求已回退到 ST；页面仍可用，但 MT 验收尚未通过。

## 9. Loader、浏览器选择与 worker 上限

### 9.1 启动选择

`web/index.html` 在加载 Wasm 前读取查询参数：

- 无 `threads=1`：只加载 `web/pkg`，调用 `boot(false)`；
- 有 `threads=1` 且能力预检通过：加载 `web/pkg-mt`，初始化线程池，再调用 `boot(true)`；
- 能力预检失败：不实例化 MT，直接加载 ST；
- MT 包导入、Wasm 初始化、线程池初始化或 `boot(true)` 任一阶段失败：记录 `GLGPU MT boot failed; falling back to ST`，再加载独立 ST 包。

预检要求同时满足：

- `globalThis.crossOriginIsolated === true`；
- `SharedArrayBuffer` 存在；
- `navigator.hardwareConcurrency > 1`；
- 可以创建 shared `WebAssembly.Memory`，且其 buffer 是 `SharedArrayBuffer`。

这是一项能力检测，不是 user-agent 名单。不要根据浏览器名称假定 MT 一定开启。

### 9.2 浏览器建议

- ST 是项目必须保持可用的基线，适合日常功能开发和 MT 不可用的浏览器/嵌入环境。
- 当前仓库记录的 P0 ST/MT 实测浏览器是本地 Chromium，因此做 MT 首轮验收时优先使用 Chromium。
- Firefox 是计划中的首批兼容目标，应单独执行相同 smoke test；是否启用 MT仍以页面能力检测结果为准。
- Safari 和受限 WebView 当前允许使用 ST fallback；不要把能打开页面等同于已经验证 shared-memory worker 路径。
- 无痕策略、企业策略、iframe 沙箱或宿主应用可能关闭共享内存能力，即使同一浏览器的普通顶层页面可以使用。

### 9.3 worker 数量

loader 的固定上限为 8，计算公式是：

```text
min(8, max(1, navigator.hardwareConcurrency - 1))
```

它为主 owner 逻辑预留一个逻辑 CPU。例如浏览器报告 2 个逻辑 CPU 时创建 1 个 Rayon worker，报告 8 个时创建 7 个，报告 9 个或更多时最多创建 8 个。若报告值不大于 1，预检直接选择 ST。

`navigator.hardwareConcurrency` 是浏览器提供的提示值，可能因隐私策略或资源限制低于物理硬件数量。当前 worker cap 写在 `web/index.html` 的 `maxRayonWorkers`，不是运行时 UI 设置。

## 10. COOP/COEP 与跨源隔离

### 10.1 开发服务器发送的头

`cargo xtask serve` 对每个响应发送：

| Header | 值 | 目的 |
| --- | --- | --- |
| `Cross-Origin-Opener-Policy` | `same-origin` | 建立独立 browsing context group |
| `Cross-Origin-Embedder-Policy` | `require-corp` | 只嵌入明确允许的资源 |
| `Cross-Origin-Resource-Policy` | `same-origin` | 当前静态资源只允许同源使用 |
| `X-Content-Type-Options` | `nosniff` | 禁止 MIME 猜测 |
| `Cache-Control` | `no-store` | 避免开发期混用旧 JS/Wasm |

本地开发应通过 `http://127.0.0.1:8080` 访问，不要双击
`web/index.html` 使用 `file://`。loopback origin 可用于本地开发，且 loader 仍会以 `crossOriginIsolated` 的真实值决定是否启动 MT。

验证文档与 Wasm 响应：

```bash
curl -I http://127.0.0.1:8080/
curl -I http://127.0.0.1:8080/pkg-mt/glgpu_web_bg.wasm
```

在浏览器 Console 可分别检查：

```javascript
crossOriginIsolated
typeof SharedArrayBuffer
navigator.hardwareConcurrency
```

### 10.2 自定义部署时的要求

生产或远程设备访问还需要安全上下文，通常意味着 HTTPS。仅设置 COOP/COEP 并不能让普通远程 HTTP origin 变成安全上下文；`xtask serve` 本身不提供 TLS。

生产静态服务器或 CDN 至少需要：

1. 对 HTML 设置 `COOP: same-origin` 和 `COEP: require-corp`；建议对同一应用的 JS、Wasm 和 worker 资源保持与开发服务器一致的头。
2. 以 `application/wasm` 返回 `.wasm`，以 JavaScript MIME 返回 `.js`。
3. 同源发布 `index.html`、`pkg`、`pkg-mt` 和 MT 的 `snippets`；若引入跨源资源，需要对应的 CORS/CORP 策略，否则 `require-corp` 会阻止加载。
4. 将同一次构建生成的 JS、Wasm、类型文件和 worker snippet 原子发布，避免 glue 与 Wasm 版本错配。
5. 为版本化产物设计一致的缓存失效策略。开发服务器的 `no-store` 不能代替生产部署的版本管理。GitHub Pages 的具体限制和 service worker 见第 11.1 节。
6. 若增加 Content Security Policy，需要同时覆盖以下边界；仓库当前没有生产 CSP 模板：
   - `script-src` 允许 `'self'`、用 nonce/hash 允许当前 inline module loader，并加入更窄的 `'wasm-unsafe-eval'` 以允许 `WebAssembly.instantiate`/`instantiateStreaming`。只在必须兼容不认识该关键字的旧实现时才考虑更宽的 `'unsafe-eval'`，因为后者也会放开 JavaScript eval。
   - `worker-src` 至少允许 `'self'` 和 `blob:`；`wasm-bindgen-rayon` 的 no-bundler helper 会把同源 helper fetch 为 Blob，再通过 `new Worker(blob:..., { type: "module" })` 启动。
   - `connect-src` 或其 `default-src` fallback 允许 `'self'`，因为生成的 glue 会 fetch Wasm。
   - `style-src` 允许 `glgpu-web` 在运行时创建的 inline `<style>`。当前代码没有给该元素附加 nonce；生产部署可使用与实际样式文本匹配的 hash，或先改代码把部署 nonce 传入 Rust 并设置到元素上，否则页面功能可启动但样式会被阻止。

   WebAssembly 与 inline script/style 的 CSP gate 见 [W3C CSP Level 3](https://www.w3.org/TR/CSP/)。

## 11. 部署流程

### 11.1 GitHub Pages

`.github/workflows/pages.yml` 在 `main` 推送和手动运行时构建并发布；pull request 只构建和检查，不部署。发布根目录是 `web/`，其中包含：

```text
index.html
webgl.html
api-nav.css
coop-coep.js
.nojekyll
pkg/glgpu_web.js
pkg/glgpu_web_bg.wasm
pkg-mt/glgpu_web.js
pkg-mt/glgpu_web_bg.wasm
pkg-mt/snippets/**
pkg-webgl/glgpu_webgl_web.js
pkg-webgl/glgpu_webgl_web_bg.wasm
```

CI 安装的工具与本指南的锁一致：Rust `1.93.0`、`wasm-pack 0.14.0`、`wasm-bindgen 0.2.127`，以及带 `rust-src` 的 `nightly-2025-11-15`。WebGPU ST 用 `cargo xtask build-web --release`，WebGPU MT 用 `cargo xtask build-web-mt`，WebGL demo 页用 `cargo xtask build-web --release --crate crates/glgpu-webgl-web --out-dir web/pkg-webgl --out-name glgpu_webgl_web`。

启用站点：

1. 把 workflow 合并到默认分支 `main`。
2. 打开仓库 **Settings → Pages → Build and deployment → Source**，选择 **GitHub Actions**。
3. 如果第一次 `deploy` 作业因为 Pages 未启用而失败，保存上述设置后重跑 `pages` workflow。
4. 部署完成后打开项目站点。本仓库是 `Lethe-HJ/glgpu`，地址为 `https://Lethe-HJ.github.io/glgpu/`。仓库改名后，路径改为 `/<仓库名>/`；用户主页仓库 `<user>.github.io` 没有这个前缀。

相对 URL 是项目页的 base path 方案。`index.html` 导入 `./pkg/glgpu_web.js` 和 `./pkg-mt/glgpu_web.js`，`webgl.html` 导入 `./pkg-webgl/glgpu_webgl_web.js`。生成的 Wasm glue 与 Rayon `no-bundler` helper 使用 `import.meta.url`。因此本地 `http://127.0.0.1:8080/pkg/...` 和 Pages `https://Lethe-HJ.github.io/glgpu/pkg/...` 共用同一份 loader，WebGL 页同样落在 `/glgpu/webgl.html`。不要添加 `<base>`，它会让这两种部署解析到不同路径。

GitHub Pages 和 `actions/deploy-pages` 都不能设置 COOP/COEP，`_headers` 在这里无效。直接 `curl -I` 项目站点看不到隔离头，这是平台行为。

页面上的分工是：

| URL | 行为 |
| --- | --- |
| `/` 或 `/glgpu/` | 只加载 WebGPU ST。不注册 service worker。顶栏可重载到 WebGL 页 |
| `/webgl.html` 或 `/glgpu/webgl.html` | 只加载 WebGL demo 页 Wasm。不注册 service worker |
| `/?threads=1` 且服务器已隔离 | 直接加载 WebGPU MT。`cargo xtask serve` 走这里 |
| `/?threads=1` 且尚未隔离 | 注册 `coop-coep.js`，刷新一次，再加载 MT |
| 隔离头被浏览器拒绝 | loader 记录警告并加载 ST |

service worker 写入的值与本地服务器相同：`Cross-Origin-Opener-Policy: same-origin`、`Cross-Origin-Embedder-Policy: require-corp`、缺少时再补 `Cross-Origin-Resource-Policy: same-origin`。响应已经带有前两个头时原样转发。刷新前会暂时加上 `glgpu_isolation=1`，成功隔离后从地址栏去掉，用来在 `sessionStorage` 不可用时防止无限刷新。硬刷新会绕过 worker。无痕模式可能拒绝 service worker，结果同样是 ST。

Pages 上的 MT 是可用路径，不是本地验收的替代品。改 loader、worker 或隔离头后，仍要用 `cargo xtask serve` 对 `/` 和 `/?threads=1` 做 smoke test。

### 11.2 其他静态宿主

可复现的构建边界如下：

```bash
cargo test --workspace
cargo xtask build-web --release
cargo xtask build-web-mt
cargo xtask build-web --release --crate crates/glgpu-webgl-web --out-dir web/pkg-webgl --out-name glgpu_webgl_web
```

随后将 `web/` 作为静态根交给满足上一节要求的 HTTPS server/CDN。同时发布两条入口时至少应包含：

```text
web/index.html
web/webgl.html
web/api-nav.css
web/pkg/glgpu_web.js
web/pkg/glgpu_web_bg.wasm
web/pkg-mt/glgpu_web.js
web/pkg-mt/glgpu_web_bg.wasm
web/pkg-mt/snippets/**
web/pkg-webgl/glgpu_webgl_web.js
web/pkg-webgl/glgpu_webgl_web_bg.wasm
```

只提供 ST 的环境至少发布 `index.html` 和完整 `pkg`，并只宣传无
`threads=1` 的入口。若客户端在具备线程能力时请求 `threads=1` 而服务器没有
`pkg-mt`，loader 会因 MT import 失败而回退 ST，但会产生 Console 警告，这不算 MT 部署成功。

上线前分别检查：

- `/` 和 `/?threads=1` 的实际 executor 文本；
- ST/MT hash 是否一致；
- HTML、`.js`、`.wasm` 和 worker snippet 的状态码、MIME 与隔离头；
- Console 是否有 fallback、CSP、COEP、worker 或 Wasm 实例化错误；
- 生产域名下 `crossOriginIsolated` 是否为 `true`。

## 12. 推荐开发流程

1. 从 native crate 开始实现语义，保留核心库无 DOM，并添加确定性/错误路径测试。
2. 运行目标 crate 测试；涉及 tile 并行时同时测试默认和 `parallel` feature。
3. 运行 `cargo check --target wasm32-unknown-unknown -p glgpu-web`，尽早发现 Wasm 边界问题。
4. 用 `cargo xtask build-web --dev` 生成 ST 包，执行 ST smoke test。
5. 改动 Rayon、共享内存、Canvas presentation、loader 或部署头时，必须重新执行 `cargo xtask build-web-mt` 和 MT smoke test。
6. 把 ST/MT hash 不一致视为正确性回归；先最小化输入并在 native 的顺序/Rayon测试中复现，不要用容差掩盖。
7. 提交前运行 workspace fmt、test 和 clippy gate。
8. 若修改支持范围，同步更新 `docs/support-matrix/webgpu-p0.toml`；不要把仅在路线图中的能力写成已实现。

webgpu-mock UI 的 DOM、样式和事件位于 `crates/glgpu-web`；webgl-mock demo 页位于 `crates/glgpu-webgl-web`，当前在主线程运行。webgpu-mock 的 Canvas byte boundary 位于
`glgpu-canvas2d`；WebGL 页在自己的壳里复制 RGBA8。`web/index.html` 继续只负责顶栏、mount point、能力检测、线程池初始化和 ST fallback。`web/webgl.html` 只负责顶栏和那一条 Wasm 启动。Pages 上的隔离注册放在 `web/coop-coep.js`，WebGL 页不注册它。不要把业务 UI 迁到手写 JavaScript。

## 13. 常见错误与排查

### 13.1 `could not find wasm-pack`

原因：`build-web` 无法在 `PATH` 中启动 wasm-pack。

```bash
cargo install wasm-pack --version 0.14.0 --locked
wasm-pack --version
cargo xtask build-web --dev
```

若已安装仍失败，确认启动 Codex、终端或 IDE 的进程继承了包含该二进制目录的 `PATH`。

### 13.2 稳定 Wasm target 缺失

典型表现是找不到 `wasm32-unknown-unknown` 的 `core`/`std`。

```bash
rustup target add wasm32-unknown-unknown --toolchain 1.93.0
cargo check --target wasm32-unknown-unknown -p glgpu-web
```

### 13.3 MT nightly、target 或 `rust-src` 缺失

`xtask` 会报告 multi-thread cargo build 失败，并提示固定 nightly、target 和
`rust-src`。

注意 MT 命令会先检查精确版本的 `wasm-bindgen` CLI；如果 CLI 与 nightly 同时缺失，首先看到的会是第 13.4 节的 CLI 错误，补齐 CLI 后才进入 Cargo 构建。

```bash
rustup toolchain install nightly-2025-11-15 --profile minimal --component rust-src --target wasm32-unknown-unknown
cargo xtask build-web-mt
```

不要用任意最新 nightly 替换固定版本后把产物当作可复现的 P0 MT profile。

### 13.4 找不到兼容的 `wasm-bindgen` CLI

典型错误包含：

```text
could not find wasm-bindgen CLI 0.2.127 on PATH or in the wasm-pack cache
```

如果 CLI 独立安装在 `PATH`，可先核对：

```bash
wasm-bindgen --version
```

若不是 `wasm-bindgen 0.2.127`：

```bash
cargo install wasm-bindgen-cli --version 0.2.127 --locked
cargo xtask build-web-mt
```

cache-only 环境中 `wasm-bindgen --version` 可以是 command not found；此时直接运行
`cargo xtask build-web-mt`，让 `xtask` 自动检查 cache。真正查找失败时，错误信息会
列出找到但版本不兼容的候选路径；优先修正 PATH 中的版本或让精确版本进入受支持的
cache 位置。

### 13.5 端口已被占用

换用明确的 loopback 端口：

```bash
cargo xtask serve --bind 127.0.0.1:8081
```

然后用相同端口打开 ST/MT URL。

### 13.6 页面 404、空白或报 MIME 错误

- 确认从 workspace 根运行，默认静态根为 `web`。
- 确认先运行过 `cargo xtask build-web`，并存在 `web/pkg/glgpu_web.js` 与 `.wasm`。
- 不要用不识别 `.wasm` MIME 的通用文件服务器替代 `cargo xtask serve`。
- 自定义 `--out-dir` 不会自动更新 `web/index.html` 的 import 路径。
- 查看 Console 中 `GLGPU boot failed` 的原始异常和 Network 面板中的实际请求路径。

### 13.7 请求 MT 却显示 ST

这是 loader 的设计性 fallback。依次检查：

1. URL 查询参数 `threads` 的首个值是否严格等于 `1`（例如 `?foo=x&threads=1` 也有效）；
2. `crossOriginIsolated` 是否为 `true`；
3. `SharedArrayBuffer` 是否存在；
4. `navigator.hardwareConcurrency` 是否大于 1；
5. `pkg-mt/glgpu_web.js`、`.wasm` 和 `snippets/**` 是否都能加载；
6. Console 是否出现 `GLGPU MT requested without cross-origin isolation; using ST` 或 `GLGPU MT boot failed; falling back to ST`。

第一条警告文字提到 cross-origin isolation，但当前 loader 也会在硬件并发不大于 1 或 shared Wasm memory 探测失败时走同一分支；应检查全部预检条件。

### 13.8 `crossOriginIsolated` 为 `false`

- 使用 `cargo xtask serve`，不要用 `file://`。
- 用 `curl -I` 检查顶层 HTML 的 COOP/COEP。
- 远程环境使用 HTTPS；普通远程 HTTP 即使有 COOP/COEP 也可能不是安全上下文。
- 检查反向代理/CDN 是否删除或覆盖响应头。
- 检查页面是否处于受限 iframe，以及页面或其资源是否被 COEP/CORP 阻止。

### 13.9 MT worker 或 module 加载失败

- 确认完整发布 `web/pkg-mt/snippets`。
- 确认 worker helper 与顶层 glue、Wasm 来自同一次构建。
- 检查 `.js`/`.wasm` MIME、同源路径、COEP/CORP 和 CSP 的 worker 规则。
- 重新运行 `cargo xtask build-web-mt`，不要只替换单个生成文件。

### 13.10 `parallel=true requires the glgpu-web parallel feature`

这表示 loader 正在用并行模式启动一个未带 `parallel` feature 的包，常见于目录混用或旧产物。ST 必须调用 `boot(false)`；MT 必须由
`cargo xtask build-web-mt` 生成并在 `initThreadPool` 完成后调用 `boot(true)`。重新生成并成套部署两个目录。

### 13.11 `ImageData` 拒绝 shared typed array

当前 MT 路径已经在 `glgpu-canvas2d::present_rgba8` 内做 ordinary-ArrayBuffer staging copy。出现此错误时：

- 排查缓存或 JS/Wasm 版本混用；
- 确认调用的是 crate 的 presentation helper；
- 重新生成完整 MT 包；
- 不要把 SharedArrayBuffer view 直接传给 `ImageData`。

### 13.12 ST 与 MT hash 不一致

这是确定性 bug，不是预期浮点噪声。先运行：

```bash
cargo test -p glgpu-raster --features parallel sequential_and_rayon_outputs_are_identical
```

记录浏览器版本、URL、`hardwareConcurrency`、ST/MT hash、Console 和最小场景。检查是否部署了同一源版本的两个包，再定位 tile binning、提交顺序或 trace 合并逻辑。

### 13.13 `build-web-mt` 拒绝额外参数

这是有意限制。该命令只接受：

```bash
cargo xtask build-web-mt
```

若确实需要改变 nightly、shared-memory flags、feature 或输出布局，应先把它作为构建 profile 变更评审，并同步更新 `.cargo/mt.toml`、`xtask`、ADR 和本指南，而不是临时绕过锁定命令。

## 14. 命令速查

```bash
# 查看 xtask 命令
cargo xtask help

# Native gate
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings

# ST
cargo xtask build-web --dev
cargo xtask build-web --release

# MT（固定 release profile）
cargo xtask build-web-mt

# 本地服务
cargo xtask serve
cargo xtask serve --bind 127.0.0.1:8081

# Wasm target check
cargo check --target wasm32-unknown-unknown -p glgpu-web
```

功能边界以 [WebGPU P0 支持矩阵](support-matrix/webgpu-p0.toml) 为准；公开 API
与行为契约见 [P0 API 参考](api-reference.zh-CN.md)；测试分层、浏览器 smoke test 和
验收记录见 [测试与验收指南](testing-and-acceptance.zh-CN.md)；长期阶段与 gate 见
[架构规划](architecture-plan.zh-CN.md)；ST/MT 分包决策见 [ADR 0002](adr/0002-wasm-build-profiles.md)。
