//! Canvas 2D presentation helpers.
//!
//! Pixel validation is target-independent. The actual Canvas 2D presentation
//! entry point is only available on `wasm32`, where the browser context type
//! exists.

use std::error::Error;
use std::fmt;

/// Errors produced while validating or presenting an RGBA8 frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresentError {
    /// `width * height * 4` cannot be represented as a `usize` on this target.
    DimensionsOverflow { width: u32, height: u32 },
    /// The supplied byte slice is not exactly one tightly packed RGBA8 frame.
    InvalidPixelLength { expected: usize, actual: usize },
    /// A browser Canvas/ImageData operation failed.
    CanvasOperation {
        operation: &'static str,
        message: String,
    },
}

impl fmt::Display for PresentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DimensionsOverflow { width, height } => write!(
                f,
                "RGBA8 byte length overflows this target for {width}x{height} pixels"
            ),
            Self::InvalidPixelLength { expected, actual } => write!(
                f,
                "invalid RGBA8 pixel length: expected exactly {expected} bytes, got {actual}"
            ),
            Self::CanvasOperation { operation, message } => {
                write!(f, "Canvas 2D operation {operation} failed: {message}")
            }
        }
    }
}

impl Error for PresentError {}

/// Return the exact number of bytes required for a tightly packed RGBA8 frame.
pub fn rgba8_byte_len(width: u32, height: u32) -> Result<usize, PresentError> {
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(PresentError::DimensionsOverflow { width, height })
}

/// Strictly validate that `pixels` contains one tightly packed RGBA8 frame.
///
/// This helper is available on native targets so frame producers can be tested
/// without pretending that a browser Canvas context exists.
pub fn validate_rgba8_pixels(width: u32, height: u32, pixels: &[u8]) -> Result<(), PresentError> {
    let expected = rgba8_byte_len(width, height)?;
    if pixels.len() != expected {
        return Err(PresentError::InvalidPixelLength {
            expected,
            actual: pixels.len(),
        });
    }

    Ok(())
}

/// Present one tightly packed RGBA8 frame through Canvas 2D.
///
/// The byte slice is validated before any browser API is called. This function
/// is intentionally only exposed on `wasm32`; native callers should use
/// [`validate_rgba8_pixels`].
#[cfg(target_arch = "wasm32")]
pub fn present_rgba8(
    context: &web_sys::CanvasRenderingContext2d,
    width: u32,
    height: u32,
    pixels: &[u8],
) -> Result<(), PresentError> {
    validate_rgba8_pixels(width, height, pixels)?;

    // A Wasm threads build stores `pixels` in a SharedArrayBuffer. Browsers
    // deliberately reject shared typed arrays in the ImageData constructor,
    // so MT must copy into an ordinary JS ArrayBuffer at this boundary.
    #[cfg(target_feature = "atomics")]
    let image_data = {
        let presentation_pixels = js_sys::Uint8ClampedArray::from(pixels);
        web_sys::ImageData::new_with_js_u8_clamped_array_and_sh(&presentation_pixels, width, height)
            .map_err(|error| canvas_error("ImageData::new", error))?
    };

    #[cfg(not(target_feature = "atomics"))]
    let image_data = web_sys::ImageData::new_with_u8_clamped_array_and_sh(
        wasm_bindgen::Clamped(pixels),
        width,
        height,
    )
    .map_err(|error| canvas_error("ImageData::new", error))?;

    context
        .put_image_data(&image_data, 0.0, 0.0)
        .map_err(|error| canvas_error("putImageData", error))
}

#[cfg(target_arch = "wasm32")]
fn canvas_error(operation: &'static str, error: wasm_bindgen::JsValue) -> PresentError {
    let message = error.as_string().unwrap_or_else(|| format!("{error:?}"));
    PresentError::CanvasOperation { operation, message }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_exact_rgba8_length() {
        let pixels = [0_u8; 3 * 2 * 4];
        assert_eq!(validate_rgba8_pixels(3, 2, &pixels), Ok(()));
    }

    #[test]
    fn accepts_empty_zero_sized_frame_for_validation() {
        assert_eq!(validate_rgba8_pixels(0, 8, &[]), Ok(()));
        assert_eq!(validate_rgba8_pixels(8, 0, &[]), Ok(()));
    }

    #[test]
    fn rejects_short_and_long_buffers() {
        assert_eq!(
            validate_rgba8_pixels(2, 2, &[0; 15]),
            Err(PresentError::InvalidPixelLength {
                expected: 16,
                actual: 15,
            })
        );
        assert_eq!(
            validate_rgba8_pixels(2, 2, &[0; 17]),
            Err(PresentError::InvalidPixelLength {
                expected: 16,
                actual: 17,
            })
        );
    }

    #[test]
    fn rejects_dimensions_that_overflow_rgba8_length() {
        assert_eq!(
            rgba8_byte_len(u32::MAX, u32::MAX),
            Err(PresentError::DimensionsOverflow {
                width: u32::MAX,
                height: u32::MAX,
            })
        );
    }

    #[test]
    fn errors_have_actionable_messages() {
        let error = validate_rgba8_pixels(1, 1, &[]).unwrap_err();
        assert_eq!(
            error.to_string(),
            "invalid RGBA8 pixel length: expected exactly 4 bytes, got 0"
        );
    }
}
