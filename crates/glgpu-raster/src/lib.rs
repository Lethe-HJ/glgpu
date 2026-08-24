//! Deterministic 16×16 tile-based CPU triangle rasterization.
//!
//! The rasterizer consumes screen-space vertices with a top-left origin. It
//! uses 8-bit fixed-point edge equations and a top-left ownership rule. Tiles
//! are independent; both the sequential and optional Rayon paths preserve the
//! submitted triangle order within every tile.

#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;

use glgpu_core::CoreError;
pub use glgpu_core::{Extent2d, Framebuffer, PixelCoord, Rgba8, ScreenVertex, Triangle};
pub use glgpu_trace::{PixelEventOutcome, PixelHistory, PixelHistoryEvent};

pub const TILE_SIZE: u32 = 16;

const SUBPIXEL_BITS: u32 = 8;
const SUBPIXEL_SCALE: i64 = 1_i64 << SUBPIXEL_BITS;
const SUBPIXEL_HALF: i64 = SUBPIXEL_SCALE / 2;
const MAX_FIXED_ABS: f64 = (1_u64 << 50) as f64;
const TILE_PIXELS: usize = (TILE_SIZE as usize) * (TILE_SIZE as usize);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderOptions {
    pub clear_color: Rgba8,
    pub clear_depth: f32,
    pub selected_pixel: Option<PixelCoord>,
    /// Requires the crate's `parallel` feature. Parallelism is between tiles;
    /// primitive order inside a tile remains stable.
    pub parallel: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            clear_color: Rgba8::BLACK,
            clear_depth: 1.0,
            selected_pixel: None,
            parallel: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub triangles_submitted: u64,
    pub triangles_rasterized: u64,
    pub tiles_total: u64,
    pub nonempty_tiles: u64,
    pub tile_triangle_refs: u64,
    pub samples_tested: u64,
    pub samples_covered: u64,
    pub fragments_written: u64,
    pub fragments_depth_rejected: u64,
}

