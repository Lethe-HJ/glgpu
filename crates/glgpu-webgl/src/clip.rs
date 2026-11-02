//! WebGL clip volume and window coordinates for webgl-mock.
//!
//! Clip space uses the WebGL rule `-w <= x, y, z <= w`, so the near plane is
//! `z = -w`. After the perspective divide, window x grows right and window y
//! grows up from the bottom-left corner. This module does not call native
//! WebGL and does not use the webgpu-mock crates.
//!
//! The CPU framebuffer still stores row 0 as the top image row. That
//! layout is transitional, so a P0-parity picture matches the WebGPU demo
//! without a present-time flip. Convert with [`storage_y_from_window_y`].

use std::error::Error;
use std::fmt;

use crate::framebuffer::{Rgba8, quantize_unorm};

/// Clip-space position. The visible volume is `-w <= x, y, z <= w` with `w > 0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipPosition {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

/// One clip-space vertex plus a linear color in `0..1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipVertex {
    pub position: ClipPosition,
    pub color: [f32; 4],
}

impl ClipVertex {
    pub const fn new(x: f32, y: f32, z: f32, w: f32, color: [f32; 4]) -> Self {
        Self {
            position: ClipPosition { x, y, z, w },
            color,
        }
    }
}

/// A point that survived the clip test and the perspective divide.
///
/// `y == 0` is the bottom edge of the window. `z_ndc` is `z / w` and is not
/// written to a depth buffer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowPoint {
    pub x: f32,
    pub y: f32,
    pub z_ndc: f32,
}

/// A projected vertex. `color` is quantized at the vertex.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowVertex {
    pub x: f32,
    pub y: f32,
    pub z_ndc: f32,
    pub color: Rgba8,
}

/// Why a clip-space position cannot be projected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClipError {
    /// A coordinate or color component was NaN or infinite.
    NonFinite,
    /// The point is outside `-w <= x, y, z <= w` or has `w <= 0`.
    OutsideClipVolume,
    /// `w <= 0` after clipping, so there is no window position.
    Unprojected,
}

impl fmt::Display for ClipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite => write!(f, "clip-space value is not finite"),
            Self::OutsideClipVolume => {
                write!(f, "clip-space position is outside -w <= x, y, z <= w")
            }
            Self::Unprojected => write!(f, "clip-space w is not positive"),
        }
    }
}

impl Error for ClipError {}

/// Result of clipping one triangle to the WebGL volume.
#[derive(Clone, Debug, PartialEq)]
pub enum ClipOutcome {
    /// All three vertices were inside. Positions are unchanged.
    Inside([ClipVertex; 3]),
    /// At least one vertex was outside. Empty means the triangle was fully outside.
    Clipped(Vec<ClipVertex>),
}

#[derive(Clone, Copy)]
enum Plane {
    Left,
    Right,
    Bottom,
    Top,
    Near,
    Far,
}

const PLANES: [Plane; 6] = [
    Plane::Left,
    Plane::Right,
    Plane::Bottom,
    Plane::Top,
    Plane::Near,
    Plane::Far,
];

/// Project a point that is already inside the WebGL clip volume.
///
/// Window x is `(x/w + 1) * 0.5 * width`. Window y uses the same scale and
/// grows upward.
pub fn clip_point_to_window(
    position: ClipPosition,
    width: u32,
    height: u32,
) -> Result<WindowPoint, ClipError> {
    if !position_finite(position) {
        return Err(ClipError::NonFinite);
    }
    if !inside_clip_volume(position) {
        return Err(ClipError::OutsideClipVolume);
    }
    Ok(project_point(position, width, height))
}

/// Clip one triangle. A fully inside triangle is returned unchanged.
pub fn clip_triangle(triangle: [ClipVertex; 3]) -> Result<ClipOutcome, ClipError> {
    for vertex in triangle {
        if !vertex_finite(vertex) {
            return Err(ClipError::NonFinite);
        }
    }
    if triangle
        .iter()
        .all(|vertex| inside_clip_volume(vertex.position))
    {
        return Ok(ClipOutcome::Inside(triangle));
    }
    let mut polygon = triangle.to_vec();
    for plane in PLANES {
        if polygon.is_empty() {
            break;
        }
        polygon = clip_against(polygon, plane);
    }
    Ok(ClipOutcome::Clipped(polygon))
}

