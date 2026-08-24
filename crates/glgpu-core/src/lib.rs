//! DOM-free data types shared by the GLGPU CPU pipeline.

#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;

/// A two-dimensional pixel extent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Extent2d {
    pub width: u32,
    pub height: u32,
}

impl Extent2d {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Returns the number of pixels, failing if it cannot be represented by
    /// the current platform's `usize`.
    pub fn pixel_count(self) -> Result<usize, CoreError> {
        let count = u64::from(self.width)
            .checked_mul(u64::from(self.height))
            .ok_or(CoreError::SizeOverflow)?;
        usize::try_from(count).map_err(|_| CoreError::SizeOverflow)
    }

    pub const fn contains(self, pixel: PixelCoord) -> bool {
        pixel.x < self.width && pixel.y < self.height
    }
}

/// Integer coordinates in the framebuffer, with `(0, 0)` at the top-left.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct PixelCoord {
    pub x: u32,
    pub y: u32,
}

impl PixelCoord {
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
}

/// An unpremultiplied 8-bit RGBA color.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba8 {
    pub const TRANSPARENT: Self = Self::new(0, 0, 0, 0);
    pub const BLACK: Self = Self::new(0, 0, 0, 255);
    pub const WHITE: Self = Self::new(255, 255, 255, 255);

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

impl From<[u8; 4]> for Rgba8 {
    fn from(value: [u8; 4]) -> Self {
        Self::new(value[0], value[1], value[2], value[3])
    }
}

impl From<Rgba8> for [u8; 4] {
    fn from(value: Rgba8) -> Self {
        value.to_array()
    }
}

/// A vertex whose position is already in framebuffer pixel coordinates.
///
/// `x` and `y` use a top-left origin, and integer coordinates denote pixel
/// corners. Pixel centers are sampled at `(x + 0.5, y + 0.5)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenVertex {
    pub x: f32,
    pub y: f32,
    pub depth: f32,
    pub color: Rgba8,
}

impl ScreenVertex {
    pub const fn new(x: f32, y: f32, depth: f32, color: Rgba8) -> Self {
        Self { x, y, depth, color }
    }
}

/// A screen-space triangle. Input slice order is the stable primitive order;
/// `primitive_id` is diagnostic metadata and need not be unique.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Triangle {
    pub primitive_id: u64,
    pub vertices: [ScreenVertex; 3],
}

impl Triangle {
    pub const fn new(primitive_id: u64, vertices: [ScreenVertex; 3]) -> Self {
        Self {
            primitive_id,
            vertices,
        }
    }

    pub const fn from_vertices(
        primitive_id: u64,
        v0: ScreenVertex,
        v1: ScreenVertex,
        v2: ScreenVertex,
    ) -> Self {
        Self::new(primitive_id, [v0, v1, v2])
    }
}

/// Owned RGBA8 and depth planes.
#[derive(Debug, Clone, PartialEq)]
pub struct Framebuffer {
    extent: Extent2d,
    color: Vec<Rgba8>,
    depth: Vec<f32>,
}

impl Framebuffer {
    pub fn new(extent: Extent2d, clear_color: Rgba8, clear_depth: f32) -> Result<Self, CoreError> {
        let len = extent.pixel_count()?;
        let mut color = Vec::new();
        color
            .try_reserve_exact(len)
            .map_err(|_| CoreError::AllocationFailed)?;
        color.resize(len, clear_color);

        let mut depth = Vec::new();
        depth
            .try_reserve_exact(len)
            .map_err(|_| CoreError::AllocationFailed)?;
        depth.resize(len, clear_depth);

        Ok(Self {
            extent,
            color,
            depth,
        })
    }

    pub fn from_planes(
        extent: Extent2d,
        color: Vec<Rgba8>,
        depth: Vec<f32>,
    ) -> Result<Self, CoreError> {
        let expected = extent.pixel_count()?;
        if color.len() != expected || depth.len() != expected {
            return Err(CoreError::PlaneLengthMismatch {
                expected,
                color: color.len(),
                depth: depth.len(),
            });
        }
        Ok(Self {
            extent,
            color,
            depth,
        })
    }

