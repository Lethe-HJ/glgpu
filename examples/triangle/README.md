# P0 Triangle 示例

[`p0_triangle.wgsl`](p0_triangle.wgsl) 是当前浏览器 demo 使用的 WGSL 校验样例，包含一个 `@vertex vs_main` 和一个 `@fragment fs_main`。

## 当前行为

启动页面时，`glgpu-web` 会把该源码传给 `CpuDevice::create_shader_module`：

1. Naga 解析 WGSL；
2. 使用 P0 optional-capability allowlist 做核心语义校验；
3. 提取两个 entry point 的名称、stage 和 workgroup metadata；
4. UI 显示 `WGSL core syntax/semantics only: 2 entry points`。

P0 **不会执行这段 WGSL**。当前 CPU triangle 使用 `crates/glgpu-web/src/lib.rs` 中手工构造的等价 screen-space position、depth 和 RGBA8 vertex color。示例存在的目的，是固定 shader frontend/diagnostic 边界，而不是证明已有 shader VM。

## 验证

```bash
cargo test -p glgpu-shader
cargo test -p glgpu-webgpu
cargo check -p glgpu-web --target wasm32-unknown-unknown
```

shader tests 同时覆盖：

- vertex/fragment entry point；
- compute entry point metadata；
- parse error 的 source span；
- semantic error；
- P0 不允许的 optional `f16` capability。

## 下一阶段变化

P1 将把 Naga module lowering 到项目自有 typed IR/bytecode，并让 `vs_main`/`fs_main` 真正在 CPU interpreter 中运行。完成该 gate 后，手工 screen-space 顶点才会从 demo 主路径移除；在此之前不能把该文件称为已执行 shader。

完整 API 边界见[中文 API 文档](../../docs/api-reference.zh-CN.md)，阶段 gate 见[实施路线图](../../docs/roadmap.zh-CN.md)。