/// Storage y for the transitional top-left framebuffer.
///
/// `window_y` is bottom-left. Storage row 0 is the top image row, so present
/// can upload row 0 to the top of an image without flipping again.
pub fn storage_y_from_window_y(window_y: f32, height: u32) -> f32 {
    height as f32 - window_y
}

pub(crate) fn project_window(
    vertex: ClipVertex,
    width: u32,
    height: u32,
) -> Result<WindowVertex, ClipError> {
    if !vertex_finite(vertex) {
        return Err(ClipError::NonFinite);
    }
    if vertex.position.w <= 0.0 {
        return Err(ClipError::Unprojected);
    }
    let point = project_point(vertex.position, width, height);
    Ok(WindowVertex {
        x: point.x,
        y: point.y,
        z_ndc: point.z_ndc,
        color: Rgba8::new(
            quantize_unorm(vertex.color[0]),
            quantize_unorm(vertex.color[1]),
            quantize_unorm(vertex.color[2]),
            quantize_unorm(vertex.color[3]),
        ),
    })
}

fn project_point(position: ClipPosition, width: u32, height: u32) -> WindowPoint {
    let inv_w = 1.0 / position.w;
    let ndc_x = position.x * inv_w;
    let ndc_y = position.y * inv_w;
    let ndc_z = position.z * inv_w;
    WindowPoint {
        x: (ndc_x + 1.0) * 0.5 * width as f32,
        y: (ndc_y + 1.0) * 0.5 * height as f32,
        z_ndc: ndc_z,
    }
}

fn inside_clip_volume(position: ClipPosition) -> bool {
    position.w > 0.0 && PLANES.iter().all(|plane| distance(position, *plane) >= 0.0)
}

fn distance(position: ClipPosition, plane: Plane) -> f32 {
    match plane {
        Plane::Left => position.w + position.x,
        Plane::Right => position.w - position.x,
        Plane::Bottom => position.w + position.y,
        Plane::Top => position.w - position.y,
        Plane::Near => position.w + position.z,
        Plane::Far => position.w - position.z,
    }
}

fn clip_against(input: Vec<ClipVertex>, plane: Plane) -> Vec<ClipVertex> {
    let mut output = Vec::new();
    let Some(last) = input.last().copied() else {
        return output;
    };
    let mut previous = last;
    let mut previous_inside = distance(previous.position, plane) >= 0.0;
    for current in input {
        let current_inside = distance(current.position, plane) >= 0.0;
        if current_inside {
            if !previous_inside {
                output.push(interpolate(previous, current, plane));
            }
            output.push(current);
        } else if previous_inside {
            output.push(interpolate(previous, current, plane));
        }
        previous = current;
        previous_inside = current_inside;
    }
    output
}

fn interpolate(start: ClipVertex, end: ClipVertex, plane: Plane) -> ClipVertex {
    let start_distance = distance(start.position, plane);
    let end_distance = distance(end.position, plane);
    let denom = start_distance - end_distance;
    let t = if denom.abs() <= f32::EPSILON {
        0.0
    } else {
        start_distance / denom
    };
    let lerp = |from: f32, to: f32| from + (to - from) * t;
    ClipVertex {
        position: ClipPosition {
            x: lerp(start.position.x, end.position.x),
            y: lerp(start.position.y, end.position.y),
            z: lerp(start.position.z, end.position.z),
            w: lerp(start.position.w, end.position.w),
        },
        color: [
            lerp(start.color[0], end.color[0]),
            lerp(start.color[1], end.color[1]),
            lerp(start.color[2], end.color[2]),
            lerp(start.color[3], end.color[3]),
        ],
    }
}

fn position_finite(position: ClipPosition) -> bool {
    position.x.is_finite()
        && position.y.is_finite()
        && position.z.is_finite()
        && position.w.is_finite()
}

