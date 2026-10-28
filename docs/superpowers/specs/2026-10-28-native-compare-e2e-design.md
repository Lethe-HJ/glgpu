# 原生画面对照：设计说明

> 状态：已批准，尚未实现  
> 日期：2026-10-28  
> 范围：`web/index.html` 的 WebGPU 调试页、`web/webgl.html` 的 WebGL 学习页、本地 `cargo xtask e2e-compare`  
> 不在本说明的实现范围：GitHub Actions、golden PNG、WebGPU CTS、WebGL conformance

本说明只定义设计。实现另做。当前仓库没有这条对照路径。

## 1. 背景

GLGPU 的产品路径是 CPU 参考实现。WebGPU 页用 `glgpu-web` 的 CPU 光栅把 RGBA8 交到 Canvas 2D。WebGL 页用 `glgpu-webgl` 的 CPU 光栅做同样的事。两页都声明自己不是浏览器实现：`CpuGpuSubset — NOT WEBGPU CONFORMANT` 与 `CpuWebGl2LearningTrack — NOT WEBGL CONFORMANT`。

现有正确性门是内部 framebuffer。`Framebuffer::stable_hash64()` 以及 L1–L8 的 hash、完整 color/depth、stats、history 比较继续有效。那些测试回答“CPU 路径是否自洽”。它们不回答“这块 CPU 画面和同一场景的浏览器原生绘制是否逐字节相同”。

调试页今天不能在同一视口里切换到原生 WebGL2 或 WebGPU。仓库也没有一条本地命令去做画布像素比较。[ADR 0003](../../adr/0003-parallel-webgl2-learning-track.md) 禁止把宿主 WebGL / WebGPU 当作学习栈或产品栈的渲染后端，并写明宿主 GPU 若出现，只能作为测试 oracle。本设计按这条边界把原生绘制放在页面旁路里。

两条 CPU 栈的覆盖规则不同。`glgpu-webgl` 的像素规则不保证与 `glgpu-raster` 逐像素相同。因此本设计不要求两页彼此相等，只要求每一页的 CPU 参考帧等于该页的原生帧。

## 2. 目标

1. 端到端正确性在这里的定义是：固定场景下，CPU 参考实现画进画布的 RGBA，与等价的原生 WebGL2 或 WebGPU 画进画布的 RGBA，逐字节相等。不使用容差，不做近似比较，也不忽略边缘像素。
2. 两页都能在同一视口位置切换「参考实现 | 原生」，让人直接看到原生结果。
3. 一条本地命令完成构建、打开两页、采样、比较。失败时写出 diff PNG 并以非零状态退出。
4. 本切片不进入 CI。`stable_hash64` 与 L1–L8 保持为另一层门。原生对照是附加的“与原生一致”门。

## 3. 非目标

- 不跑 WebGPU CTS，也不跑 WebGL conformance。
- 原生路径不是产品 API，不注入 `navigator.gpu`，也不是 `WebGL2RenderingContext` 的 façade。
- 不把宿主 GPU 接进 `glgpu-webgpu`、`glgpu-webgl` 或任何 CPU crate 的绘制后端。
- 不用离线 golden PNG 作为主 oracle。
- 不把内部 buffer hash 当作这条门的主 oracle。hash 门继续单独存在。
- 不改 `.github/workflows/pages.yml`，不加新的 workflow。
- 不在这条命令里启动 MT（`?threads=1`），也不比较 ST/MT。
- 不让原生模式编译、运行或单步跟踪编辑器里的源码。
- 不要求 WebGPU 页的参考帧等于 WebGL 页的参考帧。
- 不改两条 CPU 光栅器的覆盖规则来迎合某张显卡。
- 不引入动画、MSAA、抗锯齿或第二块并排画布。

## 4. 已定决策

### 4.1 采用方案 A

共享一份场景常量，参考后端与原生后端都画这一帧，对当场画进画布的像素做精确 RGBA 比较。

不采用的做法：

- 方案 B：事先提交 golden PNG，再和原生截图比。画面一变就要更新图片，而且看不到参考实现与原生并存在同一视口。
- 方案 C：只比较 CPU buffer 或 `stable_hash64`，不把原生帧画到页面上。这不能满足“用户能看到原生结果”。

