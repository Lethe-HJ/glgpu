//! Debug page for webgpu-mock.
//!
//! This is not native WebGPU. The page contract stays
//! `CpuGpuSubset — NOT WEBGPU CONFORMANT`. The triangle on the page is a demo.
//! `?threads=1` still runs the P0 subset on the main owner thread. Rayon
//! workers in that package only run tile kernels, then `putImageData` copies
//! the frame. Without that flag, and when `SharedArrayBuffer` exists, the
//! triangle is submitted in `glgpu-webgpu-worker`. Packed color is written into
//! the back slot; this page's JavaScript presents the front slot. Depth stays
//! in the worker device framebuffer. There is no OffscreenCanvas blit. Without
//! a shared buffer the same submit stays on the main thread.

#![forbid(unsafe_code)]

#[cfg(any(target_arch = "wasm32", test))]
mod editor_support {
    use std::fmt::Write as _;

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct TrianglePreviewInputs {
        pub positions: [[f32; 2]; 3],
        pub colors: [[f32; 3]; 3],
    }

    const KEYWORDS: &[&str] = &[
        "alias",
        "break",
        "case",
        "const",
        "const_assert",
        "continue",
        "continuing",
        "default",
        "diagnostic",
        "discard",
        "else",
        "enable",
        "false",
        "fn",
        "for",
        "if",
        "let",
        "loop",
        "override",
        "requires",
        "return",
        "struct",
        "switch",
        "true",
        "var",
        "while",
    ];

    const TYPES_AND_BUILTINS: &[&str] = &[
        "array",
        "atomic",
        "bitcast",
        "bool",
        "f16",
        "f32",
        "i32",
        "mat2x2f",
        "mat2x3f",
        "mat2x4f",
        "mat3x2f",
        "mat3x3f",
        "mat3x4f",
        "mat4x2f",
        "mat4x3f",
        "mat4x4f",
        "ptr",
        "sampler",
        "sampler_comparison",
        "texture_1d",
        "texture_2d",
        "texture_2d_array",
        "texture_3d",
        "texture_cube",
        "texture_cube_array",
        "texture_depth_2d",
        "texture_depth_2d_array",
        "texture_depth_cube",
        "texture_depth_cube_array",
        "texture_external",
        "texture_multisampled_2d",
        "texture_storage_1d",
        "texture_storage_2d",
        "texture_storage_2d_array",
        "texture_storage_3d",
        "u32",
        "vec2",
        "vec2f",
        "vec2h",
        "vec2i",
        "vec2u",
        "vec3",
        "vec3f",
        "vec3h",
        "vec3i",
        "vec3u",
        "vec4",
        "vec4f",
        "vec4h",
        "vec4i",
        "vec4u",
    ];

    /// HTML for the highlight `<pre>`.
    ///
    /// A textarea paints a line after a trailing newline. A `<pre>` does not,
    /// so without one extra newline the overlay is one line short and cannot
    /// scroll with the caret.
    pub fn highlight_overlay_html(source: &str) -> String {
        let mut html = highlight_wgsl(source);
        html.push('\n');
        html
    }

    pub fn highlight_wgsl(source: &str) -> String {
        let mut output = String::with_capacity(source.len() + source.len() / 2);
        let bytes = source.as_bytes();
        let mut cursor = 0;

        while cursor < bytes.len() {
            if bytes[cursor..].starts_with(b"//") {
                let start = cursor;
                cursor += 2;
                while cursor < bytes.len() && bytes[cursor] != b'\n' {
                    cursor += char_len_at(source, cursor);
                }
                push_token(&mut output, "tok-comment", &source[start..cursor]);
                continue;
            }

            if bytes[cursor..].starts_with(b"/*") {
                let start = cursor;
                cursor += 2;
                let mut depth = 1_u32;
                while cursor < bytes.len() && depth > 0 {
                    if bytes[cursor..].starts_with(b"/*") {
                        depth += 1;
                        cursor += 2;
                    } else if bytes[cursor..].starts_with(b"*/") {
                        depth -= 1;
                        cursor += 2;
                    } else {
                        cursor += char_len_at(source, cursor);
                    }
                }
                push_token(&mut output, "tok-comment", &source[start..cursor]);
                continue;
            }

            if bytes[cursor] == b'@'
                && cursor + 1 < bytes.len()
                && is_identifier_start(bytes[cursor + 1])
            {
                let start = cursor;
                cursor += 2;
                while cursor < bytes.len() && is_identifier_continue(bytes[cursor]) {
                    cursor += 1;
                }
                push_token(&mut output, "tok-attribute", &source[start..cursor]);
                continue;
            }

            if bytes[cursor].is_ascii_digit()
                || (bytes[cursor] == b'.'
                    && cursor + 1 < bytes.len()
                    && bytes[cursor + 1].is_ascii_digit())
            {
                let start = cursor;
                cursor = consume_number(bytes, cursor);
                push_token(&mut output, "tok-number", &source[start..cursor]);
                continue;
            }

            if is_identifier_start(bytes[cursor]) {
                let start = cursor;
                cursor += 1;
                while cursor < bytes.len() && is_identifier_continue(bytes[cursor]) {
                    cursor += 1;
                }
                let identifier = &source[start..cursor];
                if KEYWORDS.contains(&identifier) {
                    push_token(&mut output, "tok-keyword", identifier);
                } else if TYPES_AND_BUILTINS.contains(&identifier) {
                    push_token(&mut output, "tok-type", identifier);
                } else if next_non_whitespace_byte(bytes, cursor) == Some(b'(') {
                    push_token(&mut output, "tok-function", identifier);
                } else {
                    push_escaped(&mut output, identifier);
                }
                continue;
            }

            let length = char_len_at(source, cursor);
            push_escaped(&mut output, &source[cursor..cursor + length]);
            cursor += length;
        }

        output
    }

    pub fn line_numbers_html(source: &str, error_line: Option<u32>) -> String {
        let line_count = source.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let mut output = String::with_capacity(line_count * 32);
        for line in 1..=line_count {
            let class_name = if error_line == Some(line as u32) {
                "line-number is-error"
            } else {
                "line-number"
            };
            let _ = write!(output, "<span class=\"{class_name}\">{line}</span>");
            if line < line_count {
                output.push('\n');
            }
        }
        output
    }

    pub fn utf8_span_to_utf16_range(source: &str, offset: u32, length: u32) -> (u32, u32) {
        let start = clamp_to_char_boundary(source, offset as usize);
        let requested_end = (offset as usize).saturating_add(length as usize);
        let end = clamp_to_char_boundary(source, requested_end).max(start);
        (
            source[..start].encode_utf16().count() as u32,
            source[..end].encode_utf16().count() as u32,
        )
    }

    pub fn triangle_preview_inputs(source: &str) -> Result<TrianglePreviewInputs, String> {
        let positions = parse_vector_binding::<2>(source, "positions", "vec2f")?;
        let colors = parse_vector_binding::<3>(source, "colors", "vec3f")?;
        Ok(TrianglePreviewInputs { positions, colors })
    }

    fn push_token(output: &mut String, class_name: &str, text: &str) {
        let _ = write!(output, "<span class=\"{class_name}\">");
        push_escaped(output, text);
        output.push_str("</span>");
    }

    fn push_escaped(output: &mut String, text: &str) {
        for character in text.chars() {
            match character {
                '&' => output.push_str("&amp;"),
                '<' => output.push_str("&lt;"),
                '>' => output.push_str("&gt;"),
                '"' => output.push_str("&quot;"),
                '\'' => output.push_str("&#39;"),
                _ => output.push(character),
            }
        }
    }

