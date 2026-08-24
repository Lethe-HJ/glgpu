#![forbid(unsafe_code)]

#[cfg(target_arch = "wasm32")]
mod app {
    use std::cell::RefCell;
    use std::fmt::Write as _;
    use std::rc::Rc;

    use glgpu_canvas2d::present_rgba8;
    use glgpu_core::{Extent2d, PixelCoord, Rgba8, ScreenVertex, Triangle};
    use glgpu_webgpu::{CpuDevice, CpuGpuSubset, DeviceDescriptor};
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::prelude::*;
    use web_sys::{
        CanvasRenderingContext2d, Document, Element, HtmlButtonElement, HtmlCanvasElement,
        HtmlElement, MouseEvent,
    };

    const WIDTH: u32 = 720;
    const HEIGHT: u32 = 480;
    const DEMO_WGSL: &str = include_str!("../../../examples/triangle/p0_triangle.wgsl");

    struct DemoState {
        device: CpuDevice,
        context: CanvasRenderingContext2d,
        canvas: HtmlCanvasElement,
        metrics: HtmlElement,
        pixel_history: HtmlElement,
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
        let viewport_panel = element(&document, "div", "panel viewport-panel")?;
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
        let reset: HtmlButtonElement = document.create_element("button")?.dyn_into()?;
        reset.set_class_name("button");
        reset.set_text_content(Some("Trace center pixel"));
        append(&inspector, &inspector_title)?;
        append(&inspector, &help)?;
        append(&inspector, &reset)?;
        append(&inspector, &pixel_history)?;
        append(&layout, &viewport_panel)?;
        append(&layout, &inspector)?;
        append(&root, &layout)?;

        let extent = Extent2d::new(WIDTH, HEIGHT);
        let device = CpuGpuSubset::new()
            .request_adapter()
            .request_device(DeviceDescriptor::new(extent).with_parallel(parallel))
            .map_err(js_error)?;
        let shader = device.create_shader_module(DEMO_WGSL).map_err(js_error)?;
        let wgsl_summary = format!(
            "WGSL core syntax/semantics only: {} entry points",
            shader.info().entry_points.len()
        );

        let selected = PixelCoord::new(WIDTH / 2, HEIGHT / 2);
        let state = Rc::new(RefCell::new(DemoState {
            device,
            context,
            canvas: canvas.clone(),
            metrics,
            pixel_history,
            selected,
            parallel,
            wgsl_summary,
        }));
        state.borrow_mut().render(selected)?;

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
            if let Err(error) = click_state.borrow_mut().render(PixelCoord::new(x, y)) {
                web_sys::console::error_1(&error);
            }
        });
        canvas.add_event_listener_with_callback("click", click.as_ref().unchecked_ref())?;
        click.forget();

        let reset_state = Rc::clone(&state);
        let reset_click = Closure::<dyn FnMut()>::new(move || {
            if let Err(error) = reset_state
                .borrow_mut()
                .render(PixelCoord::new(WIDTH / 2, HEIGHT / 2))
            {
                web_sys::console::error_1(&error);
            }
        });
        reset.add_event_listener_with_callback("click", reset_click.as_ref().unchecked_ref())?;
        reset_click.forget();

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
                body { margin: 0; min-height: 100vh; background: radial-gradient(circle at 15% 0%, #17213d 0, #070a12 42rem); }
                #app { width: min(1180px, calc(100% - 32px)); margin: 0 auto; padding: 42px 0 56px; }
                .hero { display: grid; gap: 10px; margin-bottom: 18px; }
                .eyebrow { color: #8da2d4; font: 700 12px/1.4 ui-monospace, monospace; letter-spacing: .16em; }
                h1 { margin: 0; font-size: clamp(34px, 6vw, 68px); line-height: .98; letter-spacing: -.045em; }
                .subtitle { max-width: 760px; margin: 4px 0; color: #aab5cf; font-size: 16px; line-height: 1.6; }
                .warning { justify-self: start; margin-top: 5px; padding: 7px 10px; border: 1px solid #7a5b29; border-radius: 7px; background: #2a1d0c; color: #ffd488; font: 700 12px ui-monospace, monospace; }
                .capabilities { margin: 18px 0; padding: 12px 14px; border: 1px solid #29334e; border-radius: 10px; background: #0d1220cc; color: #96a5c7; font: 12px/1.5 ui-monospace, monospace; }
                .workspace { display: grid; grid-template-columns: minmax(0, 1.8fr) minmax(270px, .8fr); gap: 16px; }
                .panel { border: 1px solid #29334e; border-radius: 14px; background: #0b101dcc; box-shadow: 0 24px 80px #0007; overflow: hidden; }
                .viewport-panel { padding: 12px; }
                .viewport { display: block; width: 100%; height: auto; border-radius: 8px; background: #0d121e; cursor: crosshair; image-rendering: pixelated; }
                .metrics { padding: 11px 2px 0; color: #93a1c3; font: 12px/1.5 ui-monospace, monospace; overflow-wrap: anywhere; }
                .inspector { padding: 20px; }
                h2 { margin: 0 0 8px; font-size: 18px; }
                .hint { margin: 0 0 14px; color: #8f9bb7; font-size: 13px; line-height: 1.5; }
                .button { appearance: none; border: 1px solid #536897; border-radius: 8px; padding: 9px 12px; background: #1b2948; color: #edf3ff; font: 700 12px ui-monospace, monospace; cursor: pointer; }
                .button:hover { background: #263a64; }
                .history { min-height: 180px; margin: 16px 0 0; padding: 14px; border-radius: 9px; background: #060913; color: #b7c5e8; font: 12px/1.65 ui-monospace, monospace; white-space: pre-wrap; overflow-wrap: anywhere; }
                @media (max-width: 820px) { .workspace { grid-template-columns: 1fr; } #app { width: min(100% - 20px, 1180px); padding-top: 24px; } }
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