### 4.2 精确 RGBA，失败就改场景或参考帧的提交方式

比较的是 320×240 的 RGBA8，共 `320 × 240 × 4 = 307200` 字节。任一字节不同即失败。

本地 GPU 若仍有 1 像素边缘差，或任意通道差 1：命令失败并写出 diff。不放宽阈值，不引入 per-channel epsilon，不把失败改成 skip。

第 5.2 节的分辨率、清屏色、三个 NDC 位置、clip z/w 和三个顶点颜色是本切片的场景契约。实现按这组数落地。精确比较失败时，按下面的顺序修，修完仍然要求逐字节相等：

1. 修正接线错误：画布尺寸、`devicePixelRatio`、预乘 alpha、颜色附件格式、Y 朝向、读回路径。原生着色器保持第 5.4 节的等价 WGSL / ESSL，不改成按像素抄 CPU 图像。
2. 修正参考帧提交：仍然调用现有 CPU 光栅器，顶点仍由第 5.3 节的公式从第 5.2 节的常量得到。
3. 若边缘覆盖仍然不一致，在同一次修订里改 `web/compare-scene.js` 和第 5.2 节的 NDC xy，并按第 5.3 节的公式重算该节的连续存储坐标。NDC 仍然只有一份，修订后的位置必须同时满足两条 CPU 规则：每个像素中心要么严格在三角形内，要么严格在三角形外。分辨率保持 320×240，清屏色、clip z/w 和三个顶点颜色保持不变。不为 WebGPU 页和 WebGL 页各写一套坐标。

第 3 步之后仍然不相等，就保持失败。不得改用容差宣布完成，也不得在未修订本说明的情况下换一组数字。

不允许的修复：

- 修改 `glgpu-raster` 或 `glgpu-webgl` 的覆盖、top-left、采样点规则来迁就 GPU；
- 把 L1–L8 的精确 hash / framebuffer 比较改成近似比较；
- 在测试里忽略边缘像素或某个颜色通道；
- 让原生着色器读取 CPU 帧缓冲再写回。

### 4.3 只在本地跑

命令是：

```bash
cargo xtask e2e-compare
```

不要求 GitHub Actions。缺少原生能力时硬失败，并提示使用本机 Google Chrome。不是给 CI 用的软跳过。

### 4.4 同一视口，默认参考实现

两页都在现有画布所在的位置放分段控件「参考实现 | 原生」。默认是「参考实现」。不是左右并排，也不新开页面。

一个 `HTMLCanvasElement` 不能同时持有 Canvas 2D 和 WebGL2 / WebGPU 上下文。实现因此使用一个视口槽：同一布局位置只显示一块画布；切换时换掉槽里的那一块画布元素。用户看到的仍是一个矩形，位置不变。

### 4.5 两页都做

| 页面 | 参考实现 | 原生 |
| --- | --- | --- |
| `web/index.html` | 现有 `glgpu-web` CPU Wasm，Canvas 2D `putImageData` | 浏览器 WebGPU，等价 WGSL，无 MSAA |
| `web/webgl.html` | 现有 `glgpu-webgl-web` CPU Wasm，Canvas 2D `putImageData` | 浏览器 WebGL2，等价 ESSL 3.00，`antialias: false` |

`index.html` 不加载 WebGL 原生模块。`webgl.html` 不加载 WebGPU 原生模块。两条 Wasm 包仍然互不加载。

### 4.6 原生调用留在页面 JS

`crates/glgpu-webgpu`、`crates/glgpu-webgl`、`crates/glgpu-raster` 不增加 `WebGL2RenderingContext` 或 `GPUDevice` 依赖。`crates/glgpu-web` 与 `crates/glgpu-webgl-web` 负责创建切换控件、在参考模式下把 CPU RGBA8 交给 Canvas 2D，并调用页面上的一个 JS 入口来切换模式。它们不为这条功能打开 web-sys 的 WebGL2 或 WebGPU feature。

这不修改 ADR 0001 的产品顺序，也不把 ADR 0003 的学习栈改成浏览器后端。原生模块是测试 oracle 和观看用的旁路。若以后要把宿主 GPU 变成产品后端，需要新的 ADR。本切片不写那份 ADR。

### 4.7 编辑器只属于参考模式