fn vertex_finite(vertex: ClipVertex) -> bool {
    position_finite(vertex.position) && vertex.color.iter().all(|channel| channel.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position(x: f32, y: f32, z: f32, w: f32) -> ClipPosition {
        ClipPosition { x, y, z, w }
    }

    #[test]
    fn window_origin_is_bottom_left_and_divides_by_w() {
        let top = clip_point_to_window(position(0.0, 1.0, 0.0, 1.0), 10, 20).expect("top");
        assert_eq!(top.x, 5.0);
        assert_eq!(top.y, 20.0);
        assert_eq!(top.z_ndc, 0.0);

        let bottom = clip_point_to_window(position(0.0, -1.0, 0.0, 1.0), 10, 20).expect("bottom");
        assert_eq!(bottom.y, 0.0);
        let right = clip_point_to_window(position(1.0, 0.0, 0.0, 1.0), 10, 20).expect("right");
        assert_eq!(right.x, 10.0);

        let scaled = clip_point_to_window(position(0.0, 0.5, -1.0, 2.0), 8, 8).expect("scaled");
        assert_eq!(scaled.x, 4.0);
        assert_eq!(scaled.y, 5.0);
        assert_eq!(scaled.z_ndc, -0.5);
        assert_eq!(storage_y_from_window_y(top.y, 20), 0.0);
        assert_eq!(storage_y_from_window_y(bottom.y, 20), 20.0);
    }

    #[test]
    fn clip_volume_is_closed_on_x_y_and_z() {
        let inside = [
            position(-1.0, 0.0, 0.0, 1.0),
            position(1.0, 0.0, 0.0, 1.0),
            position(0.0, -1.0, 0.0, 1.0),
            position(0.0, 1.0, 0.0, 1.0),
            position(0.0, 0.0, -1.0, 1.0),
            position(0.0, 0.0, 1.0, 1.0),
            position(0.0, 0.0, -0.5, 1.0),
            position(-2.0, 0.0, -1.5, 2.0),
        ];
        for point in inside {
            assert!(
                clip_point_to_window(point, 2, 2).is_ok(),
                "inside {point:?}"
            );
        }

        let outside = [
            position(-1.1, 0.0, 0.0, 1.0),
            position(1.1, 0.0, 0.0, 1.0),
            position(0.0, -1.1, 0.0, 1.0),
            position(0.0, 1.1, 0.0, 1.0),
            position(0.0, 0.0, -1.1, 1.0),
            position(0.0, 0.0, 1.1, 1.0),
            position(0.0, 0.0, 0.0, 0.0),
            position(0.0, 0.0, 0.0, -1.0),
        ];
        for point in outside {
            assert_eq!(
                clip_point_to_window(point, 2, 2),
                Err(ClipError::OutsideClipVolume),
                "outside {point:?}"
            );
        }
        assert_eq!(
            clip_point_to_window(position(f32::NAN, 0.0, 0.0, 1.0), 2, 2),
            Err(ClipError::NonFinite)
        );
    }

    #[test]
    fn inside_triangle_is_preserved_and_outside_triangle_is_empty() {
        let triangle = [
            ClipVertex::new(-0.2, -0.2, -0.5, 1.0, [0.0, 0.0, 0.0, 1.0]),
            ClipVertex::new(0.2, -0.2, -0.5, 1.0, [0.0, 0.0, 0.0, 1.0]),
            ClipVertex::new(0.0, 0.4, -0.5, 1.0, [0.0, 0.0, 0.0, 1.0]),
        ];
        assert_eq!(clip_triangle(triangle), Ok(ClipOutcome::Inside(triangle)));

        let outside = triangle.map(|mut vertex| {
            vertex.position.z = -1.5;
            vertex
        });
        assert_eq!(clip_triangle(outside), Ok(ClipOutcome::Clipped(Vec::new())));
    }

    #[test]
    fn a_vertex_past_the_right_plane_is_clipped_with_color() {
        let triangle = [
            ClipVertex::new(2.0, 0.0, 0.0, 1.0, [1.0, 0.0, 0.0, 1.0]),
            ClipVertex::new(-0.5, -0.5, 0.0, 1.0, [0.0, 0.0, 0.0, 1.0]),
            ClipVertex::new(-0.5, 0.5, 0.0, 1.0, [0.0, 0.0, 0.0, 1.0]),
        ];
        let ClipOutcome::Clipped(vertices) = clip_triangle(triangle).expect("clip") else {
            panic!("the x = 2 vertex is outside");
        };
        assert!(vertices.len() >= 3);
        let boundary = vertices
            .iter()
            .filter(|vertex| (vertex.position.x - 1.0).abs() < 1.0e-4)
            .count();
        assert!(boundary >= 2);
        assert!(
            vertices
                .iter()
                .any(|vertex| (vertex.color[0] - 0.6).abs() < 1.0e-4)
        );
        for vertex in vertices {
            assert!(clip_point_to_window(vertex.position, 8, 8).is_ok());
        }
    }
}
