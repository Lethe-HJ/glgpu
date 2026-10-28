//! Debug page for the CPU WebGL2 learning track.
//!
//! This crate is a second Wasm entry point. It depends only on `glgpu-webgl`
//! and talks to the browser through Canvas 2D `putImageData`. It does not call
//! `getContext("webgl")`, `getContext("webgl2")`, or `navigator.gpu`, and it
//! does not link the WebGPU-track crates.
//!
//! The permanent banner stays `CpuWebGl2LearningTrack — NOT WEBGL CONFORMANT`.

#![forbid(unsafe_code)]

mod compare_frame;

/// What 「追踪中心像素」 may do for the current compare slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(any(test, target_arch = "wasm32")), allow(dead_code))]
enum CenterTrace {
    /// Native mode: leave the toggle and the visible frame alone.
    Ignore,
    /// Editor preview is already the 720×480 CPU frame.
    EditorPreview,
    /// Fixed compare scene: read the visible canvas, not the editor framebuffer.
    VisibleCanvas,
}

#[cfg_attr(not(any(test, target_arch = "wasm32")), allow(dead_code))]
fn center_trace_for(mode: Option<&str>) -> CenterTrace {
    match mode {
        Some("native") => CenterTrace::Ignore,
        Some("editor") => CenterTrace::EditorPreview,
        _ => CenterTrace::VisibleCanvas,
    }
}

#[cfg(any(target_arch = "wasm32", test))]
mod editor;

#[cfg(target_arch = "wasm32")]
mod app {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::editor::{
        self, DEMO_ESSL, MAX_ESSL_BYTES, PREVIEW_HEIGHT, PREVIEW_WIDTH, SAVED_ESSL_KEY,
        format_tracked_pixel, highlight_overlay_html, line_numbers_html, utf8_span_to_utf16_range,
    };
    use glgpu_webgl::{LearningContext, P0_MILESTONE_CLEAR, WebGl2LearningTrack};
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::prelude::*;
    use web_sys::{
        Document, Element, HtmlButtonElement, HtmlCanvasElement, HtmlElement, HtmlTextAreaElement,
        KeyboardEvent, MouseEvent, Storage,
    };

    struct DemoState {
        context: LearningContext,
        compare_slot: HtmlElement,
        compare_status: HtmlElement,
        viewport_badge: HtmlElement,
        compile_button: HtmlButtonElement,
        run_button: HtmlButtonElement,
        reference_button: HtmlButtonElement,
        native_button: HtmlButtonElement,
        center_trace: HtmlButtonElement,
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
        selected_x: u32,
        selected_y: u32,
        frame_serial: u64,
        essl_summary: String,
    }

    impl DemoState {
        fn compile_editor(&mut self, run_after_compile: bool) -> Result<bool, JsValue> {
            if self.editor.read_only() {
                return Ok(false);
            }
            let source = self.editor.value();
            if source.len() > MAX_ESSL_BYTES {
                self.show_limit_error(source.len());
                return Ok(false);
            }

            let compiled = match editor::compile_essl(&source) {
                Ok(compiled) => compiled,
                Err(diagnostic) => {
                    self.show_diagnostic(&source, diagnostic.message, diagnostic.location);
                    return Ok(false);
                }
            };

            if run_after_compile {
                let started = js_sys::Date::now();
                match editor::render_preview(&source) {
                    Ok(context) => {
                        self.context = context;
                        self.show_success(&source, &compiled.functions, true)?;
                        self.present(started)?;
                        self.show_selected_pixel();
                    }
                    Err(diagnostic) => {
                        self.show_run_error(&source, &diagnostic.message);
                        return Ok(false);
                    }
                }
            } else {
                self.show_success(&source, &compiled.functions, false)?;
            }
            Ok(true)
        }

        fn call_window1(&self, name: &str, arg: &JsValue) -> Result<JsValue, JsValue> {
            let window =
                web_sys::window().ok_or_else(|| JsValue::from_str("window is unavailable"))?;
            let function = js_sys::Reflect::get(&window, &JsValue::from_str(name))?;
            let function: js_sys::Function = function.dyn_into()?;
            function.call1(&JsValue::UNDEFINED, arg)
        }

