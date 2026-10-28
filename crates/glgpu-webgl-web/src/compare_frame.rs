#![cfg_attr(not(any(test, target_arch = "wasm32")), allow(dead_code))]

use std::fmt;

use glgpu_webgl::{ClipVertex, LearningContext, Rgba8, WebGl2LearningTrack};

#[derive(Debug, PartialEq)]
pub enum CompareFrameError {
    ExtentOutOfRange { width: u32, height: u32 },
    ClearLength { actual: usize },
    VertexLength { actual: usize },
    NonFiniteVertex { index: usize },
    Raster(String),
}

impl fmt::Display for CompareFrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExtentOutOfRange { width, height } => {
                write!(f, "compare extent {width}x{height} is outside 1..=4096")
            }
            Self::ClearLength { actual } => {
                write!(f, "compare clear must be 4 bytes, got {actual}")
            }
            Self::VertexLength { actual } => {
                write!(f, "compare vertices must be 24 floats, got {actual}")
            }
            Self::NonFiniteVertex { index } => {
                write!(f, "compare vertex float {index} is not finite")
            }
            Self::Raster(message) => write!(f, "compare raster failed: {message}"),
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn storage_position(ndc_x: f32, ndc_y: f32, width: u32, height: u32) -> (f32, f32) {
    let window_x = (ndc_x + 1.0) * 0.5 * width as f32;
    let window_y = (ndc_y + 1.0) * 0.5 * height as f32;
    (window_x, height as f32 - window_y)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn quantize_unorm8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn validate(
    width: u32,
    height: u32,
    clear: &[u8],
    vertices: &[f32],
) -> Result<(), CompareFrameError> {
    if !(1..=4096).contains(&width) || !(1..=4096).contains(&height) {
        return Err(CompareFrameError::ExtentOutOfRange { width, height });
    }
    if clear.len() != 4 {
        return Err(CompareFrameError::ClearLength {
            actual: clear.len(),
        });
    }
    if vertices.len() != 24 {
        return Err(CompareFrameError::VertexLength {
            actual: vertices.len(),
        });
    }
    if let Some(index) = vertices.iter().position(|value| !value.is_finite()) {
        return Err(CompareFrameError::NonFiniteVertex { index });
    }
    Ok(())
}

pub fn compare_frame_rgba8(
    width: u32,
    height: u32,
    clear: &[u8],
    vertices: &[f32],
) -> Result<Vec<u8>, CompareFrameError> {
    validate(width, height, clear, vertices)?;
    let mut context: LearningContext = WebGl2LearningTrack::new()
        .create_context(width, height)
        .map_err(|error| CompareFrameError::Raster(error.to_string()))?;
    context.clear(Rgba8::new(clear[0], clear[1], clear[2], clear[3]));
    let clips = [0, 1, 2].map(|index| {
        let offset = index * 8;
        ClipVertex::new(
            vertices[offset],
            vertices[offset + 1],
            vertices[offset + 2],
            vertices[offset + 3],
            [
                vertices[offset + 4],
                vertices[offset + 5],
                vertices[offset + 6],
                vertices[offset + 7],
            ],
        )
    });
    context
        .draw_clip_space_triangle(clips)
        .map_err(|error| CompareFrameError::Raster(error.to_string()))?;
    Ok(context.to_rgba8_bytes())
}

#[cfg(test)]
mod tests {
    use super::compare_frame_rgba8;

    fn locked_vertices() -> [f32; 24] {
        [
            0.0, 0.75, 0.0, 1.0, 1.0, 0.2, 0.2, 1.0, -0.75, -0.65, 0.0, 1.0, 0.2, 1.0, 0.3, 1.0,
            0.75, -0.65, 0.0, 1.0, 0.2, 0.45, 1.0, 1.0,
        ]
    }

    fn pixel(frame: &[u8], x: u32, y: u32) -> [u8; 4] {
        let index = ((y * 320 + x) * 4) as usize;
        frame[index..index + 4].try_into().unwrap()
    }

    #[test]
    fn locked_scene_is_320_by_240_and_leaves_the_corner_clear() {
        let frame = compare_frame_rgba8(320, 240, &[13, 18, 30, 255], &locked_vertices()).unwrap();
        assert_eq!(frame.len(), 307_200);
        assert_eq!(pixel(&frame, 0, 0), [13, 18, 30, 255]);
        assert_ne!(pixel(&frame, 160, 142), [13, 18, 30, 255]);
    }

    #[test]
    fn storage_anchors_match_the_spec() {
        assert_eq!(super::storage_position(0.0, 0.75, 320, 240), (160.0, 30.0));
        assert_eq!(
            super::storage_position(-0.75, -0.65, 320, 240),
            (40.0, 198.0)
        );
        assert_eq!(
            super::storage_position(0.75, -0.65, 320, 240),
            (280.0, 198.0)
        );
        assert_eq!(super::quantize_unorm8(0.2), 51);
        assert_eq!(super::quantize_unorm8(0.3), 77);
        assert_eq!(super::quantize_unorm8(0.45), 115);
        assert_eq!(super::quantize_unorm8(1.0), 255);
    }

    #[test]
    fn rejects_bad_extent_clear_and_vertices() {
        let vertices = locked_vertices();
        let error = compare_frame_rgba8(0, 240, &[13, 18, 30, 255], &vertices).unwrap_err();
        assert!(error.to_string().contains("outside 1..=4096"));
        let error = compare_frame_rgba8(4097, 240, &[13, 18, 30, 255], &vertices).unwrap_err();
        assert!(error.to_string().contains("outside 1..=4096"));
        let error = compare_frame_rgba8(320, 240, &[13, 18, 30], &vertices).unwrap_err();
        assert!(error.to_string().contains("4 bytes"));
        let error = compare_frame_rgba8(320, 240, &[13, 18, 30, 255], &vertices[..23]).unwrap_err();
        assert!(error.to_string().contains("24 floats"));
        let mut non_finite = vertices;
        non_finite[1] = f32::NAN;
        let error = compare_frame_rgba8(320, 240, &[13, 18, 30, 255], &non_finite).unwrap_err();
        assert!(error.to_string().contains("not finite"));
    }
}
