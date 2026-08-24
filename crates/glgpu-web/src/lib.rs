#![forbid(unsafe_code)]

#[cfg(any(target_arch = "wasm32", test))]
mod editor_support {
    use std::fmt::Write as _;

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
}

#[cfg(target_arch = "wasm32")]
mod app {
    use std::cell::RefCell;
    use std::fmt::Write as _;
    use std::rc::Rc;

    use super::editor_support::{highlight_wgsl, line_numbers_html, utf8_span_to_utf16_range};
    use glgpu_canvas2d::present_rgba8;
    use glgpu_core::{Extent2d, PixelCoord, Rgba8, ScreenVertex, Triangle};
    use glgpu_webgpu::{CpuDevice, CpuGpuSubset, DeviceDescriptor};
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::prelude::*;
    use web_sys::{
        CanvasRenderingContext2d, Document, Element, HtmlButtonElement, HtmlCanvasElement,
        HtmlElement, HtmlTextAreaElement, KeyboardEvent, MouseEvent,
    };

    const WIDTH: u32 = 720;
    const HEIGHT: u32 = 480;
    const MAX_WGSL_BYTES: usize = 256 * 1024;
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
        selected: PixelCoord,
        parallel: bool,
        wgsl_summary: String,
    }

    impl DemoState {
        fn render(&mut self, selected: PixelCoord) -> Result<(), JsValue> {
            let started = js_sys::Date::now();
            let mut encoder = self.device.create_command_encoder();
            encoder
                .clear(Rgba8::new(13, 18, 30, 255), 1.0)
                .map_err(js_error)?;
            encoder.select_pixel(selected).map_err(js_error)?;
            encoder
                .draw_triangle(Triangle::from_vertices(
                    1,
                    ScreenVertex::new(
                        WIDTH as f32 * 0.50,
                        HEIGHT as f32 * 0.10,
                        0.35,
                        Rgba8::new(255, 76, 96, 255),
                    ),
                    ScreenVertex::new(
                        WIDTH as f32 * 0.12,
                        HEIGHT as f32 * 0.88,
                        0.55,
                        Rgba8::new(62, 230, 139, 255),
                    ),
                    ScreenVertex::new(
                        WIDTH as f32 * 0.88,
                        HEIGHT as f32 * 0.88,
                        0.75,
                        Rgba8::new(66, 139, 255, 255),
                    ),
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

        fn refresh_editor(&self) {
            let source = self.editor.value();
            self.highlight.set_inner_html(&highlight_wgsl(&source));
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
                "点击“编译”只进行 WGSL 语法与语义校验；点击“运行”会先编译，再重跑当前 CPU 预览。",
            ));
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
                format!("{entries}\n已重新提交固定 CPU 三角形预览；当前 P0 尚未执行编辑后的 WGSL。")
            } else {
                format!("{entries}\n仅完成校验；编译操作不会提交新帧。")
            };
            self.diagnostic_detail.set_text_content(Some(&detail));

            if run_after_compile && let Err(error) = self.render(self.selected) {
                self.show_runtime_error(&error);
                return Err(error);
            }
            Ok(true)
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
        append(&editor_heading, &editor_title)?;
        append(&editor_heading, &editor_file)?;

        let editor_actions = element(&document, "div", "editor-actions")?;
        let compile_status = element(&document, "span", "compile-status status-idle")?;
        compile_status.set_text_content(Some("准备中"));
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
            "编辑内容会由 Naga 编译校验；运行会重提固定 CPU 三角形，当前尚不执行 WGSL。",
        ));
        append(&execution_boundary, &boundary_label)?;
        append(&execution_boundary, &boundary_text)?;

        let editor_body = element(&document, "div", "editor-body")?;
        let line_numbers = element(&document, "pre", "line-numbers")?;
        line_numbers.set_attribute("aria-hidden", "true")?;
        line_numbers.set_inner_html(&line_numbers_html(DEMO_WGSL, None));
        let editor_surface = element(&document, "div", "editor-surface")?;
        let highlight = element(&document, "pre", "syntax-highlight")?;
        highlight.set_attribute("aria-hidden", "true")?;
        highlight.set_inner_html(&highlight_wgsl(DEMO_WGSL));
        let editor: HtmlTextAreaElement = document.create_element("textarea")?.dyn_into()?;
        editor.set_class_name("source-editor");
        editor.set_value(DEMO_WGSL);
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
            "Ctrl/⌘ + Enter 运行 · Ctrl/⌘ + Shift + Enter 仅编译 · 支持语法与语义错误定位",
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
            selected,
            parallel,
            wgsl_summary: "WGSL not compiled".to_owned(),
        }));
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
            state.highlight.set_inner_html(&highlight_wgsl(DEMO_WGSL));
            state
                .line_numbers
                .set_inner_html(&line_numbers_html(DEMO_WGSL, None));
            if let Err(error) = state.compile_editor(true) {
                web_sys::console::error_1(&error);
            }
        });
        reset_code
            .add_event_listener_with_callback("click", reset_code_click.as_ref().unchecked_ref())?;
        reset_code_click.forget();

        let keyboard_state = Rc::clone(&state);
        let keyboard = Closure::<dyn FnMut(KeyboardEvent)>::new(move |event: KeyboardEvent| {
            if event.key() != "Enter" || (!event.ctrl_key() && !event.meta_key()) {
                return;
            }
            event.prevent_default();
            if let Err(error) = keyboard_state
                .borrow_mut()
                .compile_editor(!event.shift_key())
            {
                web_sys::console::error_1(&error);
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
                .editor-file { color: #7f8ba8; font: 11px/1.4 ui-monospace, SFMono-Regular, Consolas, monospace; overflow-wrap: anywhere; }
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
    use super::editor_support::{highlight_wgsl, line_numbers_html, utf8_span_to_utf16_range};

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
}