impl Stats {
    fn add_tile(&mut self, tile: Self) {
        self.nonempty_tiles = self.nonempty_tiles.saturating_add(tile.nonempty_tiles);
        self.tile_triangle_refs = self
            .tile_triangle_refs
            .saturating_add(tile.tile_triangle_refs);
        self.samples_tested = self.samples_tested.saturating_add(tile.samples_tested);
        self.samples_covered = self.samples_covered.saturating_add(tile.samples_covered);
        self.fragments_written = self
            .fragments_written
            .saturating_add(tile.fragments_written);
        self.fragments_depth_rejected = self
            .fragments_depth_rejected
            .saturating_add(tile.fragments_depth_rejected);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderOutput {
    pub framebuffer: Framebuffer,
    pub stats: Stats,
    pub pixel_history: Option<PixelHistory>,
}

impl RenderOutput {
    pub fn stable_hash64(&self) -> u64 {
        self.framebuffer.stable_hash64()
    }
}

/// Rasterizes `triangles` in slice order using a strict-less depth test.
/// Equal-depth fragments therefore retain the earliest submitted triangle.
pub fn render(
    extent: Extent2d,
    triangles: &[Triangle],
    options: RenderOptions,
) -> Result<RenderOutput, RasterError> {
    if !options.clear_depth.is_finite() {
        return Err(RasterError::InvalidClearDepth);
    }
    if let Some(pixel) = options.selected_pixel
        && !extent.contains(pixel)
    {
        return Err(RasterError::SelectedPixelOutOfBounds { pixel, extent });
    }

    let tile_grid = TileGrid::new(extent)?;
    let prepared = prepare_triangles(extent, triangles)?;
    let bins = bin_triangles(tile_grid, &prepared)?;

    let tile_results = raster_all_tiles(tile_grid, &bins, &prepared, options)?;
    let pixel_count = extent.pixel_count().map_err(RasterError::Core)?;
    let mut color = allocate_filled(pixel_count, options.clear_color)?;
    let mut depth = allocate_filled(pixel_count, options.clear_depth)?;

    let mut stats = Stats {
        triangles_submitted: triangles.len() as u64,
        triangles_rasterized: prepared.len() as u64,
        tiles_total: tile_grid.tile_count as u64,
        ..Stats::default()
    };
    let mut pixel_history = None;

    for tile in tile_results {
        stats.add_tile(tile.stats);
        merge_tile(extent, &tile, &mut color, &mut depth);
        if tile.history.is_some() {
            pixel_history = tile.history;
        }
    }

    let framebuffer = Framebuffer::from_planes(extent, color, depth).map_err(RasterError::Core)?;
    Ok(RenderOutput {
        framebuffer,
        stats,
        pixel_history,
    })
}

#[derive(Debug, Clone, Copy)]
struct TileGrid {
    extent: Extent2d,
    tiles_x: u32,
    tile_count: usize,
}

impl TileGrid {
    fn new(extent: Extent2d) -> Result<Self, RasterError> {
        let tiles_x = div_ceil_u32(extent.width, TILE_SIZE);
        let tiles_y = div_ceil_u32(extent.height, TILE_SIZE);
        let count = u64::from(tiles_x)
            .checked_mul(u64::from(tiles_y))
            .ok_or(RasterError::SizeOverflow)?;
        let tile_count = usize::try_from(count).map_err(|_| RasterError::SizeOverflow)?;
        Ok(Self {
            extent,
            tiles_x,
            tile_count,
        })
    }

    fn bounds(self, tile_index: usize) -> PixelBounds {
        let tiles_x = self.tiles_x as usize;
        let tile_y = (tile_index / tiles_x) as u32;
        let tile_x = (tile_index % tiles_x) as u32;
        let min_x = tile_x * TILE_SIZE;
        let min_y = tile_y * TILE_SIZE;
        PixelBounds {
            min_x,
            min_y,
            max_x: (min_x + TILE_SIZE - 1).min(self.extent.width - 1),
            max_y: (min_y + TILE_SIZE - 1).min(self.extent.height - 1),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PixelBounds {
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
}

impl PixelBounds {
    fn contains(self, pixel: PixelCoord) -> bool {
        pixel.x >= self.min_x
            && pixel.x <= self.max_x
            && pixel.y >= self.min_y
            && pixel.y <= self.max_y
    }

    fn intersect(self, other: Self) -> Option<Self> {
        let result = Self {
            min_x: self.min_x.max(other.min_x),
            min_y: self.min_y.max(other.min_y),
            max_x: self.max_x.min(other.max_x),
            max_y: self.max_y.min(other.max_y),
        };
        (result.min_x <= result.max_x && result.min_y <= result.max_y).then_some(result)
    }
}

#[derive(Debug, Clone, Copy)]
struct FixedPoint {
    x: i64,
    y: i64,
}

#[derive(Debug, Clone)]
struct PreparedTriangle {
    primitive_id: u64,
    triangle_index: u32,
    fixed: [FixedPoint; 3],
    vertices: [ScreenVertex; 3],
    area: i128,
    swapped_winding: bool,
    top_left: [bool; 3],
    bounds: PixelBounds,
}

fn prepare_triangles(
    extent: Extent2d,
    triangles: &[Triangle],
) -> Result<Vec<PreparedTriangle>, RasterError> {
    let mut prepared = Vec::new();
    prepared
        .try_reserve(triangles.len())
        .map_err(|_| RasterError::AllocationFailed)?;

    for (triangle_index, triangle) in triangles.iter().enumerate() {
        let triangle_index_u32 =
            u32::try_from(triangle_index).map_err(|_| RasterError::TooManyTriangles {
                count: triangles.len(),
            })?;
        if let Some(prepared_triangle) = prepare_triangle(extent, triangle_index_u32, *triangle)? {
            prepared.push(prepared_triangle);
        }
    }
    Ok(prepared)
}

fn prepare_triangle(
    extent: Extent2d,
    triangle_index: u32,
    triangle: Triangle,
) -> Result<Option<PreparedTriangle>, RasterError> {
    if extent.width == 0 || extent.height == 0 {
        return Ok(None);
    }

    let mut fixed = [FixedPoint { x: 0, y: 0 }; 3];
    let mut vertices = triangle.vertices;
    for (vertex_index, vertex) in vertices.iter().enumerate() {
        if !vertex.x.is_finite() || !vertex.y.is_finite() || !vertex.depth.is_finite() {
            return Err(RasterError::InvalidVertex {
                triangle_index,
                vertex_index: vertex_index as u8,
            });
        }
        fixed[vertex_index] = FixedPoint {
            x: to_fixed(vertex.x).ok_or(RasterError::CoordinateOutOfRange {
                triangle_index,
                vertex_index: vertex_index as u8,
            })?,
            y: to_fixed(vertex.y).ok_or(RasterError::CoordinateOutOfRange {
                triangle_index,
                vertex_index: vertex_index as u8,
            })?,
        };
    }

    let mut area = orient(fixed[0], fixed[1], fixed[2]);
    if area == 0 {
        return Ok(None);
    }
    let swapped_winding = area < 0;
    if area < 0 {
        fixed.swap(1, 2);
        vertices.swap(1, 2);
        area = -area;
    }

    let min_fixed_x = fixed.iter().map(|point| point.x).min().unwrap();
    let min_fixed_y = fixed.iter().map(|point| point.y).min().unwrap();
    let max_fixed_x = fixed.iter().map(|point| point.x).max().unwrap();
    let max_fixed_y = fixed.iter().map(|point| point.y).max().unwrap();

    let min_x = div_ceil_i64(min_fixed_x - SUBPIXEL_HALF, SUBPIXEL_SCALE).max(0);
    let min_y = div_ceil_i64(min_fixed_y - SUBPIXEL_HALF, SUBPIXEL_SCALE).max(0);
    let max_x = (max_fixed_x - SUBPIXEL_HALF).div_euclid(SUBPIXEL_SCALE);
    let max_y = (max_fixed_y - SUBPIXEL_HALF).div_euclid(SUBPIXEL_SCALE);

    let extent_max_x = i64::from(extent.width - 1);
    let extent_max_y = i64::from(extent.height - 1);
    if max_x < 0 || max_y < 0 || min_x > extent_max_x || min_y > extent_max_y {
        return Ok(None);
    }

    let bounds = PixelBounds {
        min_x: min_x.min(extent_max_x) as u32,
        min_y: min_y.min(extent_max_y) as u32,
        max_x: max_x.min(extent_max_x) as u32,
        max_y: max_y.min(extent_max_y) as u32,
    };

    Ok(Some(PreparedTriangle {
        primitive_id: triangle.primitive_id,
        triangle_index,
        fixed,
        vertices,
        area,
        swapped_winding,
        top_left: [
            is_top_left(fixed[1], fixed[2]),
            is_top_left(fixed[2], fixed[0]),
            is_top_left(fixed[0], fixed[1]),
        ],
        bounds,
    }))
}

fn bin_triangles(
    grid: TileGrid,
    triangles: &[PreparedTriangle],
) -> Result<Vec<Vec<usize>>, RasterError> {
    let mut bins = Vec::new();
    bins.try_reserve_exact(grid.tile_count)
        .map_err(|_| RasterError::AllocationFailed)?;
    bins.resize_with(grid.tile_count, Vec::new);

    for (triangle_index, triangle) in triangles.iter().enumerate() {
        let min_tile_x = triangle.bounds.min_x / TILE_SIZE;
        let max_tile_x = triangle.bounds.max_x / TILE_SIZE;
        let min_tile_y = triangle.bounds.min_y / TILE_SIZE;
        let max_tile_y = triangle.bounds.max_y / TILE_SIZE;
        for tile_y in min_tile_y..=max_tile_y {
            for tile_x in min_tile_x..=max_tile_x {
                let tile_index_u64 = u64::from(tile_y)
                    .checked_mul(u64::from(grid.tiles_x))
                    .and_then(|row| row.checked_add(u64::from(tile_x)))
                    .ok_or(RasterError::SizeOverflow)?;
                let tile_index =
                    usize::try_from(tile_index_u64).map_err(|_| RasterError::SizeOverflow)?;
                bins[tile_index]
                    .try_reserve(1)
                    .map_err(|_| RasterError::AllocationFailed)?;
                // Iterating triangles in input order makes every bin stable.
                bins[tile_index].push(triangle_index);
            }
        }
    }
    Ok(bins)
}

fn raster_all_tiles(
    grid: TileGrid,
    bins: &[Vec<usize>],
    triangles: &[PreparedTriangle],
    options: RenderOptions,
) -> Result<Vec<TileResult>, RasterError> {
    if options.parallel {
        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            return Ok(bins
                .par_iter()
                .enumerate()
                .map(|(tile_index, bin)| {
                    raster_tile(grid.bounds(tile_index), bin, triangles, options)
                })
                .collect());
        }
        #[cfg(not(feature = "parallel"))]
        {
            return Err(RasterError::ParallelFeatureDisabled);
        }
    }

    Ok(bins
        .iter()
        .enumerate()
        .map(|(tile_index, bin)| raster_tile(grid.bounds(tile_index), bin, triangles, options))
        .collect())
}

struct TileResult {
    bounds: PixelBounds,
    color: [Rgba8; TILE_PIXELS],
    depth: [f32; TILE_PIXELS],
    stats: Stats,
    history: Option<PixelHistory>,
}

fn raster_tile(
    tile_bounds: PixelBounds,
    bin: &[usize],
    triangles: &[PreparedTriangle],
    options: RenderOptions,
) -> TileResult {
    let mut result = TileResult {
        bounds: tile_bounds,
        color: [options.clear_color; TILE_PIXELS],
        depth: [options.clear_depth; TILE_PIXELS],
        stats: Stats {
            nonempty_tiles: u64::from(!bin.is_empty()),
            tile_triangle_refs: bin.len() as u64,
            ..Stats::default()
        },
        history: options
            .selected_pixel
            .filter(|pixel| tile_bounds.contains(*pixel))
            .map(|pixel| PixelHistory::new(pixel, options.clear_color, options.clear_depth)),
    };

    for &triangle_index in bin {
        let triangle = &triangles[triangle_index];
        let Some(bounds) = tile_bounds.intersect(triangle.bounds) else {
            continue;
        };
        for y in bounds.min_y..=bounds.max_y {
            for x in bounds.min_x..=bounds.max_x {
                result.stats.samples_tested = result.stats.samples_tested.saturating_add(1);
                raster_sample(&mut result, triangle, PixelCoord::new(x, y));
            }
        }
    }

    if let Some(history) = result.history.as_mut() {
        let index = tile_offset(result.bounds, history.pixel);
        history.finish(result.color[index], result.depth[index]);
    }
    result
}

fn raster_sample(result: &mut TileResult, triangle: &PreparedTriangle, pixel: PixelCoord) {
    let sample = FixedPoint {
        x: i64::from(pixel.x) * SUBPIXEL_SCALE + SUBPIXEL_HALF,
        y: i64::from(pixel.y) * SUBPIXEL_SCALE + SUBPIXEL_HALF,
    };
    let weights = [
        orient(triangle.fixed[1], triangle.fixed[2], sample),
        orient(triangle.fixed[2], triangle.fixed[0], sample),
        orient(triangle.fixed[0], triangle.fixed[1], sample),
    ];
    let covered =
        (0..3).all(|edge| weights[edge] > 0 || (weights[edge] == 0 && triangle.top_left[edge]));
    let index = tile_offset(result.bounds, pixel);

    if !covered {
        if let Some(history) = history_for_pixel(&mut result.history, pixel) {
            history.push(PixelHistoryEvent {
                triangle_index: triangle.triangle_index,
                primitive_id: triangle.primitive_id,
                outcome: PixelEventOutcome::OutsideCoverage,
                barycentric: None,
                candidate_depth: None,
                previous_depth: result.depth[index],
                color_before: result.color[index],
                color_after: result.color[index],
            });
        }
        return;
    }

    result.stats.samples_covered = result.stats.samples_covered.saturating_add(1);
    let barycentric_f64 = [
        weights[0] as f64 / triangle.area as f64,
        weights[1] as f64 / triangle.area as f64,
        weights[2] as f64 / triangle.area as f64,
    ];
    let barycentric = [
        barycentric_f64[0] as f32,
        barycentric_f64[1] as f32,
        barycentric_f64[2] as f32,
    ];
    let submitted_barycentric = if triangle.swapped_winding {
        [barycentric[0], barycentric[2], barycentric[1]]
    } else {
        barycentric
    };
    let candidate_depth = interpolate_depth(&triangle.vertices, barycentric_f64);
    let candidate_color = interpolate_color(&triangle.vertices, barycentric_f64);
    let previous_depth = result.depth[index];
    let color_before = result.color[index];
    let depth_passed = candidate_depth < previous_depth;

    if depth_passed {
        result.depth[index] = candidate_depth;
        result.color[index] = candidate_color;
        result.stats.fragments_written = result.stats.fragments_written.saturating_add(1);
    } else {
        result.stats.fragments_depth_rejected =
            result.stats.fragments_depth_rejected.saturating_add(1);
    }

    if let Some(history) = history_for_pixel(&mut result.history, pixel) {
        history.push(PixelHistoryEvent {
            triangle_index: triangle.triangle_index,
            primitive_id: triangle.primitive_id,
            outcome: if depth_passed {
                PixelEventOutcome::Written
            } else {
                PixelEventOutcome::DepthRejected
            },
            barycentric: Some(submitted_barycentric),
            candidate_depth: Some(candidate_depth),
            previous_depth,
            color_before,
            color_after: result.color[index],
        });
    }
}

fn history_for_pixel(
    history: &mut Option<PixelHistory>,
    pixel: PixelCoord,
) -> Option<&mut PixelHistory> {
    history.as_mut().filter(|history| history.pixel == pixel)
}

fn merge_tile(extent: Extent2d, tile: &TileResult, color: &mut [Rgba8], depth: &mut [f32]) {
    let width = extent.width as usize;
    for y in tile.bounds.min_y..=tile.bounds.max_y {
        for x in tile.bounds.min_x..=tile.bounds.max_x {
            let local = tile_offset(tile.bounds, PixelCoord::new(x, y));
            let global = (y as usize) * width + (x as usize);
            color[global] = tile.color[local];
            depth[global] = tile.depth[local];
        }
    }
}

fn tile_offset(bounds: PixelBounds, pixel: PixelCoord) -> usize {
    ((pixel.y - bounds.min_y) as usize) * (TILE_SIZE as usize) + ((pixel.x - bounds.min_x) as usize)
}

fn interpolate_depth(vertices: &[ScreenVertex; 3], barycentric: [f64; 3]) -> f32 {
    (barycentric[0] * f64::from(vertices[0].depth)
        + barycentric[1] * f64::from(vertices[1].depth)
        + barycentric[2] * f64::from(vertices[2].depth)) as f32
}

fn interpolate_color(vertices: &[ScreenVertex; 3], barycentric: [f64; 3]) -> Rgba8 {
    fn channel(values: [u8; 3], barycentric: [f64; 3]) -> u8 {
        let value = barycentric[0] * f64::from(values[0])
            + barycentric[1] * f64::from(values[1])
            + barycentric[2] * f64::from(values[2]);
        value.round().clamp(0.0, 255.0) as u8
    }

    Rgba8::new(
        channel(
            [
                vertices[0].color.r,
                vertices[1].color.r,
                vertices[2].color.r,
            ],
            barycentric,
        ),
        channel(
            [
                vertices[0].color.g,
                vertices[1].color.g,
                vertices[2].color.g,
            ],
            barycentric,
        ),
        channel(
            [
                vertices[0].color.b,
                vertices[1].color.b,
                vertices[2].color.b,
            ],
            barycentric,
        ),
        channel(
            [
                vertices[0].color.a,
                vertices[1].color.a,
                vertices[2].color.a,
            ],
            barycentric,
        ),
    )
}

fn orient(a: FixedPoint, b: FixedPoint, point: FixedPoint) -> i128 {
    let ab_x = i128::from(b.x) - i128::from(a.x);
    let ab_y = i128::from(b.y) - i128::from(a.y);
    let ap_x = i128::from(point.x) - i128::from(a.x);
    let ap_y = i128::from(point.y) - i128::from(a.y);
    ab_x * ap_y - ab_y * ap_x
}

/// Top-left ownership for a top-down framebuffer. With positive edge
/// functions, top horizontal edges run left-to-right and left edges run up.
fn is_top_left(a: FixedPoint, b: FixedPoint) -> bool {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    dy < 0 || (dy == 0 && dx > 0)
}

fn to_fixed(value: f32) -> Option<i64> {
    let scaled = f64::from(value) * SUBPIXEL_SCALE as f64;
    (scaled.is_finite() && scaled.abs() <= MAX_FIXED_ABS).then(|| scaled.round() as i64)
}

fn div_ceil_i64(value: i64, divisor: i64) -> i64 {
    let quotient = value.div_euclid(divisor);
    if value.rem_euclid(divisor) == 0 {
        quotient
    } else {
        quotient + 1
    }
}

fn div_ceil_u32(value: u32, divisor: u32) -> u32 {
    value.div_ceil(divisor)
}

fn allocate_filled<T: Clone>(len: usize, value: T) -> Result<Vec<T>, RasterError> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(len)
        .map_err(|_| RasterError::AllocationFailed)?;
    result.resize(len, value);
    Ok(result)
}

#[derive(Debug, Clone, PartialEq)]
pub enum RasterError {
    Core(CoreError),
    SizeOverflow,
    AllocationFailed,
    InvalidClearDepth,
    InvalidVertex {
        triangle_index: u32,
        vertex_index: u8,
    },
    CoordinateOutOfRange {
        triangle_index: u32,
        vertex_index: u8,
    },
    TooManyTriangles {
        count: usize,
    },
    SelectedPixelOutOfBounds {
        pixel: PixelCoord,
        extent: Extent2d,
    },
    ParallelFeatureDisabled,
}

impl fmt::Display for RasterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(error) => write!(f, "{error}"),
            Self::SizeOverflow => f.write_str("tile grid size overflow"),
            Self::AllocationFailed => f.write_str("raster allocation failed"),
            Self::InvalidClearDepth => f.write_str("clear depth must be finite"),
            Self::InvalidVertex {
                triangle_index,
                vertex_index,
            } => write!(
                f,
                "triangle {triangle_index} vertex {vertex_index} contains a non-finite value"
            ),
            Self::CoordinateOutOfRange {
                triangle_index,
                vertex_index,
            } => write!(
                f,
                "triangle {triangle_index} vertex {vertex_index} is outside the fixed-point range"
            ),
            Self::TooManyTriangles { count } => {
                write!(f, "triangle count {count} exceeds diagnostic index range")
            }
            Self::SelectedPixelOutOfBounds { pixel, extent } => write!(
                f,
                "selected pixel ({}, {}) is outside {}x{} framebuffer",
                pixel.x, pixel.y, extent.width, extent.height
            ),
            Self::ParallelFeatureDisabled => {
                f.write_str("parallel rendering requires the glgpu-raster `parallel` feature")
            }
        }
    }
}

