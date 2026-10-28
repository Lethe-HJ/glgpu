//! Screen-space triangle rasterizer for the WebGL2 learning track.
//!
//! Vertices are already in framebuffer pixels. The origin is the top-left, so
//! a picture can sit at the same milestone as the WebGPU P0 triangle demo.
//! WebGL window space is bottom-left and clip Z is `-w..w`; neither convention
//! is applied here. Coverage uses sample centers and barycentric weights.
//! There is no depth test, no `drawArrays`, and no shared raster crate.

use std::error::Error;
use std::fmt;

use crate::framebuffer::{LearningContext, Rgba8};

/// One corner of a learning triangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenVertex {
    pub x: f32,
    pub y: f32,
    pub color: Rgba8,
}

/// A single pre-transformed triangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenTriangle {
    pub vertices: [ScreenVertex; 3],
}

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
}

impl fmt::Display for RasterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteVertex => write!(f, "screen-space vertex is not finite"),
            Self::CoordinateOutOfRange => {
                write!(
                    f,
                    "screen-space vertex is outside the learning coordinate range"
                )
            }
            Self::DegenerateTriangle => write!(f, "screen-space triangle has zero area"),
        }
    }
}

impl Error for RasterError {}

/// Build the colored triangle that corresponds to the P0 demo's three vertices.
///
/// `x = (ndc_x * 0.5 + 0.5) * width` and `y = (0.5 - ndc_y * 0.5) * height`.
/// Y grows downward. A lit rotating cube is a later milestone.
pub fn p0_milestone_triangle(width: u32, height: u32) -> ScreenTriangle {
    let width = width as f32;
    let height = height as f32;
    let vertices = std::array::from_fn(|index| {
        let [x, y] = P0_MILESTONE_NDC[index];
        let [r, g, b] = P0_MILESTONE_COLOR[index];
        ScreenVertex {
            x: (x * 0.5 + 0.5) * width,
            y: (0.5 - y * 0.5) * height,
            color: Rgba8::new(quantize(r), quantize(g), quantize(b), 255),
        }
    });
    ScreenTriangle { vertices }
}

fn quantize(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

impl LearningContext {
    pub fn draw_screen_space_triangle(
        &mut self,
        triangle: ScreenTriangle,
    ) -> Result<(), RasterError> {
        let width = self.width();
        let height = self.height();
        draw(self.pixels_mut(), width, height, &triangle)
    }
}

fn draw(
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

    fn vertex(x: f32, y: f32, color: Rgba8) -> ScreenVertex {
        ScreenVertex { x, y, color }
    }

    #[test]
    fn clear_and_triangle_cover_the_sample_center() {
        let mut context = LearningContext::new(8, 8).expect("extent");
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
        let mut context = LearningContext::new(720, 480).expect("extent");
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
        let mut context = LearningContext::new(4, 4).expect("extent");
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
}