        fn mark_pending(&self, state: &str) -> Result<(), JsValue> {
            self.compare_slot
                .set_attribute("data-compare-state", state)?;
            self.compare_slot.remove_attribute("data-compare-ready")?;
            self.compare_slot.remove_attribute("data-compare-error")?;
            Ok(())
        }

        fn set_pressed(&self, native: bool) -> Result<(), JsValue> {
            self.reference_button
                .set_attribute("aria-pressed", if native { "false" } else { "true" })?;
            self.native_button
                .set_attribute("aria-pressed", if native { "true" } else { "false" })?;
            self.editor.set_read_only(native);
            self.compile_button.set_disabled(native);
            self.run_button.set_disabled(native);
            self.center_trace.set_disabled(native);
            Ok(())
        }

        fn compare_mode(&self) -> Option<String> {
            self.compare_slot.get_attribute("data-compare-state")
        }

        fn show_visible_center(&self) -> Result<(), JsValue> {
            let canvas = self
                .compare_slot
                .query_selector("canvas")?
                .ok_or_else(|| JsValue::from_str("compare canvas is missing"))?
                .dyn_into::<HtmlCanvasElement>()?;
            let width = canvas.width();
            let height = canvas.height();
            if width == 0 || height == 0 {
                return Err(JsValue::from_str("compare canvas has no pixels"));
            }
            let x = width / 2;
            let y = height / 2;
            let (red, green, blue, alpha) = self.read_canvas_pixel(x, y)?;
            self.pixel_history.set_text_content(Some(&format!(
                "pixel ({x}, {y})\nrgba({red}, {green}, {blue}, {alpha})\n固定对照场景没有 primitive history"
            )));
            Ok(())
        }

        fn trace_center(&mut self) -> Result<(), JsValue> {
            match super::center_trace_for(self.compare_mode().as_deref()) {
                super::CenterTrace::Ignore => Ok(()),
                super::CenterTrace::EditorPreview => {
                    self.selected_x = PREVIEW_WIDTH / 2;
                    self.selected_y = PREVIEW_HEIGHT / 2;
                    self.show_selected_pixel();
                    Ok(())
                }
                super::CenterTrace::VisibleCanvas => self.show_visible_center(),
            }
        }

        fn show_reference_compare(&self) -> Result<(), JsValue> {
            self.set_pressed(false)?;
            self.mark_pending("reference")?;
            self.call_window1("glgpuPresentCompare", &JsValue::from_str("reference"))?;
            self.compare_status
                .set_text_content(Some("正在显示：固定对照场景 · 参考实现"));
            self.viewport_badge
                .set_text_content(Some("RGBA8 · 320×240"));
            self.metrics
                .set_text_content(Some("固定对照场景 · RGBA8 · 320×240"));
            Ok(())
        }

        fn show_native_compare(&self) -> Result<(), JsValue> {
            self.set_pressed(true)?;
            self.mark_pending("native")?;
            self.call_window1("glgpuPresentCompare", &JsValue::from_str("native"))?;
            Ok(())
        }

        fn present_editor_pixels(&self, pixels: &[u8]) -> Result<(), JsValue> {
            let window =
                web_sys::window().ok_or_else(|| JsValue::from_str("window is unavailable"))?;
            let function: js_sys::Function =
                js_sys::Reflect::get(&window, &JsValue::from_str("glgpuPresentEditorFrame"))?
                    .dyn_into()?;
            let bytes = js_sys::Uint8Array::from(pixels);
            function.call3(
                &JsValue::UNDEFINED,
                &JsValue::from(PREVIEW_WIDTH),
                &JsValue::from(PREVIEW_HEIGHT),
                &bytes,
            )?;
            self.compare_status
                .set_text_content(Some("正在显示：编辑器预览 · 不参与原生对照"));
            self.viewport_badge
                .set_text_content(Some(&format!("RGBA8 · {PREVIEW_WIDTH}×{PREVIEW_HEIGHT}")));
            self.set_pressed(false)?;
            Ok(())
        }