「编译」「运行」和点击 trace 继续走现有 CPU 路径。原生模式下编辑器只读，`「编译」`和`「运行」`禁用但仍留在页面上，避免用户以为原生在执行完整调试栈。trace 面板保留上一次参考模式的文本，原生模式不更新它。

`「运行」`仍然提交现有的 720×480 编辑器预览，并把它画进同一视口。该预览不参与原生比较。再次点「参考实现」会丢掉这块预览，重新画 320×240 的固定对照场景。

## 5. 架构

```text
web/compare-scene.js          唯一的场景数字来源
        │
        ├── 参考实现：JS 把常量传入 Wasm compare_frame_rgba8
        │              CPU 光栅 → RGBA8 → 槽内 Canvas 2D putImageData
        │
        ├── 原生 WebGPU：web/native-webgpu.js
        │              槽内新画布，WGSL，一次提交
        │
        └── 原生 WebGL2：web/native-webgl.js
                       槽内新画布，ESSL，一次绘制

cargo xtask e2e-compare
        │
        ├── 构建 web/pkg 与 web/pkg-webgl（都不构建 MT）
        ├── cargo xtask serve --bind 127.0.0.1:8765
        └── Playwright 启动本机 Google Chrome
                ├── GET /
                └── GET /webgl.html
                      参考实现采样 → 原生采样 → 逐字节比较
```

页面产品代码仍然由 Rust/Wasm 绘制调试 UI。`e2e/` 的 Node 依赖只服务于这条本地命令，不参与页面构建，不引入 Vite，也不改变“调试页没有前端打包链”的现状。

### 5.1 页面行为

两页在视口旁显示第 4.4 节的分段控件。控件是一组按钮：

- 可访问名称分别是 `参考实现` 和 `原生`；
- 容器 `aria-label="对照后端"`；
- 当前项 `aria-pressed="true"`。

视口槽是页面里唯一用来对照的画布容器。槽上有：

| 属性 | 取值 |
| --- | --- |
| `data-compare-state` | `reference`、`native` 或 `editor` |
| `data-compare-ready` | 像素已经呈现时为 `true`；失败时为 `error` |
| `data-compare-error` | 仅在 `error` 时存在，内容为下面第 7.4 节的对应句子 |

加载完成后、用户尚未点击「运行」时：状态是 `reference`，画布是固定对照场景，`data-compare-ready="true"`。Wasm 尚未画完之前不设置 `data-compare-ready`。

状态文案：

| 状态 | 文案 |
| --- | --- |
| 固定场景，参考实现 | `正在显示：固定对照场景 · 参考实现` |
| 固定场景，原生 | `正在显示：固定对照场景 · 原生` |
| 编辑器预览 | `正在显示：编辑器预览 · 不参与原生对照` |

WebGPU 页附加说明：

`原生对照只用于观看画面和本地像素比较。它不运行编辑器里的调试流程，也不改变 CpuGpuSubset — NOT WEBGPU CONFORMANT。`

WebGL 页附加说明：

`原生对照只用于观看画面和本地像素比较。它不运行编辑器里的调试流程，也不改变 CpuWebGl2LearningTrack — NOT WEBGL CONFORMANT。`

两页现有的永久标识字符串保持原样。视口角标在固定对照场景下是 `RGBA8 · 320×240`，在编辑器预览下恢复为今天的 `RGBA8 · 720×480`。

切换动作只重画固定对照场景，各画一帧。不启动 `requestAnimationFrame` 循环。从原生切回「参考实现」时，编辑器恢复可编辑，「编译」和「运行」恢复可用，画布回到 320×240 的 CPU 固定场景，而不是回到上一次 720×480 预览。

参考模式下，点击画布仍走现有 CPU trace。原生模式下，点击不进入 CPU trace。

ST/MT 加载逻辑不变。对照命令打开的 URL 不带查询串。用户手工打开 `/?threads=1` 时，参考实现仍是当时启动的那份 CPU 包；本设计不把 MT 纳入像素门。

### 5.2 场景契约

数字只写在 `web/compare-scene.js`。页面、两个原生模块和 Playwright 都导入它。Wasm 不内嵌第二份对照常量；JS 在调用时传入。