impl Error for RasterError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Core(error) => Some(error),
            _ => None,
        }
    }
}

impl From<CoreError> for RasterError {
    fn from(value: CoreError) -> Self {
        Self::Core(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vertex(x: f32, y: f32, depth: f32, color: Rgba8) -> ScreenVertex {
        ScreenVertex::new(x, y, depth, color)
    }

    fn flat_triangle(id: u64, depth: f32, color: Rgba8) -> Triangle {
        Triangle::from_vertices(
            id,
            vertex(1.0, 1.0, depth, color),
            vertex(7.0, 1.0, depth, color),
            vertex(1.0, 7.0, depth, color),
        )
    }

    #[test]
    fn rasterizes_a_triangle_with_color_and_depth() {
        let output = render(
            Extent2d::new(8, 8),
            &[flat_triangle(1, 0.4, Rgba8::new(255, 0, 0, 255))],
            RenderOptions::default(),
        )
        .unwrap();

        assert_eq!(
            output.framebuffer.color_at(PixelCoord::new(2, 2)),
            Some(Rgba8::new(255, 0, 0, 255))
        );
        assert_eq!(
            output.framebuffer.depth_at(PixelCoord::new(2, 2)),
            Some(0.4)
        );
        assert!(output.stats.fragments_written > 0);
    }

    #[test]
    fn shared_edge_has_neither_cracks_nor_double_coverage() {
        let white = Rgba8::WHITE;
        let triangles = [
            Triangle::from_vertices(
                10,
                vertex(1.0, 1.0, 0.5, white),
                vertex(7.0, 1.0, 0.5, white),
                vertex(1.0, 7.0, 0.5, white),
            ),
            Triangle::from_vertices(
                11,
                vertex(7.0, 1.0, 0.5, white),
                vertex(7.0, 7.0, 0.5, white),
                vertex(1.0, 7.0, 0.5, white),
            ),
        ];
        let output = render(Extent2d::new(8, 8), &triangles, RenderOptions::default()).unwrap();

        for y in 1..7 {
            for x in 1..7 {
                assert_eq!(
                    output.framebuffer.color_at(PixelCoord::new(x, y)),
                    Some(white),
                    "missing square pixel ({x}, {y})"
                );
            }
        }
        assert_eq!(output.stats.samples_covered, 36);
        assert_eq!(output.stats.fragments_written, 36);
        assert_eq!(output.stats.fragments_depth_rejected, 0);
    }

    #[test]
    fn depth_test_prefers_near_fragment_and_equal_depth_is_stable() {
        let red = Rgba8::new(255, 0, 0, 255);
        let green = Rgba8::new(0, 255, 0, 255);
        let near_after_far = render(
            Extent2d::new(8, 8),
            &[flat_triangle(1, 0.8, red), flat_triangle(2, 0.2, green)],
            RenderOptions::default(),
        )
        .unwrap();
        assert_eq!(
            near_after_far.framebuffer.color_at(PixelCoord::new(2, 2)),
            Some(green)
        );

        let equal_depth = render(
            Extent2d::new(8, 8),
            &[flat_triangle(1, 0.3, red), flat_triangle(2, 0.3, green)],
            RenderOptions::default(),
        )
        .unwrap();
        assert_eq!(
            equal_depth.framebuffer.color_at(PixelCoord::new(2, 2)),
            Some(red)
        );
        assert!(equal_depth.stats.fragments_depth_rejected > 0);
    }

    #[test]
    fn selected_pixel_history_explains_depth_updates_in_submit_order() {
        let selected = PixelCoord::new(2, 2);
        let options = RenderOptions {
            selected_pixel: Some(selected),
            ..RenderOptions::default()
        };
        let output = render(
            Extent2d::new(8, 8),
            &[
                flat_triangle(41, 0.8, Rgba8::new(255, 0, 0, 255)),
                flat_triangle(42, 0.2, Rgba8::new(0, 255, 0, 255)),
            ],
            options,
        )
        .unwrap();
        let history = output.pixel_history.unwrap();

        assert_eq!(history.pixel, selected);
        assert_eq!(history.events.len(), 2);
        assert_eq!(history.events[0].primitive_id, 41);
        assert_eq!(history.events[1].primitive_id, 42);
        assert_eq!(history.events[0].outcome, PixelEventOutcome::Written);
        assert_eq!(history.events[1].outcome, PixelEventOutcome::Written);
        assert_eq!(history.final_color, Rgba8::new(0, 255, 0, 255));
        assert_eq!(history.final_depth, 0.2);
    }

    #[test]
    fn clockwise_triangle_history_keeps_submitted_vertex_order() {
        let selected = PixelCoord::new(3, 2);
        let triangle = Triangle::from_vertices(
            77,
            vertex(1.0, 1.0, 0.5, Rgba8::new(255, 0, 0, 255)),
            vertex(1.0, 7.0, 0.5, Rgba8::new(0, 255, 0, 255)),
            vertex(7.0, 1.0, 0.5, Rgba8::new(0, 0, 255, 255)),
        );
        let output = render(
            Extent2d::new(8, 8),
            &[triangle],
            RenderOptions {
                selected_pixel: Some(selected),
                ..RenderOptions::default()
            },
        )
        .unwrap();
        let barycentric = output.pixel_history.unwrap().events[0].barycentric.unwrap();

        let expected = [1.0 / 3.0, 1.0 / 4.0, 5.0 / 12.0];
        for (actual, expected) in barycentric.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1.0e-6);
        }
    }