        fn read_canvas_pixel(&self, x: u32, y: u32) -> Result<(u8, u8, u8, u8), JsValue> {
            let window =
                web_sys::window().ok_or_else(|| JsValue::from_str("window is unavailable"))?;
            let function: js_sys::Function =
                js_sys::Reflect::get(&window, &JsValue::from_str("glgpuReadCanvasPixel"))?
                    .dyn_into()?;
            let value =
                function.call2(&JsValue::UNDEFINED, &JsValue::from(x), &JsValue::from(y))?;
            let bytes = js_sys::Uint8Array::new(&value);
            Ok((
                bytes.get_index(0),
                bytes.get_index(1),
                bytes.get_index(2),
                bytes.get_index(3),
            ))
        }

        fn present(&mut self, started: f64) -> Result<(), JsValue> {
            let pixels = self.context.to_rgba8_bytes();
            self.present_editor_pixels(&pixels)?;
            self.frame_serial = self.frame_serial.saturating_add(1);
            let elapsed_ms = js_sys::Date::now() - started;
            self.metrics.set_text_content(Some(&format!(
                "frame {} · {:.2} ms · hash {:016x} · CPU interpreter · {}",
                self.frame_serial,
                elapsed_ms,
                self.context.stable_hash64(),
                self.essl_summary,
            )));
            Ok(())
        }