Rust 里已有的 `P0_MILESTONE_NDC`、`P0_MILESTONE_COLOR`、`P0_MILESTONE_CLEAR` 继续服务学习轨道的单元测试。它们不是对照场景的可编辑副本。对照场景的字面量以 JS 模块为准，并且必须等于下面这组数。

| 字段 | 值 |
| --- | --- |
| 分辨率 | 320×240 |
| 清屏色 | RGBA8 `(13, 18, 30, 255)` |
| 顶点 0 | NDC `(0.0, 0.75)`，clip z `0`，clip w `1`，颜色 `(1.0, 0.2, 0.2, 1.0)` |
| 顶点 1 | NDC `(-0.75, -0.65)`，clip z `0`，clip w `1`，颜色 `(0.2, 1.0, 0.3, 1.0)` |
| 顶点 2 | NDC `(0.75, -0.65)`，clip z `0`，clip w `1`，颜色 `(0.2, 0.45, 1.0, 1.0)` |

这组 NDC 与颜色就是现有 P0 三角形。对照帧把 clip z 固定为 0、clip w 固定为 1，使 NDC xy 就是位置，插值不带透视。WebGPU 编辑器示例里的 `z = 0.5` 只属于 720×480 编辑器预览，不用于对照帧。

在 320×240、w = 1 时，两套现有 CPU 投影把三个顶点放到同一组连续存储坐标（原点在图像左上，y 向下）：

| 顶点 | 连续存储坐标 |
| --- | --- |
| 0 | `(160, 30)` |
| 1 | `(40, 198)` |
| 2 | `(280, 198)` |

这些坐标是顶点位置，不是“该像素一定被三角形覆盖”的断言。朝向契约是：顶点 0 靠近图像上方。原生模块若默认视口把画面上下颠倒，只在该原生模块内改正 Y，直到与本页 CPU 参考帧一致。

画布缓冲是 320×240。CSS 宽度 320px、高度 240px。页面不得用 `devicePixelRatio` 放大缓冲，不得对这块画布设置 `transform` 或 `zoom`。Playwright 的 `deviceScaleFactor` 为 1。

未被三角形覆盖的像素，读回必须是 `(13, 18, 30, 255)`。原生颜色附件是 8-unorm。WebGPU 使用 `navigator.gpu.getPreferredCanvasFormat()` 的返回值，且该值必须是 `rgba8unorm` 或 `bgra8unorm`；其它格式按第 7.4 节的 WebGPU 句子失败。WebGL2 使用默认 RGBA8 绘图缓冲。不用 `rgba16float`。比较读取的是画布位图里的 RGBA 字节，不是 GPU 纹理的存储通道顺序。

单帧、无动画、无抗锯齿。WebGL2 上下文属性固定为：

```text
antialias: false
alpha: false
depth: false
stencil: false
premultipliedAlpha: false
preserveDrawingBuffer: true
desynchronized: false
```

WebGPU 不创建 MSAA 纹理，渲染通道没有 `resolveTarget`，canvas `alphaMode` 为 `"opaque"`。绘制后等待 `queue.onSubmittedWorkDone()` 再把状态标为 ready。WebGL2 在 `finish()` 之后再标 ready。

### 5.3 参考帧导出

两个 Wasm 包各增加一个导出：

```text
compare_frame_rgba8(width, height, clear_rgba4, vertices24) -> Uint8Array
```

`vertices24` 是 24 个有限 f32：每个顶点按 `ndc_x, ndc_y, clip_z, clip_w, r, g, b, a` 排列，共三个顶点。`width` 或 `height` 不在 `1..=4096`，或顶点不是 24 个有限数时，导出返回错误，不画半帧。页面对照只传入场景模块里的 320×240 与那 24 个数。

WebGPU 页的导出使用现有 CPU 路径：清屏字节直接写入，屏幕坐标使用今天 `preview_vertices` 的映射

```text
x = (ndc_x * 0.5 + 0.5) * width
y = (0.5 - ndc_y * 0.5) * height
```

顶点颜色在进入 `glgpu-raster` 之前按现有 `color_channel` 量化：`round(clamp(c, 0, 1) * 255)`。深度使用 `clip_z`。这条导出不执行 WGSL。

