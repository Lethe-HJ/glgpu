//! Triangle rasterizer for webgl-mock demos.
//!
//! [`DemoContext::draw_screen_space_triangle`] takes vertices that are
//! already in storage pixels. Storage y grows downward from the top image row.
//! [`DemoContext::draw_clip_space_triangle`] applies the WebGL clip volume
//! `-w <= x, y, z <= w` and a bottom-left window, then writes that same
//! storage. [`DemoContext::draw_shader_triangle`] fills the clip vertices
//! from interpreter outputs. Coverage uses sample centers and barycentric
//! weights. There is no depth test, no `drawArrays`, and no shared raster crate.

use std::error::Error;
use std::fmt;

use crate::clip::{
    ClipError, ClipOutcome, ClipPosition, ClipVertex, WindowVertex, clip_triangle, project_window,
    storage_y_from_window_y,
};
use crate::framebuffer::{DemoContext, Rgba8};
use crate::vm::{BytecodeModule, ExecTrace, VmError, execute_bound};

/// One corner of a demo triangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenVertex {
    pub x: f32,
    pub y: f32,
    pub color: Rgba8,
}

/// A single triangle already in storage pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenTriangle {
    pub vertices: [ScreenVertex; 3],
}

/// Vertex-shader outputs read by [`DemoContext::draw_shader_triangle`].
///
/// The subset has no `vec4` and no `gl_Position`. These scalar names stand in
/// for clip position and the color varying.
pub const SHADER_CLIP_OUTPUTS: [&str; 4] = ["v_clip_x", "v_clip_y", "v_clip_z", "v_clip_w"];
pub const SHADER_COLOR_OUTPUTS: [&str; 3] = ["v_color_r", "v_color_g", "v_color_b"];
/// Optional. When the shader does not declare it, alpha is 1.
pub const SHADER_COLOR_ALPHA: &str = "v_color_a";

/// Clear color used by the P0-milestone picture.
///
/// The bytes match the WebGPU demo's clear `(13, 18, 30, 255)`. The triangles
/// are produced by this crate, not by that demo's renderer.
pub const P0_MILESTONE_CLEAR: Rgba8 = Rgba8::new(13, 18, 30, 255);

/// NDC positions published by the WebGPU P0 triangle demo.
const P0_MILESTONE_NDC: [[f32; 2]; 3] = [[0.0, 0.75], [-0.75, -0.65], [0.75, -0.65]];

/// Linear colors published by that same demo, quantized when the triangle is built.
const P0_MILESTONE_COLOR: [[f32; 3]; 3] = [[1.0, 0.2, 0.2], [0.2, 1.0, 0.3], [0.2, 0.45, 1.0]];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RasterError {
    NonFiniteVertex,
    CoordinateOutOfRange,
    DegenerateTriangle,
    /// Perspective divide needs clip `w > 0`.
    UnprojectedClipVertex,
}

impl fmt::Display for RasterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteVertex => write!(f, "screen-space vertex is not finite"),
            Self::CoordinateOutOfRange => {
                write!(
                    f,
                    "screen-space vertex is outside the framebuffer coordinate range"
                )
            }
            Self::DegenerateTriangle => write!(f, "screen-space triangle has zero area"),
            Self::UnprojectedClipVertex => write!(f, "clip-space w is not positive"),
        }
    }
}

/// Failure while running three vertex invocations and rasterizing them.
#[derive(Debug, Eq, PartialEq)]
pub enum ShaderDrawError {
    Vm(VmError),
    Raster(RasterError),
    MissingOutput { name: &'static str },
    UnassignedOutput { name: &'static str },
    NonFiniteOutput { name: &'static str },
}

impl fmt::Display for ShaderDrawError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vm(error) => write!(f, "{error}"),
            Self::Raster(error) => write!(f, "{error}"),
            Self::MissingOutput { name } => {
                write!(f, "shader is missing output `{name}`")
            }
            Self::UnassignedOutput { name } => {
                write!(f, "shader output `{name}` was not written")
            }
            Self::NonFiniteOutput { name } => {
                write!(f, "shader output `{name}` is not finite")
            }
        }
    }
}

