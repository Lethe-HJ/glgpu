//! Minimal, DOM-free tracing records for the P0 CPU rasterizer.

#![forbid(unsafe_code)]

use glgpu_core::{PixelCoord, Rgba8};

/// The observable outcome of one primitive at the selected pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelEventOutcome {
    OutsideCoverage,
    DepthRejected,
    Written,
}

/// One triangle's contribution attempt at a selected pixel.
#[derive(Debug, Clone, PartialEq)]
pub struct PixelHistoryEvent {
    /// Stable position of the triangle in the submitted slice.
    pub triangle_index: u32,
    /// Caller-supplied diagnostic identifier.
    pub primitive_id: u64,
    pub outcome: PixelEventOutcome,
    /// Edge-function barycentrics in vertex order. `None` means the sample
    /// failed coverage before interpolation.
    pub barycentric: Option<[f32; 3]>,
    pub candidate_depth: Option<f32>,
    pub previous_depth: f32,
    pub color_before: Rgba8,
    pub color_after: Rgba8,
}

/// Ordered raster-stage history for a single selected framebuffer pixel.
///
/// P0 records prepared primitives whose raster bounds reach this sample. It
/// does not yet record separate events for degenerate or fully culled
/// primitives that never reach sample coverage.
#[derive(Debug, Clone, PartialEq)]
pub struct PixelHistory {
    pub pixel: PixelCoord,
    pub events: Vec<PixelHistoryEvent>,
    pub final_color: Rgba8,
    pub final_depth: f32,
}

impl PixelHistory {
    pub fn new(pixel: PixelCoord, clear_color: Rgba8, clear_depth: f32) -> Self {
        Self {
            pixel,
            events: Vec::new(),
            final_color: clear_color,
            final_depth: clear_depth,
        }
    }

    pub fn push(&mut self, event: PixelHistoryEvent) {
        self.events.push(event);
    }

    pub fn finish(&mut self, final_color: Rgba8, final_depth: f32) {
        self.final_color = final_color;
        self.final_depth = final_depth;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_preserves_submission_order_and_final_value() {
        let pixel = PixelCoord::new(4, 7);
        let mut history = PixelHistory::new(pixel, Rgba8::BLACK, 1.0);
        history.push(PixelHistoryEvent {
            triangle_index: 3,
            primitive_id: 99,
            outcome: PixelEventOutcome::Written,
            barycentric: Some([0.25, 0.25, 0.5]),
            candidate_depth: Some(0.4),
            previous_depth: 1.0,
            color_before: Rgba8::BLACK,
            color_after: Rgba8::new(10, 20, 30, 255),
        });
        history.finish(Rgba8::new(10, 20, 30, 255), 0.4);

        assert_eq!(history.pixel, pixel);
        assert_eq!(history.events[0].triangle_index, 3);
        assert_eq!(history.events[0].primitive_id, 99);
        assert_eq!(history.final_depth, 0.4);
    }
}