WebGL 页的导出使用现有 CPU 路径：`draw_clip_space_triangle` 或与 `p0_milestone_triangle` 相同的投影和存储翻转，颜色走现有 `quantize_unorm`（同一舍入公式）。不调用浏览器 WebGL。这条导出不把学习子集的标量 ESSL 当作能在 GPU 上编译的着色器。

`「运行」`不调用 `compare_frame_rgba8`。它保持今天的 720×480 编辑器预览。

### 5.4 原生模块

`web/native-webgpu.js`：

- 导入 `web/compare-scene.js`；
- `navigator.gpu` 缺失，或 `requestAdapter({ powerPreference: "high-performance" })` 得到 null，或 `requestDevice` 失败时，把槽标为 error，文案用第 7.4 节的 WebGPU 句子；
- 在槽里新建画布并配置 WebGPU 上下文；
- WGSL 与 `examples/triangle/p0_triangle.wgsl` 等价：顶点色插值，alpha 为 1，位置来自场景常量而不是写死的第二份数字，clip z/w 使用场景里的 0 和 1；
- 画一帧三角形，无 MSAA，然后等待提交完成。

`web/native-webgl.js`：

- 导入同一场景模块；
- 按第 5.2 节的属性创建 `webgl2` 上下文，失败时把槽标为 error，文案用第 7.4 节的 WebGL 句子；
- ESSL `#version 300 es` 使用真实的 `vec4` 与 `gl_Position`，顶点位置和颜色来自场景常量。这是浏览器着色器，不是 `glgpu-webgl` 的标量子集，也不声称学习栈已经实现了 `gl_Position`；
- 画一帧，`finish()` 后标 ready。

两个模块都不读取编辑器文本，都不写 `stable_hash64`。

### 5.5 `xtask e2e-compare`

`xtask` 的用法字符串增加 `e2e-compare`。命令接受可选的 `--bind <address>`，默认 `127.0.0.1:8765`。不接受其它参数。

步骤固定为：

1. 在仓库根目录执行现有的 ST 构建：`cargo xtask build-web --release`。
2. 执行现有的 WebGL 包构建：`cargo xtask build-web --release --crate crates/glgpu-webgl-web --out-dir web/pkg-webgl --out-name glgpu_webgl_web`。
3. 不执行 `build-web-mt`。
4. 若 `e2e/node_modules` 不存在，或 `e2e/package-lock.json` 比已安装依赖新，则在 `e2e/` 执行 `npm ci`。
5. 启动子进程 `cargo xtask serve --bind <address>`。`GET /` 返回 200 后再跑测试。绑定失败则打印 xtask 的错误并退出非零，不继续测试。
6. 用 Playwright 启动本机 Google Chrome（`channel: "chrome"`），`headless: true`，`deviceScaleFactor: 1`，并始终传入启动参数 `--enable-unsafe-webgpu`。不下载 Playwright 自带的 Chromium 作为通过路径，不添加 SwiftShader 或软件光栅标志。
7. 依次打开 `http://<bind>/` 和 `http://<bind>/webgl.html`。不附加查询串。
8. 无论成功或失败，都结束 serve 子进程。

`e2e/package.json` 为 `"type": "module"`，依赖 `playwright` 和 PNG 编码所需的 `pngjs`。两个版本都由提交进仓库的 `e2e/package-lock.json` 钉死，不使用浮动 tag。

`cargo test -p xtask` 以及现有 `pages` workflow 不得启动 Chrome，不得占用 8765，也不得要求本机安装 npm。xtask 单元测试必须覆盖两件事：`e2e-compare` 是已知命令；未知参数被拒绝。像素比较只在 `cargo xtask e2e-compare` 里发生。本设计不修改 workflow 文件；上述约束是为了让现有 CI 保持绿色。

### 5.6 采样与比较

每一页：

1. 等待槽同时满足 `data-compare-state="reference"` 与 `data-compare-ready="true"`。超时 30 秒则失败。
2. 用同一条读回路径采样参考画布。
3. 点击「原生」。
4. 等待 `data-compare-state="native"` 与 `data-compare-ready="true"`。若变为 `error`，以 `data-compare-error` 的原文失败。
5. 用同一条读回路径采样原生画布。
6. 两个缓冲的长度都必须是 307200。按字节比较。

