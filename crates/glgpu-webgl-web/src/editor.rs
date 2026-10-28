//! ESSL editor helpers for the learning-track debug page.
//!
//! Parsing, lowering, and raster stay in `glgpu-webgl`. This module only
//! prepares diagnostics, highlighting, and the fixed P0 vertex bindings the
//! page feeds to that crate. It does not call browser WebGL or WebGPU.

use std::fmt::Write as _;

use glgpu_webgl::{
    BytecodeModule, DebugSymbol, FrontendError, LowerError, P0_MILESTONE_CLEAR, Rgba8,
    ShaderDrawError, SourceSpan, WebGl2LearningTrack, lower, p0_milestone_clip_vertices,
    parse_essl_subset,
};

pub const PREVIEW_WIDTH: u32 = 720;
pub const PREVIEW_HEIGHT: u32 = 480;
pub const MAX_ESSL_BYTES: usize = 256 * 1024;
pub const DEMO_ESSL: &str = include_str!("../../../examples/webgl2-triangle/p0_vertex.essl");
pub const SAVED_ESSL_KEY: &str = "glgpu.essl.editor.v1";

const KEYWORDS: &[&str] = &[
    "attribute",
    "break",
    "case",
    "centroid",
    "const",
    "continue",
    "default",
    "discard",
    "do",
    "else",
    "false",
    "flat",
    "for",
    "highp",
    "if",
    "in",
    "inout",
    "invariant",
    "layout",
    "lowp",
    "mediump",
    "out",
    "precision",
    "return",
    "switch",
    "true",
    "uniform",
    "varying",
    "void",
    "while",
];

const TYPES: &[&str] = &[
    "bool",
    "bvec2",
    "bvec3",
    "bvec4",
    "float",
    "int",
    "ivec2",
    "ivec3",
    "ivec4",
    "mat2",
    "mat3",
    "mat4",
    "sampler2D",
    "sampler3D",
    "samplerCube",
    "uint",
    "uvec2",
    "uvec3",
    "uvec4",
    "vec2",
    "vec3",
    "vec4",
];

pub fn conformance_banner() -> &'static str {
    WebGl2LearningTrack::new().banner()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLocation {
    pub line: u32,
    pub column: u32,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    pub location: Option<SourceLocation>,
}

#[derive(Clone, Debug)]
pub struct CompileSuccess {
    pub module: BytecodeModule,
    pub functions: Vec<String>,
}