    pub const fn extent(&self) -> Extent2d {
        self.extent
    }

    pub fn color(&self) -> &[Rgba8] {
        &self.color
    }

    pub fn depth(&self) -> &[f32] {
        &self.depth
    }

    pub fn color_at(&self, pixel: PixelCoord) -> Option<Rgba8> {
        self.index(pixel).map(|index| self.color[index])
    }

    pub fn depth_at(&self, pixel: PixelCoord) -> Option<f32> {
        self.index(pixel).map(|index| self.depth[index])
    }

    pub fn clear(&mut self, color: Rgba8, depth: f32) {
        self.color.fill(color);
        self.depth.fill(depth);
    }

    /// A stable FNV-1a hash of dimensions, color bytes, and exact f32 depth
    /// bit patterns. It is intended for deterministic regression tests, not
    /// for cryptographic or content-addressing use.
    pub fn stable_hash64(&self) -> u64 {
        const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        fn add_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
            const PRIME: u64 = 0x0000_0100_0000_01b3;
            for byte in bytes {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(PRIME);
            }
            hash
        }

        let mut hash = OFFSET;
        hash = add_bytes(hash, &self.extent.width.to_le_bytes());
        hash = add_bytes(hash, &self.extent.height.to_le_bytes());
        for pixel in &self.color {
            hash = add_bytes(hash, &pixel.to_array());
        }
        for depth in &self.depth {
            hash = add_bytes(hash, &depth.to_bits().to_le_bytes());
        }
        hash
    }

    fn index(&self, pixel: PixelCoord) -> Option<usize> {
        if !self.extent.contains(pixel) {
            return None;
        }
        let row = usize::try_from(pixel.y).ok()?;
        let width = usize::try_from(self.extent.width).ok()?;
        let column = usize::try_from(pixel.x).ok()?;
        row.checked_mul(width)?.checked_add(column)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    SizeOverflow,
    AllocationFailed,
    PlaneLengthMismatch {
        expected: usize,
        color: usize,
        depth: usize,
    },
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SizeOverflow => f.write_str("framebuffer dimensions overflow usize"),
            Self::AllocationFailed => f.write_str("framebuffer allocation failed"),
            Self::PlaneLengthMismatch {
                expected,
                color,
                depth,
            } => write!(
                f,
                "framebuffer plane length mismatch: expected {expected}, color {color}, depth {depth}"
            ),
        }
    }
}

impl Error for CoreError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framebuffer_planes_and_coordinates_are_consistent() {
        let extent = Extent2d::new(3, 2);
        let framebuffer = Framebuffer::new(extent, Rgba8::new(1, 2, 3, 4), 0.75).unwrap();

        assert_eq!(framebuffer.color().len(), 6);
        assert_eq!(framebuffer.depth().len(), 6);
        assert_eq!(
            framebuffer.color_at(PixelCoord::new(2, 1)),
            Some(Rgba8::new(1, 2, 3, 4))
        );
        assert_eq!(framebuffer.depth_at(PixelCoord::new(2, 1)), Some(0.75));
        assert_eq!(framebuffer.color_at(PixelCoord::new(3, 1)), None);
    }

    #[test]
    fn from_planes_rejects_mismatched_lengths() {
        let error =
            Framebuffer::from_planes(Extent2d::new(2, 2), vec![Rgba8::BLACK; 3], vec![1.0; 4])
                .unwrap_err();
        assert_eq!(
            error,
            CoreError::PlaneLengthMismatch {
                expected: 4,
                color: 3,
                depth: 4,
            }
        );
    }

    #[test]
    fn stable_hash_includes_color_and_depth() {
        let extent = Extent2d::new(1, 1);
        let a = Framebuffer::new(extent, Rgba8::BLACK, 1.0).unwrap();
        let b = Framebuffer::new(extent, Rgba8::WHITE, 1.0).unwrap();
        let c = Framebuffer::new(extent, Rgba8::BLACK, 0.5).unwrap();

        assert_ne!(a.stable_hash64(), b.stable_hash64());
        assert_ne!(a.stable_hash64(), c.stable_hash64());
        assert_eq!(a.stable_hash64(), a.clone().stable_hash64());
    }
}