impl Error for ShaderDrawError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Vm(error) => Some(error),
            Self::Raster(error) => Some(error),
            Self::MissingOutput { .. }
            | Self::UnassignedOutput { .. }
            | Self::NonFiniteOutput { .. } => None,
        }
    }
}

impl From<VmError> for ShaderDrawError {
    fn from(error: VmError) -> Self {
        Self::Vm(error)
    }
}

impl From<RasterError> for ShaderDrawError {
    fn from(error: RasterError) -> Self {
        Self::Raster(error)
    }
}

/// Three interpreter traces plus the bottom-left window polygon that was drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderTriangleDraw {
    pub traces: [ExecTrace; 3],
    /// Empty when the triangle was fully outside the clip volume.
    pub window: Vec<WindowVertex>,
}

impl Error for RasterError {}

/// Clip-space vertices for the P0 demo triangle.
///
/// `z = 0` and `w = 1`, so the positions match the demo's NDC xy. The near
/// plane of this stack is still `z = -w`, not WebGPU's `z = 0`.
pub fn p0_milestone_clip_vertices() -> [ClipVertex; 3] {
    std::array::from_fn(|index| {
        let [x, y] = P0_MILESTONE_NDC[index];
        let [r, g, b] = P0_MILESTONE_COLOR[index];
        ClipVertex {
            position: ClipPosition {
                x,
                y,
                z: 0.0,
                w: 1.0,
            },
            color: [r, g, b, 1.0],
        }
    })
}

/// Build the colored triangle that corresponds to the P0 demo's three vertices.
///
/// The vertices go through the WebGL window transform and then the transitional
/// storage flip. A lit rotating cube is a later milestone.
pub fn p0_milestone_triangle(width: u32, height: u32) -> ScreenTriangle {
    let window = p0_milestone_clip_vertices().map(|vertex| {
        project_window(vertex, width, height).expect("p0 clip vertices are inside the volume")
    });
    ScreenTriangle {
        vertices: [
            to_storage(&window[0], height),
            to_storage(&window[1], height),
            to_storage(&window[2], height),
        ],
    }
}

impl DemoContext {
    /// Draw a triangle whose vertices are already in storage pixels.
    ///
    /// Storage y grows downward from the top image row. This is not WebGL
    /// window space.
    pub fn draw_screen_space_triangle(
        &mut self,
        triangle: ScreenTriangle,
    ) -> Result<(), RasterError> {
        let width = self.width();
        let height = self.height();
        paint_storage_triangle(self.pixels_mut(), width, height, &triangle)
    }

    /// Clip, project, and rasterize one triangle.
    ///
    /// The returned vertices are in bottom-left window coordinates. Pixels are
    /// stored with row 0 at the top. A triangle fully outside the clip volume
    /// draws nothing and returns an empty window.
    pub fn draw_clip_space_triangle(
        &mut self,
        triangle: [ClipVertex; 3],
    ) -> Result<Vec<WindowVertex>, RasterError> {
        let width = self.width();
        let height = self.height();
        let pixels = self.pixels_mut();
        match clip_triangle(triangle).map_err(map_clip)? {
            ClipOutcome::Inside(vertices) => {
                let window = window_vertices(&vertices, width, height)?;
                paint_window_polygon(pixels, width, height, &window, false)?;
                Ok(window)
            }
            ClipOutcome::Clipped(vertices) => {
                let window = window_vertices(&vertices, width, height)?;
                paint_window_polygon(pixels, width, height, &window, true)?;
                Ok(window)
            }
        }
    }

    /// Run `entry_point` once per vertex and rasterize the clip outputs.
    ///
    /// Uniforms are shared. Each vertex supplies its own `in` floats. The
    /// function reads [`SHADER_CLIP_OUTPUTS`] and [`SHADER_COLOR_OUTPUTS`].
    /// This is not `drawArrays`.
    pub fn draw_shader_triangle(
        &mut self,
        module: &BytecodeModule,
        entry_point: &str,
        uniforms: &[(&str, f32)],
        vertices: [&[(&str, f32)]; 3],
    ) -> Result<ShaderTriangleDraw, ShaderDrawError> {
        let mut traces = Vec::with_capacity(3);
        let mut clips = Vec::with_capacity(3);
        for inputs in vertices {
            let trace = execute_bound(module, entry_point, uniforms, inputs)?;
            clips.push(clip_vertex_from_trace(&trace)?);
            traces.push(trace);
        }
        let clips = [clips[0], clips[1], clips[2]];
        let window = self.draw_clip_space_triangle(clips)?;
        let traces = <[ExecTrace; 3]>::try_from(traces).expect("three vertex invocations");
        Ok(ShaderTriangleDraw { traces, window })
    }
}