    fn consume_number(bytes: &[u8], mut cursor: usize) -> usize {
        let mut previous = None;
        while cursor < bytes.len() {
            let byte = bytes[cursor];
            let is_sign =
                matches!(byte, b'+' | b'-') && matches!(previous, Some(b'e' | b'E' | b'p' | b'P'));
            if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_') || is_sign {
                previous = Some(byte);
                cursor += 1;
            } else {
                break;
            }
        }
        cursor
    }

    fn next_non_whitespace_byte(bytes: &[u8], mut cursor: usize) -> Option<u8> {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        bytes.get(cursor).copied()
    }

    fn is_identifier_start(byte: u8) -> bool {
        byte == b'_' || byte.is_ascii_alphabetic()
    }

    fn is_identifier_continue(byte: u8) -> bool {
        is_identifier_start(byte) || byte.is_ascii_digit()
    }

    fn char_len_at(source: &str, cursor: usize) -> usize {
        source[cursor..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(1)
    }

    fn clamp_to_char_boundary(source: &str, requested: usize) -> usize {
        let mut cursor = requested.min(source.len());
        while cursor > 0 && !source.is_char_boundary(cursor) {
            cursor -= 1;
        }
        cursor
    }

    fn parse_vector_binding<const COMPONENTS: usize>(
        source: &str,
        binding: &str,
        constructor: &str,
    ) -> Result<[[f32; COMPONENTS]; 3], String> {
        let binding_start = source
            .find(binding)
            .ok_or_else(|| format!("找不到 `{binding}` 数组"))?;
        let source_after_binding = &source[binding_start + binding.len()..];
        let array_start = source_after_binding
            .find("array")
            .ok_or_else(|| format!("`{binding}` 必须使用 array(...) 初始化"))?;
        let array_source = &source_after_binding[array_start..];
        let open = array_source
            .find('(')
            .ok_or_else(|| format!("`{binding}` 的 array 缺少左括号"))?;
        let close = matching_parenthesis(array_source, open)
            .ok_or_else(|| format!("`{binding}` 的 array 括号不完整"))?;
        let body = &array_source[open + 1..close];

        let mut values = Vec::with_capacity(3);
        let marker = format!("{constructor}(");
        let mut cursor = 0;
        while values.len() < 3 {
            let relative = body[cursor..]
                .find(&marker)
                .ok_or_else(|| format!("`{binding}` 需要 3 个 {constructor}(...) 常量"))?;
            let constructor_start = cursor + relative;
            let open = constructor_start + constructor.len();
            let close = matching_parenthesis(body, open)
                .ok_or_else(|| format!("`{binding}` 中的 {constructor} 括号不完整"))?;
            let arguments = &body[open + 1..close];
            let components = arguments
                .split(',')
                .map(parse_decimal_literal)
                .collect::<Result<Vec<_>, _>>()?;
            if components.len() != COMPONENTS {
                return Err(format!(
                    "`{binding}` 中的 {constructor} 需要 {COMPONENTS} 个数字"
                ));
            }
            let value: [f32; COMPONENTS] = components
                .try_into()
                .map_err(|_| format!("无法读取 `{binding}` 中的 {constructor}"))?;
            values.push(value);
            cursor = close + 1;
        }

        values
            .try_into()
            .map_err(|_| format!("`{binding}` 需要恰好 3 个预览顶点"))
    }

    fn matching_parenthesis(source: &str, open: usize) -> Option<usize> {
        if source.as_bytes().get(open) != Some(&b'(') {
            return None;
        }
        let mut depth = 0_u32;
        for (relative, byte) in source.as_bytes()[open..].iter().copied().enumerate() {
            match byte {
                b'(' => depth += 1,
                b')' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(open + relative);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn parse_decimal_literal(source: &str) -> Result<f32, String> {
        let source = source.trim();
        let source = source
            .strip_suffix('f')
            .or_else(|| source.strip_suffix('h'))
            .unwrap_or(source);
        let value = source
            .parse::<f32>()
            .map_err(|_| format!("CPU 预览暂时只支持十进制常量，无法读取 `{source}`"))?;
        if value.is_finite() {
            Ok(value)
        } else {
            Err(format!("CPU 预览不接受非有限数字 `{source}`"))
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod app {
    use std::cell::RefCell;
    use std::fmt::Write as _;
    use std::rc::Rc;

    use super::editor_support::{
        TrianglePreviewInputs, highlight_overlay_html, line_numbers_html, triangle_preview_inputs,
        utf8_span_to_utf16_range,
    };
    use glgpu_canvas2d::present_rgba8;
    use glgpu_core::{Extent2d, PixelCoord, Rgba8, ScreenVertex, Triangle};
    use glgpu_webgpu::{CpuDevice, CpuGpuSubset, DeviceDescriptor};
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::prelude::*;
    use web_sys::{
        CanvasRenderingContext2d, Document, Element, HtmlButtonElement, HtmlCanvasElement,
        HtmlElement, HtmlTextAreaElement, KeyboardEvent, MouseEvent, Storage,
    };

    const WIDTH: u32 = 720;
    const HEIGHT: u32 = 480;
    const MAX_WGSL_BYTES: usize = 256 * 1024;
    const SAVED_WGSL_KEY: &str = "glgpu.wgsl.editor.v1";
    const DEMO_WGSL: &str = include_str!("../../../examples/triangle/p0_triangle.wgsl");

    struct DemoState {
        device: CpuDevice,
        context: CanvasRenderingContext2d,
        canvas: HtmlCanvasElement,
        metrics: HtmlElement,
        pixel_history: HtmlElement,
        editor: HtmlTextAreaElement,
        highlight: HtmlElement,
        line_numbers: HtmlElement,
        compile_status: HtmlElement,
        diagnostic_title: HtmlElement,
        diagnostic_detail: HtmlElement,
        diagnostic_panel: HtmlElement,
        save_status: HtmlElement,
        storage: Option<Storage>,
        saved_source: String,
        has_saved_source: bool,
        preview: TrianglePreviewInputs,
        selected: PixelCoord,
        parallel: bool,
        wgsl_summary: String,
    }

    impl DemoState {
        fn render(&mut self, selected: PixelCoord) -> Result<(), JsValue> {
            if self.queue_worker(selected) {
                self.selected = selected;
                self.metrics.set_text_content(Some(
                    "worker raster · triangle command is in the shared buffer",
                ));
                return Ok(());
            }
            let started = js_sys::Date::now();
            let vertices = self.preview_vertices();
            let mut encoder = self.device.create_command_encoder();
            encoder
                .clear(Rgba8::new(13, 18, 30, 255), 1.0)
                .map_err(js_error)?;
            encoder.select_pixel(selected).map_err(js_error)?;
            encoder
                .draw_triangle(Triangle::from_vertices(
                    1,
                    vertices[0],
                    vertices[1],
                    vertices[2],
                ))
                .map_err(js_error)?;

            let command_buffer = encoder.finish().map_err(js_error)?;
            let report = self
                .device
                .queue_mut()
                .submit(command_buffer)
                .map_err(js_error)?;
            let pixels = self.device.queue().readback_rgba8().map_err(js_error)?;
            present_rgba8(&self.context, WIDTH, HEIGHT, &pixels).map_err(js_error)?;

            let elapsed_ms = js_sys::Date::now() - started;
            self.metrics.set_text_content(Some(&format!(
                "submit #{} · {:.2} ms · hash {:016x} · {} · {}",
                report.serial,
                elapsed_ms,
                report.stable_hash64,
                if self.parallel {
                    "Rayon tile executor"
                } else {
                    "sequential tile executor"
                },
                self.wgsl_summary,
            )));

            let history = self
                .device
                .queue()
                .selected_pixel_history()
                .ok_or_else(|| JsValue::from_str("selected-pixel history was not produced"))?;
            let mut text = format!(
                "pixel ({}, {})\nfinal rgba({}, {}, {}, {}) · depth {:.6}\n",
                history.pixel.x,
                history.pixel.y,
                history.final_color.r,
                history.final_color.g,
                history.final_color.b,
                history.final_color.a,
                history.final_depth,
            );
            if history.events.is_empty() {
                text.push_str("no triangle reached this pixel");
            } else {
                for event in &history.events {
                    let _ = writeln!(
                        text,
                        "primitive {} · {:?} · candidate depth {:?} · rgba({}, {}, {}, {})",
                        event.primitive_id,
                        event.outcome,
                        event.candidate_depth,
                        event.color_after.r,
                        event.color_after.g,
                        event.color_after.b,
                        event.color_after.a,
                    );
                }
            }
            self.pixel_history.set_text_content(Some(&text));
            self.selected = selected;
            Ok(())
        }

        fn queue_worker(&self, selected: PixelCoord) -> bool {
            let Some(window) = web_sys::window() else {
                return false;
            };
            let Ok(value) =
                js_sys::Reflect::get(window.as_ref(), &JsValue::from_str("glgpuQueueTriangle"))
            else {
                return false;
            };
            let Ok(func) = value.dyn_into::<js_sys::Function>() else {
                return false;
            };
            let vertices = self.preview_vertices();
            let mut coords = [0_f32; 21];
            for (index, vertex) in vertices.iter().enumerate() {
                let base = index * 7;
                coords[base] = vertex.x;
                coords[base + 1] = vertex.y;
                coords[base + 2] = vertex.depth;
                coords[base + 3] = f32::from(vertex.color.r);
                coords[base + 4] = f32::from(vertex.color.g);
                coords[base + 5] = f32::from(vertex.color.b);
                coords[base + 6] = f32::from(vertex.color.a);
            }
            let array = js_sys::Float32Array::from(coords.as_slice());
            func.call3(
                &JsValue::NULL,
                &array,
                &JsValue::from_f64(f64::from(selected.x)),
                &JsValue::from_f64(f64::from(selected.y)),
            )
            .ok()
            .and_then(|queued| queued.as_bool())
            .unwrap_or(false)
        }

        fn apply_worker_frame(&mut self, serial: f64, hash: &str, history: &str) {
            self.metrics.set_text_content(Some(&format!(
                "submit #{serial:.0} · worker front slot · hash {hash} · sequential worker · {}",
                self.wgsl_summary,
            )));
            self.pixel_history.set_text_content(Some(history));
        }

        fn preview_vertices(&self) -> [ScreenVertex; 3] {
            std::array::from_fn(|index| {
                let [x, y] = self.preview.positions[index];
                let [r, g, b] = self.preview.colors[index];
                ScreenVertex::new(
                    (x * 0.5 + 0.5) * WIDTH as f32,
                    (0.5 - y * 0.5) * HEIGHT as f32,
                    0.5,
                    Rgba8::new(color_channel(r), color_channel(g), color_channel(b), 255),
                )
            })
        }

        fn refresh_editor(&self) {
            let source = self.editor.value();
            self.highlight
                .set_inner_html(&highlight_overlay_html(&source));
            self.line_numbers
                .set_inner_html(&line_numbers_html(&source, None));
            self.editor.set_attribute("aria-invalid", "false").ok();
            self.compile_status
                .set_class_name("compile-status status-dirty");
            self.compile_status.set_text_content(Some("未编译"));
            self.diagnostic_panel
                .set_class_name("diagnostics diagnostics-idle");
            self.diagnostic_title
                .set_text_content(Some("代码已修改，等待编译"));
            self.diagnostic_detail.set_text_content(Some(
                "点击“编译”只进行 WGSL 语法与语义校验；点击“运行”会读取示例中的 positions/colors 并重绘 CPU 预览。",
            ));
            self.update_save_status();
            self.sync_editor_scroll();
        }

        fn compile_editor(&mut self, run_after_compile: bool) -> Result<bool, JsValue> {
            let source = self.editor.value();
            if source.len() > MAX_WGSL_BYTES {
                self.show_limit_error(source.len());
                return Ok(false);
            }

            let compiled = self.device.create_shader_module(source.clone());
            let shader = match compiled {
                Ok(shader) => shader,
                Err(error) => {
                    let error_line = error.span.map(|span| span.line);
                    self.line_numbers
                        .set_inner_html(&line_numbers_html(&source, error_line));
                    self.compile_status
                        .set_class_name("compile-status status-error");
                    self.compile_status.set_text_content(Some("编译失败"));
                    self.diagnostic_panel
                        .set_class_name("diagnostics diagnostics-error");
                    let location = error
                        .span
                        .map(|span| format!(" · 第 {} 行，第 {} 列", span.line, span.column))
                        .unwrap_or_default();
                    self.diagnostic_title.set_text_content(Some(&format!(
                        "{:?} error{location} · 未运行",
                        error.kind
                    )));
                    self.diagnostic_detail
                        .set_text_content(Some(&error.source_message));
                    self.editor.set_attribute("aria-invalid", "true")?;
                    if let Some(span) = error.span {
                        let (start, end) =
                            utf8_span_to_utf16_range(&source, span.offset, span.length.max(1));
                        self.editor.focus()?;
                        self.editor.set_selection_range(start, end)?;
                    }
                    self.sync_editor_scroll();
                    return Ok(false);
                }
            };

            let preview = if run_after_compile {
                match triangle_preview_inputs(&source) {
                    Ok(preview) => Some(preview),
                    Err(message) => {
                        self.show_preview_error(&message);
                        return Ok(false);
                    }
                }
            } else {
                None
            };

            let mut entry_points = Vec::with_capacity(shader.info().entry_points.len());
            for entry in &shader.info().entry_points {
                entry_points.push(format!(
                    "{} {}",
                    format!("{:?}", entry.stage).to_lowercase(),
                    entry.name
                ));
            }
            self.wgsl_summary = format!(
                "WGSL validated: {} entry points",
                shader.info().entry_points.len()
            );
            self.editor.set_attribute("aria-invalid", "false")?;
            self.line_numbers
                .set_inner_html(&line_numbers_html(&source, None));
            self.compile_status
                .set_class_name("compile-status status-ok");
            self.compile_status
                .set_text_content(Some(if run_after_compile {
                    "预览已运行"
                } else {
                    "编译通过"
                }));
            self.diagnostic_panel
                .set_class_name("diagnostics diagnostics-ok");
            self.diagnostic_title.set_text_content(Some(&format!(
                "WGSL 编译通过 · {} 个入口点",
                shader.info().entry_points.len()
            )));
            let entries = if entry_points.is_empty() {
                "未发现入口点".to_owned()
            } else {
                entry_points.join(" · ")
            };
            let detail = if run_after_compile {
                format!(
                    "{entries}\n已读取 positions/colors 常量并重绘 CPU 三角形；其他 WGSL 控制流尚未由 P0 执行。"
                )
            } else {
                format!("{entries}\n仅完成校验；编译操作不会提交新帧。")
            };
            self.diagnostic_detail.set_text_content(Some(&detail));

            if let Some(preview) = preview {
                self.preview = preview;
                if let Err(error) = self.render(self.selected) {
                    self.show_runtime_error(&error);
                    return Err(error);
                }
            }
            Ok(true)
        }

        fn save_editor(&mut self) -> Result<bool, JsValue> {
            let source = self.editor.value();
            let Some(storage) = &self.storage else {
                self.save_status.set_class_name("save-status save-error");
                self.save_status.set_text_content(Some("保存不可用"));
                return Ok(false);
            };
            if let Err(error) = storage.set_item(SAVED_WGSL_KEY, &source) {
                self.save_status.set_class_name("save-status save-error");
                self.save_status.set_text_content(Some("保存失败"));
                return Err(error);
            }
            self.saved_source = source;
            self.has_saved_source = true;
            self.update_save_status();
            Ok(true)
        }

        fn update_save_status(&self) {
            if self.editor.value() != self.saved_source {
                self.save_status.set_class_name("save-status save-dirty");
                self.save_status.set_text_content(Some("未保存"));
            } else if self.has_saved_source {
                self.save_status.set_class_name("save-status save-saved");
                self.save_status.set_text_content(Some("已保存到本机"));
            } else {
                self.save_status.set_class_name("save-status");
                self.save_status.set_text_content(Some("内置示例"));
            }
        }

        fn show_preview_error(&self, message: &str) {
            self.compile_status
                .set_class_name("compile-status status-error");
            self.compile_status.set_text_content(Some("预览不支持"));
            self.diagnostic_panel
                .set_class_name("diagnostics diagnostics-error");
            self.diagnostic_title
                .set_text_content(Some("WGSL 编译通过，但 CPU 预览无法运行"));
            self.diagnostic_detail.set_text_content(Some(&format!(
                "{message}\n当前预览支持修改示例中的 3 个 vec2f positions 和 3 个 vec3f colors 十进制常量。"
            )));
        }

        fn show_limit_error(&self, source_bytes: usize) {
            self.compile_status
                .set_class_name("compile-status status-error");
            self.compile_status.set_text_content(Some("源码过大"));
            self.diagnostic_panel
                .set_class_name("diagnostics diagnostics-error");
            self.diagnostic_title
                .set_text_content(Some("编译已停止 · 未运行"));
            self.diagnostic_detail.set_text_content(Some(&format!(
                "源码为 {source_bytes} bytes，当前编辑器上限为 {MAX_WGSL_BYTES} bytes。"
            )));
            self.editor.set_attribute("aria-invalid", "true").ok();
        }

        fn show_runtime_error(&self, error: &JsValue) {
            self.compile_status
                .set_class_name("compile-status status-error");
            self.compile_status.set_text_content(Some("运行失败"));
            self.diagnostic_panel
                .set_class_name("diagnostics diagnostics-error");
            self.diagnostic_title
                .set_text_content(Some("CPU 预览运行失败"));
            self.diagnostic_detail
                .set_text_content(Some(&format!("{error:?}")));
        }

        fn sync_editor_scroll(&self) {
            self.highlight.set_scroll_top(self.editor.scroll_top());
            self.highlight.set_scroll_left(self.editor.scroll_left());
            self.line_numbers.set_scroll_top(self.editor.scroll_top());
        }
    }

    pub fn boot(parallel: bool) -> Result<(), JsValue> {
        console_error_panic_hook::set_once();

        if parallel && !cfg!(feature = "parallel") {
            return Err(JsValue::from_str(
                "parallel=true requires the glgpu-web `parallel` feature",
            ));
        }

        let window = web_sys::window().ok_or_else(|| JsValue::from_str("window is unavailable"))?;
        let storage = window.local_storage().ok().flatten();
        let saved_source = storage
            .as_ref()
            .and_then(|storage| storage.get_item(SAVED_WGSL_KEY).ok().flatten())
            .filter(|source| source.len() <= MAX_WGSL_BYTES);
        let has_saved_source = saved_source.is_some();
        let initial_source = saved_source.unwrap_or_else(|| DEMO_WGSL.to_owned());
        let document = window
            .document()
            .ok_or_else(|| JsValue::from_str("document is unavailable"))?;
        let root = document
            .get_element_by_id("app")
            .ok_or_else(|| JsValue::from_str("#app is missing"))?;
        root.set_inner_html("");
        install_style(&document, &root)?;

        let header = element(&document, "header", "hero")?;
        let eyebrow = element(&document, "div", "eyebrow")?;
        eyebrow.set_text_content(Some("CPU REFERENCE RENDERER · P0"));
        let title = element(&document, "h1", "")?;
        title.set_text_content(Some("GLGPU WebGPU Debug Lab"));
        let subtitle = element(&document, "p", "subtitle")?;
        subtitle.set_text_content(Some(
            "Rust/Wasm executes the supported pipeline on the CPU. Canvas 2D only presents the resulting RGBA8 frame.",
        ));
        let warning = element(&document, "div", "warning")?;
        warning.set_text_content(Some("CpuGpuSubset — NOT WEBGPU CONFORMANT"));
        append(&header, &eyebrow)?;
        append(&header, &title)?;
        append(&header, &subtitle)?;
        append(&header, &warning)?;
        append(&root, &header)?;

        let capability = element(&document, "section", "capabilities")?;
        capability.set_text_content(Some(&capability_text(&window, parallel)));
        append(&root, &capability)?;

        let layout = element(&document, "section", "workspace")?;

        let editor_panel = element(&document, "section", "panel editor-panel")?;
        editor_panel.set_attribute("aria-labelledby", "wgsl-editor-title")?;
        let editor_toolbar = element(&document, "div", "editor-toolbar")?;
        let editor_heading = element(&document, "div", "editor-heading")?;
        let editor_title = element(&document, "h2", "")?;
        editor_title.set_id("wgsl-editor-title");
        editor_title.set_text_content(Some("WGSL Shader Editor"));
        let editor_file = element(&document, "div", "editor-file")?;
        editor_file.set_text_content(Some("p0_triangle.wgsl"));
        let save_status = element(
            &document,
            "span",
            if has_saved_source {
                "save-status save-saved"
            } else {
                "save-status"
            },
        )?;
        save_status.set_attribute("aria-live", "polite")?;
        save_status.set_text_content(Some(if has_saved_source {
            "已从本机恢复"
        } else {
            "内置示例"
        }));
        append(&editor_heading, &editor_title)?;
        append(&editor_heading, &editor_file)?;
        append(&editor_heading, &save_status)?;

        let editor_actions = element(&document, "div", "editor-actions")?;
        let compile_status = element(&document, "span", "compile-status status-idle")?;
        compile_status.set_text_content(Some("准备中"));
        let save_button: HtmlButtonElement = document.create_element("button")?.dyn_into()?;
        save_button.set_class_name("button button-quiet");
        save_button.set_text_content(Some("保存"));
        save_button.set_attribute("title", "保存到当前浏览器（Ctrl/⌘ + S）")?;
        let compile_button: HtmlButtonElement = document.create_element("button")?.dyn_into()?;
        compile_button.set_class_name("button button-secondary");
        compile_button.set_text_content(Some("编译"));
        compile_button.set_attribute("title", "编译 / 校验（Ctrl/⌘ + Shift + Enter）")?;
        let run_button: HtmlButtonElement = document.create_element("button")?.dyn_into()?;
        run_button.set_class_name("button button-primary");
        run_button.set_text_content(Some("运行"));
        run_button.set_attribute("title", "编译并运行 CPU 预览（Ctrl/⌘ + Enter）")?;
        let reset_code: HtmlButtonElement = document.create_element("button")?.dyn_into()?;
        reset_code.set_class_name("button button-quiet");
        reset_code.set_text_content(Some("还原"));
        reset_code.set_attribute("title", "还原示例 WGSL")?;
        append(&editor_actions, &compile_status)?;
        append(&editor_actions, &save_button)?;
        append(&editor_actions, &compile_button)?;
        append(&editor_actions, &run_button)?;
        append(&editor_actions, &reset_code)?;
        append(&editor_toolbar, &editor_heading)?;
        append(&editor_toolbar, &editor_actions)?;

        let execution_boundary = element(&document, "div", "execution-boundary")?;
        let boundary_label = element(&document, "strong", "")?;
        boundary_label.set_text_content(Some("P0 执行边界"));
        let boundary_text = element(&document, "span", "")?;
        boundary_text.set_text_content(Some(
            "运行会用示例中的 positions/colors 常量重绘 CPU 三角形；任意 WGSL 控制流执行仍属于后续阶段。",
        ));
        append(&execution_boundary, &boundary_label)?;
        append(&execution_boundary, &boundary_text)?;

        let editor_body = element(&document, "div", "editor-body")?;
        let line_numbers = element(&document, "pre", "line-numbers")?;
        line_numbers.set_attribute("aria-hidden", "true")?;
        line_numbers.set_inner_html(&line_numbers_html(&initial_source, None));
        let editor_surface = element(&document, "div", "editor-surface")?;
        let highlight = element(&document, "pre", "syntax-highlight")?;
        highlight.set_attribute("aria-hidden", "true")?;
        highlight.set_inner_html(&highlight_overlay_html(&initial_source));
        let editor: HtmlTextAreaElement = document.create_element("textarea")?.dyn_into()?;
        editor.set_class_name("source-editor");
        editor.set_value(&initial_source);
        editor.set_attribute("aria-label", "可编辑的 WGSL shader 源码")?;
        editor.set_attribute("aria-describedby", "wgsl-editor-help")?;
        editor.set_attribute("aria-invalid", "false")?;
        editor.set_attribute("autocomplete", "off")?;
        editor.set_attribute("autocapitalize", "off")?;
        editor.set_attribute("spellcheck", "false")?;
        editor.set_attribute("wrap", "off")?;
        editor.set_attribute("maxlength", &MAX_WGSL_BYTES.to_string())?;
        append(&editor_surface, &highlight)?;
        append(&editor_surface, &editor)?;
        append(&editor_body, &line_numbers)?;
        append(&editor_body, &editor_surface)?;

        let editor_help = element(&document, "div", "editor-help")?;
        editor_help.set_id("wgsl-editor-help");
        editor_help.set_text_content(Some(
            "Ctrl/⌘ + S 保存 · Ctrl/⌘ + Enter 运行 · Ctrl/⌘ + Shift + Enter 仅编译",
        ));

        let diagnostic_panel = element(&document, "section", "diagnostics diagnostics-idle")?;
        diagnostic_panel.set_attribute("aria-live", "polite")?;
        diagnostic_panel.set_attribute("aria-atomic", "true")?;
        let diagnostic_marker = element(&document, "span", "diagnostic-marker")?;
        diagnostic_marker.set_attribute("aria-hidden", "true")?;
        let diagnostic_copy = element(&document, "div", "diagnostic-copy")?;
        let diagnostic_title = element(&document, "div", "diagnostic-title")?;
        diagnostic_title.set_text_content(Some("正在准备编译器"));
        let diagnostic_detail = element(&document, "pre", "diagnostic-detail")?;
        diagnostic_detail.set_text_content(Some("首次运行将校验内置 WGSL。"));
        append(&diagnostic_copy, &diagnostic_title)?;
        append(&diagnostic_copy, &diagnostic_detail)?;
        append(&diagnostic_panel, &diagnostic_marker)?;
        append(&diagnostic_panel, &diagnostic_copy)?;

        append(&editor_panel, &editor_toolbar)?;
        append(&editor_panel, &execution_boundary)?;
        append(&editor_panel, &editor_body)?;
        append(&editor_panel, &editor_help)?;
        append(&editor_panel, &diagnostic_panel)?;

        let preview_column = element(&document, "div", "preview-column")?;
        let viewport_panel = element(&document, "div", "panel viewport-panel")?;
        let viewport_header = element(&document, "div", "panel-heading")?;
        let viewport_title = element(&document, "h2", "")?;
        viewport_title.set_text_content(Some("CPU Framebuffer"));
        let viewport_badge = element(&document, "span", "preview-badge")?;
        viewport_badge.set_text_content(Some("RGBA8 · 720×480"));
        append(&viewport_header, &viewport_title)?;
        append(&viewport_header, &viewport_badge)?;
        let canvas: HtmlCanvasElement = document.create_element("canvas")?.dyn_into()?;
        canvas.set_width(WIDTH);
        canvas.set_height(HEIGHT);
        canvas.set_class_name("viewport");
        canvas.set_attribute("aria-label", "GLGPU CPU-rendered output")?;
        let context: CanvasRenderingContext2d = canvas
            .get_context("2d")?
            .ok_or_else(|| JsValue::from_str("Canvas 2D context is unavailable"))?
            .dyn_into()?;
        let metrics = element(&document, "div", "metrics")?;
        metrics.set_text_content(Some("waiting for first submit"));
        append(&viewport_panel, &viewport_header)?;
        append(&viewport_panel, &canvas)?;
        append(&viewport_panel, &metrics)?;

        let inspector = element(&document, "aside", "panel inspector")?;
        let inspector_title = element(&document, "h2", "")?;
        inspector_title.set_text_content(Some("Selected pixel history"));
        let help = element(&document, "p", "hint")?;
        help.set_text_content(Some(
            "Click the framebuffer to rerun the command buffer and trace that pixel.",
        ));
        let pixel_history = element(&document, "pre", "history")?;
        let reset_trace: HtmlButtonElement = document.create_element("button")?.dyn_into()?;
        reset_trace.set_class_name("button button-secondary");
        reset_trace.set_text_content(Some("追踪中心像素"));
        append(&inspector, &inspector_title)?;
        append(&inspector, &help)?;
        append(&inspector, &reset_trace)?;
        append(&inspector, &pixel_history)?;
        append(&preview_column, &viewport_panel)?;
        append(&preview_column, &inspector)?;
        append(&layout, &editor_panel)?;
        append(&layout, &preview_column)?;
        append(&root, &layout)?;

        let extent = Extent2d::new(WIDTH, HEIGHT);
        let device = CpuGpuSubset::new()
            .request_adapter()
            .request_device(DeviceDescriptor::new(extent).with_parallel(parallel))
            .map_err(js_error)?;
        let selected = PixelCoord::new(WIDTH / 2, HEIGHT / 2);
        let preview = triangle_preview_inputs(DEMO_WGSL).map_err(js_error)?;
        let state = Rc::new(RefCell::new(DemoState {
            device,
            context,
            canvas: canvas.clone(),
            metrics,
            pixel_history,
            editor: editor.clone(),
            highlight,
            line_numbers,
            compile_status,
            diagnostic_title,
            diagnostic_detail,
            diagnostic_panel,
            save_status,
            storage,
            saved_source: initial_source,
            has_saved_source,
            preview,
            selected,
            parallel,
            wgsl_summary: "WGSL not compiled".to_owned(),
        }));
        install_present_hooks(&state);
        state.borrow_mut().compile_editor(true)?;

        let input_state = Rc::clone(&state);
        let input = Closure::<dyn FnMut()>::new(move || {
            input_state.borrow().refresh_editor();
        });
        editor.add_event_listener_with_callback("input", input.as_ref().unchecked_ref())?;
        input.forget();

        let scroll_state = Rc::clone(&state);
        let scroll = Closure::<dyn FnMut()>::new(move || {
            scroll_state.borrow().sync_editor_scroll();
        });
        editor.add_event_listener_with_callback("scroll", scroll.as_ref().unchecked_ref())?;
        scroll.forget();

        let save_state = Rc::clone(&state);
        let save_click = Closure::<dyn FnMut()>::new(move || {
            if let Err(error) = save_state.borrow_mut().save_editor() {
                web_sys::console::error_1(&error);
            }
        });
        save_button
            .add_event_listener_with_callback("click", save_click.as_ref().unchecked_ref())?;
        save_click.forget();

        let compile_state = Rc::clone(&state);
        let compile_click = Closure::<dyn FnMut()>::new(move || {
            if let Err(error) = compile_state.borrow_mut().compile_editor(false) {
                web_sys::console::error_1(&error);
            }
        });
        compile_button
            .add_event_listener_with_callback("click", compile_click.as_ref().unchecked_ref())?;
        compile_click.forget();

        let run_state = Rc::clone(&state);
        let run_click = Closure::<dyn FnMut()>::new(move || {
            if let Err(error) = run_state.borrow_mut().compile_editor(true) {
                web_sys::console::error_1(&error);
            }
        });
        run_button.add_event_listener_with_callback("click", run_click.as_ref().unchecked_ref())?;
        run_click.forget();

        let reset_code_state = Rc::clone(&state);
        let reset_code_click = Closure::<dyn FnMut()>::new(move || {
            let mut state = reset_code_state.borrow_mut();
            state.editor.set_value(DEMO_WGSL);
            state.refresh_editor();
            if let Err(error) = state.compile_editor(true) {
                web_sys::console::error_1(&error);
            }
        });
        reset_code
            .add_event_listener_with_callback("click", reset_code_click.as_ref().unchecked_ref())?;
        reset_code_click.forget();

        let keyboard_state = Rc::clone(&state);
        let keyboard = Closure::<dyn FnMut(KeyboardEvent)>::new(move |event: KeyboardEvent| {
            if !event.ctrl_key() && !event.meta_key() {
                return;
            }
            if event.key().eq_ignore_ascii_case("s") {
                event.prevent_default();
                if let Err(error) = keyboard_state.borrow_mut().save_editor() {
                    web_sys::console::error_1(&error);
                }
            } else if event.key() == "Enter" {
                event.prevent_default();
                if let Err(error) = keyboard_state
                    .borrow_mut()
                    .compile_editor(!event.shift_key())
                {
                    web_sys::console::error_1(&error);
                }
            }
        });
        editor.add_event_listener_with_callback("keydown", keyboard.as_ref().unchecked_ref())?;
        keyboard.forget();

        let click_state = Rc::clone(&state);
        let click = Closure::<dyn FnMut(MouseEvent)>::new(move |event: MouseEvent| {
            let (client_width, client_height) = {
                let state = click_state.borrow();
                (state.canvas.client_width(), state.canvas.client_height())
            };
            if client_width <= 0 || client_height <= 0 {
                return;
            }
            let x = ((event.offset_x().max(0) as u64 * u64::from(WIDTH)) / client_width as u64)
                .min(u64::from(WIDTH - 1)) as u32;
            let y = ((event.offset_y().max(0) as u64 * u64::from(HEIGHT)) / client_height as u64)
                .min(u64::from(HEIGHT - 1)) as u32;
            let mut state = click_state.borrow_mut();
            if let Err(error) = state.render(PixelCoord::new(x, y)) {
                state.show_runtime_error(&error);
                web_sys::console::error_1(&error);
            }
        });
        canvas.add_event_listener_with_callback("click", click.as_ref().unchecked_ref())?;
        click.forget();

        let reset_trace_state = Rc::clone(&state);
        let reset_trace_click = Closure::<dyn FnMut()>::new(move || {
            let mut state = reset_trace_state.borrow_mut();
            if let Err(error) = state.render(PixelCoord::new(WIDTH / 2, HEIGHT / 2)) {
                state.show_runtime_error(&error);
                web_sys::console::error_1(&error);
            }
        });
        reset_trace.add_event_listener_with_callback(
            "click",
            reset_trace_click.as_ref().unchecked_ref(),
        )?;
        reset_trace_click.forget();

        Ok(())
    }

    fn install_present_hooks(state: &Rc<RefCell<DemoState>>) {
        let window = web_sys::window().expect("window");
        let presented = Rc::clone(state);
        let on_frame = Closure::<dyn FnMut(JsValue, JsValue)>::new(
            move |message: JsValue, _pixels: JsValue| {
                let serial = js_sys::Reflect::get(&message, &JsValue::from_str("serial"))
                    .ok()
                    .and_then(|value| value.as_f64())
                    .unwrap_or(0.0);
                let hash = js_sys::Reflect::get(&message, &JsValue::from_str("hash"))
                    .ok()
                    .and_then(|value| value.as_string())
                    .unwrap_or_default();
                let history = js_sys::Reflect::get(&message, &JsValue::from_str("history"))
                    .ok()
                    .and_then(|value| value.as_string())
                    .unwrap_or_default();
                presented
                    .borrow_mut()
                    .apply_worker_frame(serial, &hash, &history);
            },
        );
        js_sys::Reflect::set(
            window.as_ref(),
            &JsValue::from_str("glgpuOnPresentedFrame"),
            on_frame.as_ref(),
        )
        .ok();
        on_frame.forget();

        let replay = Rc::clone(state);
        let on_replay = Closure::<dyn FnMut()>::new(move || {
            let selected = replay.borrow().selected;
            if let Err(error) = replay.borrow_mut().render(selected) {
                web_sys::console::error_1(&error);
            }
        });
        js_sys::Reflect::set(
            window.as_ref(),
            &JsValue::from_str("glgpuReplayLocal"),
            on_replay.as_ref(),
        )
        .ok();
        on_replay.forget();
    }

    fn capability_text(window: &web_sys::Window, parallel: bool) -> String {
        let isolated =
            js_sys::Reflect::get(window.as_ref(), &JsValue::from_str("crossOriginIsolated"))
                .ok()
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
        let has_shared_array_buffer =
            js_sys::Reflect::has(&js_sys::global(), &JsValue::from_str("SharedArrayBuffer"))
                .unwrap_or(false);
        format!(
            "build={} · crossOriginIsolated={} · SharedArrayBuffer={} · logical CPUs={}",
            if parallel { "MT" } else { "ST" },
            isolated,
            has_shared_array_buffer,
            window.navigator().hardware_concurrency(),
        )
    }

    fn install_style(document: &Document, root: &Element) -> Result<(), JsValue> {
        let style = document.create_element("style")?;
        style.set_text_content(Some(
            r#"
                :root { color-scheme: dark; font-family: Inter, ui-sans-serif, system-ui, sans-serif; background: #070a12; color: #e8ecf7; }
                * { box-sizing: border-box; }
                body { margin: 0; min-height: 100vh; background: radial-gradient(circle at 12% -10%, #1d2b50 0, #070a12 40rem), #070a12; }
                button, textarea { font: inherit; }
                #app { width: min(1480px, calc(100% - 40px)); margin: 0 auto; padding: 38px 0 56px; }
                .hero { display: grid; gap: 10px; margin-bottom: 18px; }
                .eyebrow { color: #8da2d4; font: 700 12px/1.4 ui-monospace, monospace; letter-spacing: .16em; }
                h1 { margin: 0; font-size: clamp(34px, 5vw, 64px); line-height: .98; letter-spacing: -.045em; }
                .subtitle { max-width: 760px; margin: 4px 0; color: #aab5cf; font-size: 16px; line-height: 1.6; }
                .warning { justify-self: start; margin-top: 5px; padding: 7px 10px; border: 1px solid #7a5b29; border-radius: 7px; background: #2a1d0c; color: #ffd488; font: 700 12px ui-monospace, monospace; }
                .capabilities { margin: 18px 0; padding: 12px 14px; border: 1px solid #29334e; border-radius: 10px; background: #0d1220cc; color: #96a5c7; font: 12px/1.5 ui-monospace, monospace; }
                .workspace { display: grid; grid-template-columns: minmax(620px, 1.22fr) minmax(390px, .78fr); align-items: start; gap: 18px; }
                .panel { border: 1px solid #29334e; border-radius: 14px; background: #0b101dcc; box-shadow: 0 24px 80px #0007; overflow: hidden; }
                .editor-panel { min-width: 0; }
                .editor-toolbar { min-height: 72px; display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 14px 16px; border-bottom: 1px solid #29334e; background: #10182a; }
                .editor-heading { min-width: 0; }
                .editor-heading h2 { margin-bottom: 3px; }
                .editor-file { display: inline-block; color: #7f8ba8; font: 11px/1.4 ui-monospace, SFMono-Regular, Consolas, monospace; overflow-wrap: anywhere; }
                .save-status { display: inline-flex; align-items: center; gap: 5px; margin-left: 8px; color: #72809d; font: 10px/1.4 ui-monospace, SFMono-Regular, Consolas, monospace; }
                .save-status::before { content: ""; width: 5px; height: 5px; border-radius: 50%; background: #72809d; }
                .save-status.save-dirty { color: #f0c56d; }
                .save-status.save-dirty::before { background: #f0b94d; }
                .save-status.save-saved { color: #79dca7; }
                .save-status.save-saved::before { background: #42d487; }
                .save-status.save-error { color: #ff8da1; }
                .save-status.save-error::before { background: #ff5a75; }
                .editor-actions { display: flex; align-items: center; justify-content: flex-end; gap: 8px; flex-wrap: wrap; }
                .compile-status { display: inline-flex; align-items: center; gap: 7px; min-height: 28px; padding: 5px 9px; border: 1px solid #35405b; border-radius: 999px; background: #111827; color: #aab5cf; font: 700 11px/1 ui-monospace, SFMono-Regular, Consolas, monospace; white-space: nowrap; }
                .compile-status::before { content: ""; width: 7px; height: 7px; border-radius: 50%; background: #77839f; box-shadow: 0 0 0 3px #77839f20; }
                .compile-status.status-ok { border-color: #286747; background: #0d251a; color: #89e8b5; }
                .compile-status.status-ok::before { background: #48d88c; box-shadow: 0 0 0 3px #48d88c22; }
                .compile-status.status-error { border-color: #7e3542; background: #2a1118; color: #ff9eae; }
                .compile-status.status-error::before { background: #ff647d; box-shadow: 0 0 0 3px #ff647d22; }
                .compile-status.status-dirty { border-color: #725a2c; background: #281f0d; color: #f7cd7e; }
                .compile-status.status-dirty::before { background: #f4be55; box-shadow: 0 0 0 3px #f4be5522; }
                .execution-boundary { display: flex; gap: 9px; align-items: baseline; padding: 10px 16px; border-bottom: 1px solid #222c43; background: #0c1322; color: #8f9bb7; font-size: 12px; line-height: 1.55; }
                .execution-boundary strong { color: #f6c56d; white-space: nowrap; }
                .editor-body { display: grid; grid-template-columns: 54px minmax(0, 1fr); height: 550px; min-height: 360px; border-bottom: 1px solid #222c43; background: #070b14; }
                .line-numbers { height: 100%; margin: 0; padding: 18px 10px 36px 6px; border-right: 1px solid #20293c; overflow: hidden; color: #56627d; background: #080d17; text-align: right; user-select: none; font: 13px/1.65 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; white-space: pre; tab-size: 4; }
                .line-number { display: inline-block; width: 100%; position: relative; }
                .line-number.is-error { color: #ff8194; font-weight: 800; }
                .line-number.is-error::before { content: "●"; position: absolute; left: 2px; color: #ff506d; font-size: 8px; }
                .editor-surface { position: relative; min-width: 0; height: 100%; overflow: hidden; }
                .syntax-highlight, .source-editor { position: absolute; inset: 0; width: 100%; height: 100%; margin: 0; padding: 18px 20px 36px; border: 0; border-radius: 0; font: 13px/1.65 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; letter-spacing: 0; white-space: pre; tab-size: 4; }
                .syntax-highlight { z-index: 1; overflow: hidden; pointer-events: none; color: #d2daeb; background: transparent; }
                .source-editor { z-index: 2; resize: none; overflow: auto; outline: none; background: transparent; color: transparent; -webkit-text-fill-color: transparent; caret-color: #f6f8ff; }
                .source-editor::selection { background: #4264a899; -webkit-text-fill-color: transparent; }
                .source-editor:focus { box-shadow: inset 3px 0 #5686ff; }
                .source-editor[aria-invalid="true"] { box-shadow: inset 3px 0 #ff5f78; }
                .tok-comment { color: #687791; font-style: italic; }
                .tok-keyword { color: #e887ff; font-weight: 650; }
                .tok-attribute { color: #70d7ff; }
                .tok-type { color: #63e6be; }
                .tok-number { color: #ffbd7a; }
                .tok-function { color: #92b8ff; }
                .editor-help { padding: 9px 16px; border-bottom: 1px solid #222c43; color: #66738f; background: #0a0f1b; font: 11px/1.5 ui-monospace, SFMono-Regular, Consolas, monospace; }
                .diagnostics { display: grid; grid-template-columns: 8px minmax(0, 1fr); gap: 12px; min-height: 102px; padding: 14px 16px; background: #0d1320; }
                .diagnostic-marker { width: 8px; height: 8px; margin-top: 5px; border-radius: 50%; background: #75819b; box-shadow: 0 0 0 4px #75819b18; }
                .diagnostics-ok .diagnostic-marker { background: #45d58a; box-shadow: 0 0 0 4px #45d58a18; }
                .diagnostics-error { background: #1d0e15; }
                .diagnostics-error .diagnostic-marker { background: #ff5a75; box-shadow: 0 0 0 4px #ff5a7518; }
                .diagnostic-copy { min-width: 0; }
                .diagnostic-title { margin-bottom: 6px; color: #dfe7f8; font: 700 12px/1.45 ui-monospace, SFMono-Regular, Consolas, monospace; }
                .diagnostics-ok .diagnostic-title { color: #8de8b8; }
                .diagnostics-error .diagnostic-title { color: #ff9daf; }
                .diagnostic-detail { max-height: 170px; margin: 0; overflow: auto; color: #8f9bb7; font: 11px/1.6 ui-monospace, SFMono-Regular, Consolas, monospace; white-space: pre-wrap; overflow-wrap: anywhere; }
                .preview-column { display: grid; gap: 18px; min-width: 0; }
                .viewport-panel { padding: 12px; }
                .panel-heading { min-height: 34px; display: flex; align-items: flex-start; justify-content: space-between; gap: 10px; padding: 3px 2px 10px; }
                .panel-heading h2 { margin: 0; }
                .preview-badge { color: #7f8ba8; font: 10px/1.4 ui-monospace, SFMono-Regular, Consolas, monospace; }
                .viewport { display: block; width: 100%; height: auto; border-radius: 8px; background: #0d121e; cursor: crosshair; image-rendering: pixelated; }
                .metrics { padding: 11px 2px 0; color: #93a1c3; font: 12px/1.5 ui-monospace, monospace; overflow-wrap: anywhere; }
                .inspector { padding: 20px; }
                h2 { margin: 0 0 8px; font-size: 18px; }
                .hint { margin: 0 0 14px; color: #8f9bb7; font-size: 13px; line-height: 1.5; }
                .button { appearance: none; min-height: 34px; border: 1px solid #536897; border-radius: 8px; padding: 8px 12px; background: #1b2948; color: #edf3ff; font: 700 12px ui-monospace, SFMono-Regular, Consolas, monospace; cursor: pointer; transition: border-color .15s ease, background .15s ease, transform .15s ease; }
                .button:hover { background: #263a64; border-color: #7890c2; }
                .button:active { transform: translateY(1px); }
                .button:focus-visible { outline: 2px solid #7da0ff; outline-offset: 2px; }
                .button-primary { border-color: #5686ff; background: #315fc5; }
                .button-primary:hover { background: #3f70d9; }
                .button-secondary { background: #17233d; }
                .button-quiet { border-color: #3a455f; background: transparent; color: #9ba8c3; }
                .history { min-height: 180px; margin: 16px 0 0; padding: 14px; border-radius: 9px; background: #060913; color: #b7c5e8; font: 12px/1.65 ui-monospace, monospace; white-space: pre-wrap; overflow-wrap: anywhere; }
                @media (max-width: 1120px) {
                    .workspace { grid-template-columns: 1fr; }
                    .preview-column { grid-template-columns: minmax(0, 1.5fr) minmax(280px, .85fr); align-items: stretch; }
                    .editor-body { height: 520px; }
                }
                @media (max-width: 760px) {
                    #app { width: min(100% - 20px, 1480px); padding-top: 24px; }
                    .editor-toolbar { align-items: flex-start; flex-direction: column; }
                    .editor-actions { width: 100%; justify-content: flex-start; }
                    .compile-status { margin-right: auto; }
                    .execution-boundary { align-items: flex-start; flex-direction: column; gap: 3px; }
                    .editor-body { grid-template-columns: 44px minmax(0, 1fr); height: 460px; }
                    .line-numbers { padding-left: 3px; padding-right: 7px; }
                    .syntax-highlight, .source-editor { padding-left: 14px; padding-right: 14px; }
                    .preview-column { grid-template-columns: 1fr; }
                }
            "#,
        ));
        append(root, &style)
    }

    fn element(document: &Document, tag: &str, class_name: &str) -> Result<HtmlElement, JsValue> {
        let element: HtmlElement = document.create_element(tag)?.dyn_into()?;
        if !class_name.is_empty() {
            element.set_class_name(class_name);
        }
        Ok(element)
    }

    fn append(parent: &Element, child: &Element) -> Result<(), JsValue> {
        parent.append_child(child).map(|_| ())
    }

    fn js_error(error: impl std::fmt::Display) -> JsValue {
        JsValue::from_str(&error.to_string())
    }

    fn color_channel(value: f32) -> u8 {
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn boot(parallel: bool) -> Result<(), wasm_bindgen::JsValue> {
    app::boot(parallel)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn native_build_marker() -> &'static str {
    "glgpu-web is active only on wasm32"
}

#[cfg(all(target_arch = "wasm32", feature = "parallel"))]
pub use wasm_bindgen_rayon::init_thread_pool;

#[cfg(test)]
mod tests {
    use super::editor_support::{
        highlight_overlay_html, highlight_wgsl, line_numbers_html, triangle_preview_inputs,
        utf8_span_to_utf16_range,
    };

    #[test]
    fn overlay_html_adds_the_line_box_a_textarea_keeps_for_a_final_newline() {
        for source in ["fn main() {}", "fn main() {}\n", "a\nb\n", ""] {
            let html = highlight_overlay_html(source);
            assert!(html.ends_with('\n'), "{source:?}");
            assert_eq!(
                html.matches('\n').count(),
                source.matches('\n').count() + 1,
                "{source:?}"
            );
        }
    }

    #[test]
    fn highlighter_marks_wgsl_tokens_and_escapes_source() {
        let highlighted =
            highlight_wgsl("// <script>&\n@vertex fn main() -> vec4f { return vec4f(1.0); }");

        assert!(highlighted.contains("class=\"tok-comment\""));
        assert!(highlighted.contains("class=\"tok-attribute\">@vertex"));
        assert!(highlighted.contains("class=\"tok-keyword\">fn"));
        assert!(highlighted.contains("class=\"tok-type\">vec4f"));
        assert!(highlighted.contains("class=\"tok-number\">1.0"));
        assert!(highlighted.contains("&lt;script&gt;&amp;"));
        assert!(!highlighted.contains("<script>"));
    }

    #[test]
    fn highlighter_keeps_nested_comments_and_unicode_as_text() {
        let highlighted = highlight_wgsl("/* 外层 /* nested */ 😀 */\nfn main() {}");

        assert_eq!(highlighted.matches("class=\"tok-comment\"").count(), 1);
        assert!(highlighted.contains("外层 /* nested */ 😀"));
        assert!(highlighted.contains("class=\"tok-function\">main"));
    }

    #[test]
    fn converts_utf8_diagnostic_offsets_to_textarea_utf16_indices() {
        let source = "中😀x";
        let (start, end) = utf8_span_to_utf16_range(source, 7, 1);

        assert_eq!((start, end), (3, 4));
    }

    #[test]
    fn line_numbers_mark_only_the_diagnostic_line() {
        let lines = line_numbers_html("first\nsecond\nthird", Some(2));

        assert!(lines.contains("class=\"line-number is-error\">2"));
        assert_eq!(lines.matches("is-error").count(), 1);
        assert_eq!(lines.matches("class=\"line-number").count(), 3);
    }

    #[test]
    fn extracts_triangle_positions_and_colors_from_the_demo_wgsl() {
        let source = include_str!("../../../examples/triangle/p0_triangle.wgsl");
        let preview = triangle_preview_inputs(source).expect("demo preview inputs");

        assert_eq!(preview.positions[0], [0.0, 0.75]);
        assert_eq!(preview.positions[2], [0.75, -0.65]);
        assert_eq!(preview.colors[0], [1.0, 0.2, 0.2]);
        assert_eq!(preview.colors[2], [0.2, 0.45, 1.0]);
    }

    #[test]
    fn edited_demo_constants_change_the_preview_inputs() {
        let source = include_str!("../../../examples/triangle/p0_triangle.wgsl")
            .replace("vec3f(1.0, 0.2, 0.2)", "vec3f(0.1, 0.8, 0.4)");
        let preview = triangle_preview_inputs(&source).expect("edited preview inputs");

        assert_eq!(preview.colors[0], [0.1, 0.8, 0.4]);
    }

    #[test]
    fn preview_reports_valid_wgsl_that_is_outside_the_demo_shape() {
        let error = triangle_preview_inputs("@compute @workgroup_size(1) fn main() {}")
            .expect_err("compute-only source has no triangle preview inputs");

        assert!(error.contains("positions"));
    }
}
