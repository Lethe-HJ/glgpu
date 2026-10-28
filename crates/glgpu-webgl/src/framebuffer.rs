//! CPU RGBA8 framebuffer owned by webgl-mock.
//!
//! This buffer is not a native WebGL drawing buffer. Nothing here calls a
//! browser GPU API, and the type does not depend on the webgpu-mock crates.
//!
//! Row 0 is the top image row. That storage order is transitional so a
//! P0-parity picture matches the WebGPU demo. WebGL window y is bottom-left
//! and is converted before pixels are written.

use std::error::Error;
use std::fmt;

use crate::CONFORMANCE_BANNER;

/// Unpremultiplied 8-bit color stored by the CPU framebuffer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba8 {
    pub const TRANSPARENT: Self = Self::new(0, 0, 0, 0);

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

pub(crate) fn quantize_unorm(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Failure while creating the CPU framebuffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextError {
    /// Width or height was zero.
    EmptyExtent,
    /// `width * height` does not fit in `usize`.
    ExtentOverflow { width: u32, height: u32 },
}

impl fmt::Display for ContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyExtent => write!(f, "CPU framebuffer extent must be non-zero"),
            Self::ExtentOverflow { width, height } => {
                write!(f, "CPU framebuffer {width}x{height} overflows usize")
            }
        }
    }
}

impl Error for ContextError {}

/// CPU demo context for webgl-mock.
///
/// The triangle page uses this type. It is not a native WebGL context.
///
/// The banner stays `NOT WEBGL CONFORMANT`. This object is not a
/// `WebGL2RenderingContext`, has no mutable GL state machine, and cannot be
/// obtained from `canvas.getContext`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DemoContext {
    width: u32,
    height: u32,
    pixels: Vec<Rgba8>,
}

impl DemoContext {
    pub fn new(width: u32, height: u32) -> Result<Self, ContextError> {
        if width == 0 || height == 0 {
            return Err(ContextError::EmptyExtent);
        }
        let count = u64::from(width)
            .checked_mul(u64::from(height))
            .ok_or(ContextError::ExtentOverflow { width, height })?;
        let count =
            usize::try_from(count).map_err(|_| ContextError::ExtentOverflow { width, height })?;
        Ok(Self {
            width,
            height,
            pixels: vec![Rgba8::TRANSPARENT; count],
        })
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub const fn banner(&self) -> &'static str {
        CONFORMANCE_BANNER
    }

    pub const fn conformant(&self) -> bool {
        false
    }

    /// Replace every pixel. This is the CPU clear, not `gl.clear`.
    pub fn clear(&mut self, color: Rgba8) {
        self.pixels.fill(color);
    }

    /// Storage coordinate. `y == 0` is the top image row, not the WebGL window origin.
    pub fn pixel(&self, x: u32, y: u32) -> Option<Rgba8> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = (u64::from(y) * u64::from(self.width) + u64::from(x)) as usize;
        self.pixels.get(index).copied()
    }

    /// Tightly packed RGBA8, row 0 at the top.
    ///
    /// Canvas 2D `putImageData` can consume this order without flipping y.
    pub fn to_rgba8_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.pixels.len().saturating_mul(4));
        for pixel in &self.pixels {
            bytes.extend_from_slice(&[pixel.r, pixel.g, pixel.b, pixel.a]);
        }
        bytes
    }

    /// FNV-1a of width, height, and tightly packed RGBA8 bytes.
    ///
    /// This framebuffer has no depth plane, so the value is not comparable to
    /// the WebGPU framebuffer hash. The algorithm is local to this crate.
    pub fn stable_hash64(&self) -> u64 {
        const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0000_0100_0000_01b3;
        let mut hash = OFFSET;
        let mut mix = |bytes: &[u8]| {
            for byte in bytes {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(PRIME);
            }
        };
        mix(&self.width.to_le_bytes());
        mix(&self.height.to_le_bytes());
        for pixel in &self.pixels {
            mix(&[pixel.r, pixel.g, pixel.b, pixel.a]);
        }
        hash
    }

    pub(crate) fn pixels_mut(&mut self) -> &mut [Rgba8] {
        &mut self.pixels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba8_bytes_and_hash_follow_the_stored_pixels() {
        let mut context = DemoContext::new(2, 1).expect("extent");
        context.clear(Rgba8::new(13, 18, 30, 255));

        assert_eq!(
            context.to_rgba8_bytes(),
            vec![13, 18, 30, 255, 13, 18, 30, 255]
        );
        let hash = context.stable_hash64();
        assert_eq!(hash, context.clone().stable_hash64());

        context.clear(Rgba8::new(0, 0, 0, 255));
        assert_ne!(hash, context.stable_hash64());
        assert_eq!(context.to_rgba8_bytes().len(), 8);
    }
}