fn clip_vertex_from_trace(trace: &ExecTrace) -> Result<ClipVertex, ShaderDrawError> {
    let x = require_output(trace, SHADER_CLIP_OUTPUTS[0])?;
    let y = require_output(trace, SHADER_CLIP_OUTPUTS[1])?;
    let z = require_output(trace, SHADER_CLIP_OUTPUTS[2])?;
    let w = require_output(trace, SHADER_CLIP_OUTPUTS[3])?;
    let r = require_output(trace, SHADER_COLOR_OUTPUTS[0])?;
    let g = require_output(trace, SHADER_COLOR_OUTPUTS[1])?;
    let b = require_output(trace, SHADER_COLOR_OUTPUTS[2])?;
    let a = if trace.output(SHADER_COLOR_ALPHA).is_some() {
        require_output(trace, SHADER_COLOR_ALPHA)?
    } else {
        1.0
    };
    Ok(ClipVertex {
        position: ClipPosition { x, y, z, w },
        color: [r, g, b, a],
    })
}

fn require_output(trace: &ExecTrace, name: &'static str) -> Result<f32, ShaderDrawError> {
    let Some(slot) = trace.output(name) else {
        return Err(ShaderDrawError::MissingOutput { name });
    };
    let Some(bits) = slot.value_bits else {
        return Err(ShaderDrawError::UnassignedOutput { name });
    };
    let value = f32::from_bits(bits);
    if !value.is_finite() {
        return Err(ShaderDrawError::NonFiniteOutput { name });
    }
    Ok(value)
}

fn window_vertices(
    vertices: &[ClipVertex],
    width: u32,
    height: u32,
) -> Result<Vec<WindowVertex>, RasterError> {
    vertices
        .iter()
        .copied()
        .map(|vertex| project_window(vertex, width, height).map_err(map_clip))
        .collect()
}