读回路径：测试代码不在原生画布上调用 `getContext("2d")`。它新建一块 320×240 的 Canvas 2D，`imageSmoothingEnabled = false`，`drawImage` 源画布，再 `getImageData(0, 0, 320, 240)`。参考画布再额外用其自身 2D 上下文的 `getImageData` 读一次；这两次参考读回必须相同，否则失败。主 oracle 仍是这条 `drawImage` + `getImageData` 路径上的两帧，这样参考与原生经过同一种画布读回。不用整页截图，也不用 `gl.readPixels` 或 `readback_rgba8` 作为通过条件。

失败时在退出前写出：

```text
target/e2e-compare/webgpu/reference.png
target/e2e-compare/webgpu/native.png
target/e2e-compare/webgpu/diff.png
target/e2e-compare/webgpu/mismatch.txt
target/e2e-compare/webgl/reference.png
target/e2e-compare/webgl/native.png
target/e2e-compare/webgl/diff.png
target/e2e-compare/webgl/mismatch.txt
```

`/` 对应 `webgpu`，`/webgl.html` 对应 `webgl`。diff 与参考同尺寸。四字节都相同的像素复制参考 RGBA；任一字节不同的像素写成 `(255, 0, 255, 255)`。`mismatch.txt` 包含总差异像素数，以及按行优先（y 自上而下，x 自左而右）的第一处差异：`x`、`y`、参考 RGBA、原生 RGBA。`target/` 已被 `.gitignore` 忽略，这些文件不入库。

一页失败仍然继续采样另一页，然后以非零状态退出，以便一次运行留下两页的 diff。任一一页失败，整条命令失败。

### 5.7 验收文档要补的段落

实现时在 `docs/testing-and-acceptance.zh-CN.md` 增加一节「本地原生像素对照」，写明下列事实，不改 L1–L8 的通过条件：

- 命令是 `cargo xtask e2e-compare`，只在本地运行；
- 通过条件是两页各自的参考画布与原生画布精确 RGBA 相等，无容差；
- 缺少 Google Chrome、`navigator.gpu` 或 WebGL2 时命令失败，不是 skip；
- 这条门不替代 `stable_hash64`，也不进入 GitHub Actions；
- 720×480 的首帧 hash `2898a668a856be56` 仍是编辑器预览场景的 ST/MT 证据。加载页面后的第一帧改为 320×240 固定对照场景，那个 hash 不再是加载瞬间的 hash；
- 固定对照场景的角标是 `RGBA8 · 320×240`；点击「运行」后的编辑器预览角标仍是 `RGBA8 · 720×480`；
- 失败产物在 `target/e2e-compare/`。

不改其它文档的通过/失败表，除非某一句把“加载后的第一帧”写成了 720×480 hash。那种句子改成上一款的区分，避免文档自相矛盾。

## 6. 成功标准

在一台装有 Google Chrome、并且 Chrome 能创建 WebGPU device 和 WebGL2 上下文的本机上：

1. `cargo xtask e2e-compare` 退出码为 0。第 7.2 节的量化差不是豁免；第 4.2 节的三步用完之后仍然不等，这一条就不成立。
2. `/` 与 `/webgl.html` 都满足 307200 字节全等。
3. 人为改坏任一原生着色器后，同一命令非零退出，并在对应目录留下 diff PNG 与 `mismatch.txt`。
4. 去掉 WebGPU 或 WebGL2 能力时，命令非零退出，输出包含第 7.4 节的对应句子。
5. 页面默认显示「参考实现」，切换后同一位置显示原生帧，无第二块并排画布。
6. 原生模式下编辑器只读，且「编译」「运行」不可用。
7. 两页永久标识仍然是 `NOT WEBGPU CONFORMANT` 与 `NOT WEBGL CONFORMANT`。
8. `cargo test --workspace` 里现有的 `stable_hash64`、framebuffer 全等和 history 全等测试仍在，比较方式没有放宽。
9. `.github/workflows/pages.yml` 无 diff。

## 7. 风险

### 7.1 驱动与边缘覆盖

CPU WebGPU 路径使用 8.8 定点边方程和 top-left 规则。CPU WebGL 路径使用采样点重心坐标。原生 GPU 的边归属由实现定义。无 MSAA、固定分辨率和单帧只能去掉动画与抗锯齿，不能保证所有驱动对同一条边给出同一覆盖。