        fn show_selected_pixel(&self) {
            let text = match self.context.pixel(self.selected_x, self.selected_y) {
                Some(color) => format_tracked_pixel(self.selected_x, self.selected_y, color),
                None => format!(
                    "pixel ({}, {}) is outside the framebuffer",
                    self.selected_x, self.selected_y
                ),
            };
            self.pixel_history.set_text_content(Some(&text));
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
                "点击“编译”只校验当前 ESSL 子集；点击“运行”会执行 CPU 解释器并重绘三角形。",
            ));
            self.update_save_status();
            self.sync_editor_scroll();
        }

        fn show_success(
            &mut self,
            source: &str,
            functions: &[String],
            ran: bool,
        ) -> Result<(), JsValue> {
            self.essl_summary = format!("ESSL subset: {} functions", functions.len());
            self.editor.set_attribute("aria-invalid", "false")?;
            self.line_numbers
                .set_inner_html(&line_numbers_html(source, None));
            self.highlight
                .set_inner_html(&highlight_overlay_html(source));
            self.compile_status
                .set_class_name("compile-status status-ok");
            self.compile_status.set_text_content(Some(if ran {
                "预览已运行"
            } else {
                "编译通过"
            }));
            self.diagnostic_panel
                .set_class_name("diagnostics diagnostics-ok");
            self.diagnostic_title.set_text_content(Some(&format!(
                "ESSL 子集编译通过 · {} 个函数",
                functions.len()
            )));
            let names = if functions.is_empty() {
                "未发现函数".to_owned()
            } else {
                functions.join(" · ")
            };
            let detail = if ran {
                format!(
                    "{names}\n已用 u_scale=1.0 和三个 P0 顶点属性执行 CPU 解释器，并按 WebGL clip 体积光栅化。这不是 drawArrays。"
                )
            } else {
                format!("{names}\n仅完成当前子集校验；编译不会提交新帧。")
            };
            self.diagnostic_detail.set_text_content(Some(&detail));
            self.sync_editor_scroll();
            Ok(())
        }

        fn show_diagnostic(
            &self,
            source: &str,
            message: String,
            location: Option<editor::SourceLocation>,
        ) {
            let error_line = location.as_ref().map(|location| location.line);
            self.line_numbers
                .set_inner_html(&line_numbers_html(source, error_line));
            self.highlight
                .set_inner_html(&highlight_overlay_html(source));
            self.compile_status
                .set_class_name("compile-status status-error");
            self.compile_status.set_text_content(Some("编译失败"));
            self.diagnostic_panel
                .set_class_name("diagnostics diagnostics-error");
            let place = location
                .as_ref()
                .map(|location| format!(" · 第 {} 行，第 {} 列", location.line, location.column))
                .unwrap_or_default();
            self.diagnostic_title
                .set_text_content(Some(&format!("ESSL 子集编译失败{place} · 未运行")));
            self.diagnostic_detail.set_text_content(Some(&message));
            self.editor.set_attribute("aria-invalid", "true").ok();
            if let Some(location) = location {
                let length = location.end.saturating_sub(location.start).max(1) as u32;
                let (start, end) = utf8_span_to_utf16_range(source, location.start as u32, length);
                self.editor.focus().ok();
                self.editor.set_selection_range(start, end).ok();
            }
            self.sync_editor_scroll();
        }

        fn show_run_error(&self, source: &str, message: &str) {
            self.line_numbers
                .set_inner_html(&line_numbers_html(source, None));
            self.highlight
                .set_inner_html(&highlight_overlay_html(source));
            self.compile_status
                .set_class_name("compile-status status-error");
            self.compile_status.set_text_content(Some("运行失败"));
            self.diagnostic_panel
                .set_class_name("diagnostics diagnostics-error");
            self.diagnostic_title
                .set_text_content(Some("ESSL 子集编译通过，但 CPU 解释器未能画出三角形"));
            self.diagnostic_detail.set_text_content(Some(message));
            self.editor.set_attribute("aria-invalid", "false").ok();
            self.sync_editor_scroll();
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
                "源码为 {source_bytes} bytes，当前编辑器上限为 {MAX_ESSL_BYTES} bytes。"
            )));
            self.editor.set_attribute("aria-invalid", "true").ok();
        }

        fn save_editor(&mut self) -> Result<bool, JsValue> {
            let source = self.editor.value();
            let Some(storage) = &self.storage else {
                self.save_status.set_class_name("save-status save-error");
                self.save_status.set_text_content(Some("保存不可用"));
                return Ok(false);
            };
            if let Err(error) = storage.set_item(SAVED_ESSL_KEY, &source) {
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

        fn sync_editor_scroll(&self) {
            self.highlight.set_scroll_top(self.editor.scroll_top());
            self.highlight.set_scroll_left(self.editor.scroll_left());
            self.line_numbers.set_scroll_top(self.editor.scroll_top());
        }
    }

    pub fn boot() -> Result<(), JsValue> {
        console_error_panic_hook::set_once();

        let window = web_sys::window().ok_or_else(|| JsValue::from_str("window is unavailable"))?;
        let storage = window.local_storage().ok().flatten();
        let saved_source = storage
            .as_ref()
            .and_then(|storage| storage.get_item(SAVED_ESSL_KEY).ok().flatten())
            .filter(|source| source.len() <= MAX_ESSL_BYTES);
        let has_saved_source = saved_source.is_some();
        let initial_source = saved_source.unwrap_or_else(|| DEMO_ESSL.to_owned());
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
        eyebrow.set_text_content(Some("CPU WEBGL2 LEARNING TRACK"));
        let title = element(&document, "h1", "")?;
        title.set_text_content(Some("GLGPU WebGL Debug Lab"));
        let subtitle = element(&document, "p", "subtitle")?;
        subtitle.set_text_content(Some(
            "Rust/Wasm runs the current ESSL subset on a CPU interpreter, then rasterizes one triangle. Canvas 2D only presents the RGBA8 frame. This page does not call browser WebGL.",
        ));
        let warning = element(&document, "div", "warning")?;
        warning.set_text_content(Some(editor::conformance_banner()));
        append(&header, &eyebrow)?;
        append(&header, &title)?;
        append(&header, &subtitle)?;
        append(&header, &warning)?;
        append(&root, &header)?;

        let capability = element(&document, "section", "capabilities")?;
        capability.set_text_content(Some(&capability_text(&window)));
        append(&root, &capability)?;

        let layout = element(&document, "section", "workspace")?;
        let editor_panel = element(&document, "section", "panel editor-panel")?;
        editor_panel.set_attribute("aria-labelledby", "essl-editor-title")?;
        let editor_toolbar = element(&document, "div", "editor-toolbar")?;
        let editor_heading = element(&document, "div", "editor-heading")?;
        let editor_title = element(&document, "h2", "")?;
        editor_title.set_id("essl-editor-title");
        editor_title.set_text_content(Some("ESSL Shader Editor"));
        let editor_file = element(&document, "div", "editor-file")?;
        editor_file.set_text_content(Some("p0_vertex.essl"));
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
        let save_button = button(&document, "button button-quiet", "保存")?;
        save_button.set_attribute("title", "保存到当前浏览器（Ctrl/⌘ + S）")?;
        let compile_button = button(&document, "button button-secondary", "编译")?;
        compile_button.set_attribute("title", "编译 / 校验（Ctrl/⌘ + Shift + Enter）")?;
        let run_button = button(&document, "button button-primary", "运行")?;
        run_button.set_attribute("title", "编译并运行 CPU 解释器（Ctrl/⌘ + Enter）")?;
        let reset_code = button(&document, "button button-quiet", "还原")?;
        reset_code.set_attribute("title", "还原示例 ESSL")?;
        append(&editor_actions, &compile_status)?;
        append(&editor_actions, &save_button)?;
        append(&editor_actions, &compile_button)?;
        append(&editor_actions, &run_button)?;
        append(&editor_actions, &reset_code)?;
        append(&editor_toolbar, &editor_heading)?;
        append(&editor_toolbar, &editor_actions)?;

        let execution_boundary = element(&document, "div", "execution-boundary")?;
        let boundary_label = element(&document, "strong", "")?;
        boundary_label.set_text_content(Some("学习切片边界"));
        let boundary_text = element(&document, "span", "")?;
        boundary_text.set_text_content(Some(
            "运行会绑定 u_scale=1.0 和示例的三个顶点属性，执行 main，再按 clip 体积 -w..w 光栅化。子集以外的 ESSL 会被拒绝。这不是 drawArrays。",
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
        editor.set_attribute("aria-label", "可编辑的 ESSL shader 源码")?;
        editor.set_attribute("aria-describedby", "essl-editor-help")?;
        editor.set_attribute("aria-invalid", "false")?;
        editor.set_attribute("autocomplete", "off")?;
        editor.set_attribute("autocapitalize", "off")?;
        editor.set_attribute("spellcheck", "false")?;
        editor.set_attribute("wrap", "off")?;
        editor.set_attribute("maxlength", &MAX_ESSL_BYTES.to_string())?;
        append(&editor_surface, &highlight)?;
        append(&editor_surface, &editor)?;
        append(&editor_body, &line_numbers)?;
        append(&editor_body, &editor_surface)?;

        let editor_help = element(&document, "div", "editor-help")?;
        editor_help.set_id("essl-editor-help");
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
        diagnostic_title.set_text_content(Some("正在准备 ESSL 子集编译器"));
        let diagnostic_detail = element(&document, "pre", "diagnostic-detail")?;
        diagnostic_detail.set_text_content(Some("首次运行将校验并执行内置 ESSL。"));
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
        viewport_badge.set_id("viewport-badge");
        viewport_badge.set_text_content(Some("RGBA8 · 720×480"));
        append(&viewport_header, &viewport_title)?;
        append(&viewport_header, &viewport_badge)?;
        let compare_slot = element(&document, "div", "compare-slot")?;
        compare_slot.set_id("compare-slot");
        let compare_status = element(&document, "p", "compare-status")?;
        compare_status.set_id("compare-status");
        compare_status.set_text_content(Some("正在准备固定对照场景"));
        let compare_note = element(&document, "p", "compare-note")?;
        compare_note.set_id("compare-note");
        compare_note.set_text_content(Some(
            "原生对照只用于观看画面和本地像素比较。它不运行编辑器里的调试流程，也不改变 CpuWebGl2LearningTrack — NOT WEBGL CONFORMANT。",
        ));
        let toggle = element(&document, "div", "compare-toggle")?;
        toggle.set_attribute("role", "group")?;
        toggle.set_attribute("aria-label", "对照后端")?;
        let reference_button = button(&document, "button button-secondary", "参考实现")?;
        reference_button.set_attribute("type", "button")?;
        reference_button.set_attribute("aria-pressed", "true")?;
        let native_button = button(&document, "button button-secondary", "原生")?;
        native_button.set_attribute("type", "button")?;
        native_button.set_attribute("aria-pressed", "false")?;
        append(&toggle, &reference_button)?;
        append(&toggle, &native_button)?;
        let metrics = element(&document, "div", "metrics")?;
        metrics.set_text_content(Some("waiting for first submit"));
        append(&viewport_panel, &viewport_header)?;
        append(&viewport_panel, &toggle)?;
        append(&viewport_panel, &compare_note)?;
        append(&viewport_panel, &compare_slot)?;
        append(&viewport_panel, &compare_status)?;
        append(&viewport_panel, &metrics)?;

        let inspector = element(&document, "aside", "panel inspector")?;
        let inspector_title = element(&document, "h2", "")?;
        inspector_title.set_text_content(Some("Selected pixel"));
        let help = element(&document, "p", "hint")?;
        help.set_text_content(Some(
            "Click the framebuffer to read that stored pixel. This slice has no depth buffer and no per-primitive event log.",
        ));
        let pixel_history = element(&document, "pre", "history")?;
        let reset_trace = button(&document, "button button-secondary", "追踪中心像素")?;
        append(&inspector, &inspector_title)?;
        append(&inspector, &help)?;
        append(&inspector, &reset_trace)?;
        append(&inspector, &pixel_history)?;
        append(&preview_column, &viewport_panel)?;
        append(&preview_column, &inspector)?;
        append(&layout, &editor_panel)?;
        append(&layout, &preview_column)?;
        append(&root, &layout)?;

        let mut context = WebGl2LearningTrack::new()
            .create_context(PREVIEW_WIDTH, PREVIEW_HEIGHT)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        context.clear(P0_MILESTONE_CLEAR);
        let selected_x = PREVIEW_WIDTH / 2;
        let selected_y = PREVIEW_HEIGHT / 2;
        let state = Rc::new(RefCell::new(DemoState {
            context,
            compare_slot: compare_slot.clone(),
            compare_status,
            viewport_badge,
            compile_button: compile_button.clone(),
            run_button: run_button.clone(),
            reference_button: reference_button.clone(),
            native_button: native_button.clone(),
            center_trace: reset_trace.clone(),
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
            selected_x,
            selected_y,
            frame_serial: 0,
            essl_summary: "ESSL not compiled".to_owned(),
        }));
        {
            let mut state = state.borrow_mut();
            state.compile_editor(false)?;
            state.show_reference_compare()?;
        }

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

        let reference_state = Rc::clone(&state);
        let reference_click = Closure::<dyn FnMut()>::new(move || {
            if let Err(error) = reference_state.borrow().show_reference_compare() {
                web_sys::console::error_1(&error);
            }
        });
        reference_button
            .add_event_listener_with_callback("click", reference_click.as_ref().unchecked_ref())?;
        reference_click.forget();

        let native_state = Rc::clone(&state);
        let native_click = Closure::<dyn FnMut()>::new(move || {
            if let Err(error) = native_state.borrow().show_native_compare() {
                web_sys::console::error_1(&error);
            }
        });
        native_button
            .add_event_listener_with_callback("click", native_click.as_ref().unchecked_ref())?;
        native_click.forget();

        let reset_code_state = Rc::clone(&state);
        let reset_code_click = Closure::<dyn FnMut()>::new(move || {
            let mut state = reset_code_state.borrow_mut();
            state.editor.set_value(DEMO_ESSL);
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
            let Some(target) = event.target() else {
                return;
            };
            let Ok(canvas) = target.dyn_into::<HtmlCanvasElement>() else {
                return;
            };
            let mut state = click_state.borrow_mut();
            let mode = state
                .compare_slot
                .get_attribute("data-compare-state")
                .unwrap_or_default();
            if mode == "native" {
                return;
            }
            if mode == "editor" {
                let Some((x, y)) = editor::pixel_coord_from_click(
                    event.offset_x(),
                    event.offset_y(),
                    canvas.client_width(),
                    canvas.client_height(),
                ) else {
                    return;
                };
                state.selected_x = x;
                state.selected_y = y;
                state.show_selected_pixel();
                return;
            }
            let width = canvas.width();
            let height = canvas.height();
            if width == 0 || height == 0 {
                return;
            }
            let x = (event.offset_x().max(0) as u32).min(width - 1);
            let y = (event.offset_y().max(0) as u32).min(height - 1);
            match state.read_canvas_pixel(x, y) {
                Ok((red, green, blue, alpha)) => {
                    state.pixel_history.set_text_content(Some(&format!(
                        "pixel ({x}, {y})\nrgba({red}, {green}, {blue}, {alpha})\n固定对照场景没有 primitive history"
                    )));
                }
                Err(error) => web_sys::console::error_1(&error),
            }
        });
        compare_slot.add_event_listener_with_callback("click", click.as_ref().unchecked_ref())?;
        click.forget();

        let reset_trace_state = Rc::clone(&state);
        let reset_trace_click = Closure::<dyn FnMut()>::new(move || {
            if let Err(error) = reset_trace_state.borrow_mut().trace_center() {
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

    fn capability_text(window: &web_sys::Window) -> String {
        let isolated =
            js_sys::Reflect::get(window.as_ref(), &JsValue::from_str("crossOriginIsolated"))
                .ok()
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
        let has_shared_array_buffer =
            js_sys::Reflect::has(&js_sys::global(), &JsValue::from_str("SharedArrayBuffer"))
                .unwrap_or(false);
        format!(
            "track=CpuWebGl2LearningTrack · build=ST · backend=cpu-from-scratch · crossOriginIsolated={} · SharedArrayBuffer={} · logical CPUs={}",
            isolated,
            has_shared_array_buffer,
            window.navigator().hardware_concurrency(),
        )
    }

    fn install_style(document: &Document, root: &Element) -> Result<(), JsValue> {
        let style = document.create_element("style")?;
        style.set_text_content(Some(include_str!("debug.css")));
        append(root, &style)
    }

    fn button(
        document: &Document,
        class_name: &str,
        label: &str,
    ) -> Result<HtmlButtonElement, JsValue> {
        let button: HtmlButtonElement = document.create_element("button")?.dyn_into()?;
        button.set_class_name(class_name);
        button.set_text_content(Some(label));
        Ok(button)
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
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn compare_frame_rgba8(
    width: u32,
    height: u32,
    clear: &[u8],
    vertices: &[f32],
) -> Result<Vec<u8>, wasm_bindgen::JsValue> {
    compare_frame::compare_frame_rgba8(width, height, clear, vertices)
        .map_err(|error| wasm_bindgen::JsValue::from_str(&error.to_string()))
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn boot() -> Result<(), wasm_bindgen::JsValue> {
    app::boot()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn native_build_marker() -> &'static str {
    "glgpu-webgl-web is active only on wasm32"
}

#[cfg(test)]
mod center_trace_tests {
    use super::{CenterTrace, center_trace_for};

    #[test]
    fn native_center_trace_does_not_read_the_learning_framebuffer() {
        assert_eq!(center_trace_for(Some("native")), CenterTrace::Ignore);
        assert_eq!(center_trace_for(Some("editor")), CenterTrace::EditorPreview);
        assert_eq!(
            center_trace_for(Some("reference")),
            CenterTrace::VisibleCanvas
        );
        assert_eq!(center_trace_for(None), CenterTrace::VisibleCanvas);
    }
}