fn paint_window_polygon(
    pixels: &mut [Rgba8],
    width: u32,
    height: u32,
    window: &[WindowVertex],
    skip_degenerate: bool,
) -> Result<(), RasterError> {
    if window.len() < 3 {
        return Ok(());
    }
    for index in 1..window.len() - 1 {
        let triangle = ScreenTriangle {
            vertices: [
                to_storage(&window[0], height),
                to_storage(&window[index], height),
                to_storage(&window[index + 1], height),
            ],
        };
        match paint_storage_triangle(pixels, width, height, &triangle) {
            Ok(()) => {}
            Err(RasterError::DegenerateTriangle) if skip_degenerate => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn to_storage(window: &WindowVertex, height: u32) -> ScreenVertex {
    ScreenVertex {
        x: window.x,
        y: storage_y_from_window_y(window.y, height),
        color: window.color,
    }
}

fn map_clip(error: ClipError) -> RasterError {
    match error {
        ClipError::NonFinite => RasterError::NonFiniteVertex,
        ClipError::OutsideClipVolume | ClipError::Unprojected => RasterError::UnprojectedClipVertex,
    }
}

fn paint_storage_triangle(
    pixels: &mut [Rgba8],
    width: u32,
    height: u32,
    triangle: &ScreenTriangle,
) -> Result<(), RasterError> {
    let [v0, v1, v2] = triangle.vertices;
    for vertex in [v0, v1, v2] {
        if !vertex.x.is_finite() || !vertex.y.is_finite() {
            return Err(RasterError::NonFiniteVertex);
        }
        if vertex.x.abs() > 1_000_000.0 || vertex.y.abs() > 1_000_000.0 {
            return Err(RasterError::CoordinateOutOfRange);
        }
    }

    let area = edge(v0, v1, v2);
    if area.abs() <= f32::EPSILON {
        return Err(RasterError::DegenerateTriangle);
    }

    let (x0, y0, x1, y1) = bounds(width, height, triangle);
    for y in y0..y1 {
        for x in x0..x1 {
            let sample = ScreenVertex {
                x: x as f32 + 0.5,
                y: y as f32 + 0.5,
                color: Rgba8::TRANSPARENT,
            };
            let w0 = edge(v1, v2, sample) / edge(v1, v2, v0);
            let w1 = edge(v2, v0, sample) / edge(v2, v0, v1);
            let w2 = edge(v0, v1, sample) / edge(v0, v1, v2);
            if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                let index = (y * width + x) as usize;
                pixels[index] = Rgba8::new(
                    mix_channel(v0.color.r, v1.color.r, v2.color.r, w0, w1, w2),
                    mix_channel(v0.color.g, v1.color.g, v2.color.g, w0, w1, w2),
                    mix_channel(v0.color.b, v1.color.b, v2.color.b, w0, w1, w2),
                    mix_channel(v0.color.a, v1.color.a, v2.color.a, w0, w1, w2),
                );
            }
        }
    }
    Ok(())
}

fn edge(a: ScreenVertex, b: ScreenVertex, p: ScreenVertex) -> f32 {
    (p.x - a.x) * (b.y - a.y) - (p.y - a.y) * (b.x - a.x)
}

fn bounds(width: u32, height: u32, triangle: &ScreenTriangle) -> (u32, u32, u32, u32) {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for vertex in triangle.vertices {
        min_x = min_x.min(vertex.x);
        min_y = min_y.min(vertex.y);
        max_x = max_x.max(vertex.x);
        max_y = max_y.max(vertex.y);
    }
    let clamp = |value: f32| value.clamp(0.0, i32::MAX as f32) as i64;
    let x0 = (clamp(min_x.floor()) - 1).clamp(0, i64::from(width)) as u32;
    let y0 = (clamp(min_y.floor()) - 1).clamp(0, i64::from(height)) as u32;
    let x1 = (clamp(max_x.ceil()) + 1).clamp(0, i64::from(width)) as u32;
    let y1 = (clamp(max_y.ceil()) + 1).clamp(0, i64::from(height)) as u32;
    (x0, y0, x1, y1)
}

fn mix_channel(c0: u8, c1: u8, c2: u8, w0: f32, w1: f32, w2: f32) -> u8 {
    let value = w0 * f32::from(c0) + w1 * f32::from(c1) + w2 * f32::from(c2);
    value.round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::essl::parse_essl_subset;
    use crate::vm::lower;

    fn vertex(x: f32, y: f32, color: Rgba8) -> ScreenVertex {
        ScreenVertex { x, y, color }
    }

    #[test]
    fn clear_and_triangle_cover_the_sample_center() {
        let mut context = DemoContext::new(8, 8).expect("extent");
        let clear = Rgba8::new(1, 2, 3, 255);
        context.clear(clear);
        context
            .draw_screen_space_triangle(ScreenTriangle {
                vertices: [
                    vertex(2.0, 1.0, Rgba8::new(255, 0, 0, 255)),
                    vertex(6.0, 1.0, Rgba8::new(0, 255, 0, 255)),
                    vertex(2.0, 5.0, Rgba8::new(0, 0, 255, 255)),
                ],
            })
            .expect("triangle");

        let covered = context.pixel(2, 1).expect("inside");
        assert_eq!(covered, Rgba8::new(191, 32, 32, 255));
        assert_eq!(context.pixel(0, 0), Some(clear));
        assert_eq!(context.pixel(7, 7), Some(clear));
        assert!(!context.conformant());
        assert!(context.banner().contains("NOT WEBGL CONFORMANT"));
    }

    #[test]
    fn p0_milestone_triangle_paints_the_interior_and_leaves_the_corner() {
        let mut context = DemoContext::new(720, 480).expect("extent");
        context.clear(P0_MILESTONE_CLEAR);
        let triangle = p0_milestone_triangle(720, 480);
        context
            .draw_screen_space_triangle(triangle)
            .expect("demo triangle");

        assert_eq!(context.pixel(0, 0), Some(P0_MILESTONE_CLEAR));
        assert_eq!(context.pixel(360, 0), Some(P0_MILESTONE_CLEAR));
        let interior = context.pixel(360, 284).expect("centroid pixel");
        assert_ne!(interior, P0_MILESTONE_CLEAR);
        assert!(interior.r > 40 && interior.g > 40 && interior.b > 40);
        assert_eq!(interior.a, 255);
    }

    #[test]
    fn degenerate_and_non_finite_triangles_are_rejected() {
        let mut context = DemoContext::new(4, 4).expect("extent");
        let color = Rgba8::new(9, 9, 9, 255);
        let degenerate = context.draw_screen_space_triangle(ScreenTriangle {
            vertices: [
                vertex(1.0, 1.0, color),
                vertex(2.0, 1.0, color),
                vertex(3.0, 1.0, color),
            ],
        });
        assert_eq!(degenerate, Err(RasterError::DegenerateTriangle));
        let non_finite = context.draw_screen_space_triangle(ScreenTriangle {
            vertices: [
                vertex(f32::NAN, 0.0, color),
                vertex(1.0, 0.0, color),
                vertex(0.0, 1.0, color),
            ],
        });
        assert_eq!(non_finite, Err(RasterError::NonFiniteVertex));
        assert!(context.pixel(0, 0).expect("untouched").r == 0);
    }

    fn unit_clip(x: f32, y: f32, z: f32) -> ClipVertex {
        ClipVertex::new(x, y, z, 1.0, [0.2, 0.7, 0.4, 1.0])
    }

    #[test]
    fn positive_clip_y_is_the_top_of_storage_and_negative_clip_y_is_the_bottom() {
        let clear = Rgba8::new(9, 9, 9, 255);
        let mut top = DemoContext::new(16, 16).expect("extent");
        top.clear(clear);
        let window = top
            .draw_clip_space_triangle([
                unit_clip(-0.3, 0.65, 0.0),
                unit_clip(0.3, 0.65, 0.0),
                unit_clip(0.0, 0.95, 0.0),
            ])
            .expect("top triangle");
        assert!(window.iter().all(|vertex| vertex.y > 8.0));
        assert_ne!(top.pixel(8, 1), Some(clear));
        assert_eq!(top.pixel(8, 14), Some(clear));

        let mut bottom = DemoContext::new(16, 16).expect("extent");
        bottom.clear(clear);
        let window = bottom
            .draw_clip_space_triangle([
                unit_clip(-0.3, -0.95, 0.0),
                unit_clip(0.3, -0.95, 0.0),
                unit_clip(0.0, -0.65, 0.0),
            ])
            .expect("bottom triangle");
        assert!(window.iter().all(|vertex| vertex.y < 8.0));
        assert_eq!(bottom.pixel(8, 1), Some(clear));
        assert_ne!(bottom.pixel(8, 14), Some(clear));
    }

    #[test]
    fn negative_clip_z_rasterizes_and_past_either_plane_does_not() {
        let clear = Rgba8::new(4, 5, 6, 255);
        let covering = |z: f32| {
            [
                ClipVertex::new(-0.8, -0.8, z, 1.0, [1.0, 0.2, 0.2, 1.0]),
                ClipVertex::new(0.8, -0.8, z, 1.0, [0.2, 1.0, 0.3, 1.0]),
                ClipVertex::new(0.0, 0.8, z, 1.0, [0.2, 0.45, 1.0, 1.0]),
            ]
        };

        let mut inside = DemoContext::new(8, 8).expect("extent");
        inside.clear(clear);
        let window = inside
            .draw_clip_space_triangle(covering(-0.5))
            .expect("inside near half");
        assert!(
            window
                .iter()
                .all(|vertex| (vertex.z_ndc + 0.5).abs() < 1.0e-5)
        );
        assert_ne!(inside.pixel(4, 4), Some(clear));

        let mut boundary = DemoContext::new(8, 8).expect("extent");
        boundary.clear(clear);
        let window = boundary
            .draw_clip_space_triangle(covering(-1.0))
            .expect("near plane");
        assert!(
            window
                .iter()
                .all(|vertex| (vertex.z_ndc + 1.0).abs() < 1.0e-5)
        );
        assert_ne!(boundary.pixel(4, 4), Some(clear));

        for z in [-1.5, 1.5] {
            let mut outside = DemoContext::new(8, 8).expect("extent");
            outside.clear(clear);
            let window = outside
                .draw_clip_space_triangle(covering(z))
                .expect("clipped away");
            assert!(window.is_empty());
            assert_eq!(outside.pixel(4, 4), Some(clear));
        }
    }

    #[test]
    fn a_vertex_beyond_the_near_plane_is_clipped_and_the_inside_part_remains() {
        let mut context = DemoContext::new(32, 32).expect("extent");
        let clear = Rgba8::new(1, 1, 1, 255);
        context.clear(clear);
        let window = context
            .draw_clip_space_triangle([
                ClipVertex::new(0.0, 0.5, -0.5, 1.0, [1.0, 0.0, 0.0, 1.0]),
                ClipVertex::new(-0.5, -0.5, -0.5, 1.0, [0.0, 1.0, 0.0, 1.0]),
                ClipVertex::new(0.5, -0.5, -3.0, 1.0, [0.0, 0.0, 1.0, 1.0]),
            ])
            .expect("partial clip");
        assert!(window.len() >= 3);
        assert!(
            window
                .iter()
                .all(|vertex| (-1.0..=1.0).contains(&vertex.z_ndc))
        );
        assert_ne!(context.pixel(16, 9), Some(clear));
    }

    #[test]
    fn shader_outputs_match_the_p0_triangle_and_the_uniform_scales_clip_x() {
        let source = include_str!("../../../examples/webgl2-triangle/p0_vertex.essl");
        let module = lower(&parse_essl_subset(source).expect("parse")).expect("lower");
        let clip = p0_milestone_clip_vertices();
        let bindings: [[(&str, f32); 7]; 3] = std::array::from_fn(|index| {
            let vertex = clip[index];
            [
                ("a_x", vertex.position.x),
                ("a_y", vertex.position.y),
                ("a_z", vertex.position.z),
                ("a_w", vertex.position.w),
                ("a_r", vertex.color[0]),
                ("a_g", vertex.color[1]),
                ("a_b", vertex.color[2]),
            ]
        });
        let vertices = [&bindings[0][..], &bindings[1][..], &bindings[2][..]];

        let mut shaded = DemoContext::new(720, 480).expect("extent");
        let mut direct = DemoContext::new(720, 480).expect("extent");
        shaded.clear(P0_MILESTONE_CLEAR);
        direct.clear(P0_MILESTONE_CLEAR);
        direct
            .draw_screen_space_triangle(p0_milestone_triangle(720, 480))
            .expect("direct");
        let drawn = shaded
            .draw_shader_triangle(&module, "main", &[("u_scale", 1.0)], vertices)
            .expect("shader");
        assert!(drawn.window[0].y > 240.0);
        assert!(drawn.window.iter().all(|vertex| vertex.z_ndc == 0.0));
        assert!(!shaded.conformant());
        for y in 0..shaded.height() {
            for x in 0..shaded.width() {
                assert_eq!(shaded.pixel(x, y), direct.pixel(x, y), "pixel {x},{y}");
            }
        }

        let mut scaled_target = DemoContext::new(720, 480).expect("extent");
        scaled_target.clear(P0_MILESTONE_CLEAR);
        let scaled = scaled_target
            .draw_shader_triangle(&module, "main", &[("u_scale", 0.5)], vertices)
            .expect("scaled");
        assert_eq!(
            scaled.traces[2].output("v_clip_x").expect("x").value_bits,
            Some((0.75f32 * 0.5).to_bits())
        );
        assert_ne!(scaled_target.pixel(360, 80), direct.pixel(360, 80));

        let mut rejected = DemoContext::new(8, 8).expect("extent");
        rejected.clear(P0_MILESTONE_CLEAR);
        let mut outside = bindings;
        outside[0][2].1 = -2.0;
        outside[1][2].1 = -2.0;
        outside[2][2].1 = -2.0;
        let outside_vertices = [&outside[0][..], &outside[1][..], &outside[2][..]];
        let drawn = rejected
            .draw_shader_triangle(&module, "main", &[("u_scale", 1.0)], outside_vertices)
            .expect("outside");
        assert!(drawn.window.is_empty());
        assert_eq!(rejected.pixel(4, 4), Some(P0_MILESTONE_CLEAR));
        assert_eq!(
            drawn.traces[0].output("v_clip_z").expect("z").value(),
            Some(-2.0)
        );
    }
}