处理方式只按第 4.2 节：命令失败并写出 diff，再按该节的三步修。不放宽比较。

### 7.2 颜色量化

顶点颜色先经过 `round(clamp * 255)` 再在 CPU 里插值。GPU 在更高精度插值，写入 8-unorm 时再量化。这两次舍入不对易，覆盖像素会相差 1。清屏色以浮点 `13/255`、`18/255`、`30/255` 清除时，读回字节也会偏离 `(13, 18, 30, 255)`。契约不接受这种偏差：未覆盖像素必须是这四个字节，覆盖像素必须与参考画布逐字节相同。修法只按第 4.2 节，修不了就保持失败。

### 7.3 画布读回

WebGL 画布经过 `drawImage` 时，预乘 alpha 会改写 RGB。上下文已经关掉 alpha 与预乘，清屏 alpha 的字节是 255。浏览器若仍改写字节，这是失败，按第 4.2 节第 1 步修上下文属性。不改为“忽略 alpha”或“每通道 ±1”。

WebGPU 首选画布格式在一些平台上是 `bgra8unorm`。oracle 读画布位图里的 RGBA 顺序。通道颠倒视为原生呈现错误，命令失败。

### 7.4 缺少原生能力

没有软跳过。失败输出使用这些句子，且整句出现：

- Chrome 通道启动失败：`原生对照需要本机已安装的 Google Chrome。Playwright 的 chrome 通道启动失败，命令失败。`
- WebGPU 页没有 `navigator.gpu` 或 adapter/device：`原生对照需要本机 Google Chrome 的 WebGPU（navigator.gpu）。当前环境没有可用的 WebGPU，命令失败。`
- WebGL 页不能创建 WebGL2：`原生对照需要本机 Google Chrome 的 WebGL2。当前环境不能创建 WebGL2 上下文，命令失败。`

Headless Chrome 若没有真实 GPU，同样走这些失败句子。不用软件光栅凑一个通过结果。

### 7.5 与现有调试页的行为差

加载后的第一帧从 720×480 编辑器场景变为 320×240 固定对照场景。720×480 预览仍由「运行」触发。第 5.7 节要求验收文档把这两种画面分开写，避免把旧的加载 hash 当成对照门。

## 8. 后续事项

这些不在本切片：

1. 将来若要把同一条命令放进 CI，需要另一次决定，并使用带 GPU 的 runner。通过条件仍是精确 RGBA。没有 GPU 时仍然失败，不改为 skip。本切片不添加 workflow。
2. 多于一个三角形的对照场景不在本切片。本切片只有第 5.2 节这一帧。
3. 编辑器源码驱动原生着色器不在本切片。原生着色器只消费 `web/compare-scene.js`。
4. 在 `e2e-compare` 里再比一次 ST 与 MT 不在本切片。ST/MT 仍由现有 hash 门负责。

## 9. 实现时要落地的文件

本说明的实现补丁应包含：

- `web/compare-scene.js`
- `web/native-webgpu.js`
- `web/native-webgl.js`
- `web/index.html` 与 `web/webgl.html` 对上述模块的接入
- `crates/glgpu-web` 与 `crates/glgpu-webgl-web` 的切换控件、`compare_frame_rgba8`，以及参考模式画布
- `xtask` 的 `e2e-compare`
- `e2e/package.json`、`e2e/package-lock.json`、`e2e/compare.spec.mjs`
- `docs/testing-and-acceptance.zh-CN.md` 的第 5.7 节段落

不包含 workflow 修改，不包含入库的 PNG。

## 10. 自检

- 没有未决的占位结论。分辨率、清屏色、初始顶点、命令、端口、读回路径、失败产物和错误句子都已写死。NDC xy 只在第 4.2 节第 3 步、并且与本说明同时修订时才允许改变。
- 精确 RGBA 是唯一的像素通过条件，与“失败后写 diff、不放宽”一致。
- WebGPU 与 WebGL 两页都在范围内，并且不要求两页互相相等。
- 命令只在本地运行，不改 CI。
- 原生绘制是 oracle，不改变两个 NOT CONFORMANT 契约，也不进入 CPU crate。
- L1–L8 与 `stable_hash64` 保留，且不改成近似比较。