pub fn highlight_essl(source: &str) -> String {
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

        if bytes[cursor] == b'#' {
            let start = cursor;
            cursor += 1;
            while cursor < bytes.len() && bytes[cursor] != b'\n' {
                cursor += char_len_at(source, cursor);
            }
            push_token(&mut output, "tok-preprocessor", &source[start..cursor]);
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
            } else if TYPES.contains(&identifier) {
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

/// Map a canvas click into storage pixels. `y == 0` is the top row.
pub fn pixel_coord_from_click(
    offset_x: i32,
    offset_y: i32,
    client_width: i32,
    client_height: i32,
) -> Option<(u32, u32)> {
    if client_width <= 0 || client_height <= 0 {
        return None;
    }
    let x = ((offset_x.max(0) as u64 * u64::from(PREVIEW_WIDTH)) / client_width as u64)
        .min(u64::from(PREVIEW_WIDTH - 1)) as u32;
    let y = ((offset_y.max(0) as u64 * u64::from(PREVIEW_HEIGHT)) / client_height as u64)
        .min(u64::from(PREVIEW_HEIGHT - 1)) as u32;
    Some((x, y))
}

pub fn format_tracked_pixel(x: u32, y: u32, color: Rgba8) -> String {
    let coverage = if color == P0_MILESTONE_CLEAR {
        "stored color matches the clear color"
    } else {
        "stored color differs from the clear color"
    };
    format!(
        "pixel ({x}, {y})\nfinal rgba({}, {}, {}, {})\n{coverage}\nno depth buffer on this learning slice\nno per-primitive event log; the interpreter runs once per vertex\n",
        color.r, color.g, color.b, color.a
    )
}

pub fn compile_essl(source: &str) -> Result<CompileSuccess, Diagnostic> {
    let unit = parse_essl_subset(source).map_err(|error| from_frontend(source, error))?;
    let module = lower(&unit).map_err(|error| from_lower(source, error))?;
    let functions = module
        .functions
        .iter()
        .map(|function| function.name.clone())
        .collect();
    Ok(CompileSuccess { module, functions })
}

/// Clear, run `main` for the three P0 vertices, and rasterize.
///
/// Declared names receive bindings only when this page knows them: `u_scale`
/// is `1.0`, and `a_x`..`a_b` are the milestone clip vertex. Other names are
/// left unbound so the interpreter can report them.
pub fn render_preview(source: &str) -> Result<glgpu_webgl::LearningContext, Diagnostic> {
    let compiled = compile_essl(source)?;
    let module = &compiled.module;
    let clips = p0_milestone_clip_vertices();
    let inputs = [
        vertex_inputs(clips[0]),
        vertex_inputs(clips[1]),
        vertex_inputs(clips[2]),
    ];
    let uniform_offer = [("u_scale", 1.0)];
    let uniforms = filter_bindings(&uniform_offer, &module.uniforms);
    let bound = [
        filter_bindings(&inputs[0], &module.inputs),
        filter_bindings(&inputs[1], &module.inputs),
        filter_bindings(&inputs[2], &module.inputs),
    ];
    let mut context = WebGl2LearningTrack::new()
        .create_context(PREVIEW_WIDTH, PREVIEW_HEIGHT)
        .map_err(|error| Diagnostic {
            message: error.to_string(),
            location: None,
        })?;
    context.clear(P0_MILESTONE_CLEAR);
    context
        .draw_shader_triangle(module, "main", &uniforms, [&bound[0], &bound[1], &bound[2]])
        .map_err(from_draw)?;
    Ok(context)
}

fn from_frontend(source: &str, error: FrontendError) -> Diagnostic {
    Diagnostic {
        message: error.message,
        location: Some(location_from_span(source, error.span)),
    }
}

fn from_lower(source: &str, error: LowerError) -> Diagnostic {
    Diagnostic {
        message: error.message,
        location: Some(location_from_span(source, error.span)),
    }
}

fn from_draw(error: ShaderDrawError) -> Diagnostic {
    Diagnostic {
        message: format!(
            "{error}\n调试页只绑定 u_scale=1.0，以及三个顶点的 a_x、a_y、a_z、a_w、a_r、a_g、a_b。未声明的名字不会绑定。"
        ),
        location: None,
    }
}

fn location_from_span(source: &str, span: SourceSpan) -> SourceLocation {
    let (line, column) = span.line_column(source);
    SourceLocation {
        line,
        column,
        start: span.start,
        end: span.end,
    }
}

fn vertex_inputs(vertex: glgpu_webgl::ClipVertex) -> [(&'static str, f32); 7] {
    [
        ("a_x", vertex.position.x),
        ("a_y", vertex.position.y),
        ("a_z", vertex.position.z),
        ("a_w", vertex.position.w),
        ("a_r", vertex.color[0]),
        ("a_g", vertex.color[1]),
        ("a_b", vertex.color[2]),
    ]
}

fn filter_bindings<'a>(
    offered: &'a [(&'a str, f32)],
    symbols: &[DebugSymbol],
) -> Vec<(&'a str, f32)> {
    offered
        .iter()
        .copied()
        .filter(|(name, _)| symbols.iter().any(|symbol| symbol.name == *name))
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_source_key_does_not_collide_with_the_webgpu_editor() {
        assert_eq!(SAVED_ESSL_KEY, "glgpu.essl.editor.v1");
        assert!(DEMO_ESSL.len() <= MAX_ESSL_BYTES);
        assert!(!SAVED_ESSL_KEY.contains("wgsl"));
    }

    #[test]
    fn banner_keeps_the_learning_track_wording() {
        let banner = conformance_banner();
        assert!(banner.contains("NOT WEBGL CONFORMANT"));
        assert!(banner.contains("CpuWebGl2LearningTrack"));
    }

    #[test]
    fn highlighter_marks_essl_tokens_and_escapes_source() {
        let highlighted = highlight_essl(
            "#version 300 es\n// <script>&\nuniform float u_scale;\nvoid main() { float x = 1.0; }\n",
        );

        assert!(highlighted.contains("class=\"tok-preprocessor\">#version 300 es"));
        assert!(highlighted.contains("class=\"tok-comment\""));
        assert!(highlighted.contains("class=\"tok-keyword\">uniform"));
        assert!(highlighted.contains("class=\"tok-keyword\">void"));
        assert!(highlighted.contains("class=\"tok-type\">float"));
        assert!(highlighted.contains("class=\"tok-number\">1.0"));
        assert!(highlighted.contains("class=\"tok-function\">main"));
        assert!(highlighted.contains("&lt;script&gt;&amp;"));
        assert!(!highlighted.contains("<script>"));
        assert!(!highlighted.contains("class=\"tok-keyword\">fn"));
    }

    #[test]
    fn highlighter_keeps_nested_comments_as_one_span() {
        let highlighted = highlight_essl("/* 外层 /* nested */ 😀 */\nvoid main() {}\n");

        assert_eq!(highlighted.matches("class=\"tok-comment\"").count(), 1);
        assert!(highlighted.contains("外层 /* nested */ 😀"));
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
    fn click_maps_css_pixels_back_to_storage_pixels() {
        assert_eq!(pixel_coord_from_click(360, 240, 720, 480), Some((360, 240)));
        assert_eq!(pixel_coord_from_click(180, 120, 360, 240), Some((360, 240)));
        assert_eq!(pixel_coord_from_click(0, 0, 720, 480), Some((0, 0)));
        assert_eq!(pixel_coord_from_click(-4, 10, 720, 480), Some((0, 10)));
        assert_eq!(pixel_coord_from_click(10, 10, 0, 480), None);
    }

    #[test]
    fn demo_essl_compiles_and_draws_the_p0_triangle() {
        let compiled = compile_essl(DEMO_ESSL).expect("demo subset");
        assert_eq!(compiled.functions, vec!["main".to_owned()]);

        let frame = render_preview(DEMO_ESSL).expect("demo frame");
        let clear = P0_MILESTONE_CLEAR;
        assert_ne!(frame.pixel(360, 80), Some(clear));
        assert_eq!(frame.pixel(0, 0), Some(clear));
        assert!(frame.banner().contains("NOT WEBGL CONFORMANT"));
        assert_eq!(frame.to_rgba8_bytes().len(), 720 * 480 * 4);
        assert_eq!(
            frame.stable_hash64(),
            render_preview(DEMO_ESSL).unwrap().stable_hash64()
        );

        let tracked = format_tracked_pixel(360, 80, frame.pixel(360, 80).unwrap());
        assert!(tracked.contains("differs from the clear color"));
        assert!(tracked.contains("no depth buffer"));
    }

    #[test]
    fn editing_the_essl_scale_changes_the_framebuffer_hash() {
        let scaled = DEMO_ESSL
            .replace("float x = a_x * u_scale;", "float x = a_x * 0.5;")
            .replace("float y = a_y * u_scale;", "float y = a_y * 0.5;");
        let original = render_preview(DEMO_ESSL).expect("original");
        let edited = render_preview(&scaled).expect("scaled x");

        assert_ne!(original.stable_hash64(), edited.stable_hash64());
        assert_ne!(original.pixel(360, 80), edited.pixel(360, 80));
    }

    #[test]
    fn rejected_version_reports_a_source_location() {
        let error = compile_essl("#version 330\nvoid main() {}\n").expect_err("desktop glsl");

        assert!(error.message.contains("300"));
        let location = error.location.expect("span");
        assert_eq!(location.line, 1);
        assert!(location.column >= 1);
    }

    #[test]
    fn interpreter_fixture_compiles_but_does_not_draw_a_triangle() {
        let source = include_str!("../../../examples/webgl2-triangle/float_local.essl");
        let compiled = compile_essl(source).expect("load_const fixture");
        assert_eq!(compiled.functions, vec!["main".to_owned()]);

        let error = render_preview(source).expect_err("missing clip outputs");
        assert!(error.message.contains("v_clip_x"));
        assert!(error.location.is_none());
    }

    #[test]
    fn shell_manifest_does_not_depend_on_webgpu_crates() {
        let manifest = include_str!("../Cargo.toml");
        let dependency_names: Vec<&str> = manifest
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(name, _)| name.trim())
            .filter(|name| name.starts_with("glgpu-"))
            .collect();
        assert_eq!(dependency_names, vec!["glgpu-webgl"]);
    }

    #[test]
    fn page_source_does_not_request_a_browser_webgl_context() {
        let source = include_str!("lib.rs");
        assert!(source.contains("get_context(\"2d\")"));
        assert!(!source.contains("get_context(\"webgl"));
        assert!(!source.contains(".gpu()"));
        assert!(source.contains("conformance_banner"));
    }
}