    #[cfg(feature = "parallel")]
    #[test]
    fn sequential_and_rayon_outputs_are_identical() {
        let triangles = [
            Triangle::from_vertices(
                1,
                vertex(1.0, 1.0, 0.7, Rgba8::new(255, 0, 0, 255)),
                vertex(63.0, 2.0, 0.7, Rgba8::new(0, 255, 0, 255)),
                vertex(2.0, 63.0, 0.7, Rgba8::new(0, 0, 255, 255)),
            ),
            Triangle::from_vertices(
                2,
                vertex(63.0, 2.0, 0.4, Rgba8::new(255, 255, 0, 255)),
                vertex(63.0, 63.0, 0.4, Rgba8::new(255, 0, 255, 255)),
                vertex(2.0, 63.0, 0.4, Rgba8::new(0, 255, 255, 255)),
            ),
            Triangle::from_vertices(
                3,
                vertex(8.0, 8.0, 0.2, Rgba8::WHITE),
                vertex(56.0, 12.0, 0.2, Rgba8::WHITE),
                vertex(24.0, 58.0, 0.2, Rgba8::WHITE),
            ),
        ];
        let base = RenderOptions {
            selected_pixel: Some(PixelCoord::new(24, 24)),
            ..RenderOptions::default()
        };
        let sequential = render(Extent2d::new(64, 64), &triangles, base).unwrap();
        let parallel = render(
            Extent2d::new(64, 64),
            &triangles,
            RenderOptions {
                parallel: true,
                ..base
            },
        )
        .unwrap();

        assert_eq!(sequential.stable_hash64(), parallel.stable_hash64());
        assert_eq!(sequential.framebuffer, parallel.framebuffer);
        assert_eq!(sequential.stats, parallel.stats);
        assert_eq!(sequential.pixel_history, parallel.pixel_history);
    }

    #[cfg(not(feature = "parallel"))]
    #[test]
    fn requesting_parallel_without_feature_is_an_error() {
        let error = render(
            Extent2d::new(1, 1),
            &[],
            RenderOptions {
                parallel: true,
                ..RenderOptions::default()
            },
        )
        .unwrap_err();
        assert_eq!(error, RasterError::ParallelFeatureDisabled);
    }
}
