//! webgpu-mock: a non-standard WebGPU-style P0 façade running on the CPU.
//!
//! This crate intentionally exposes [`CpuGpuSubset`] rather than pretending to
//! be native WebGPU or a conformant browser `GPUAdapter`. The banner stays
//! `CpuGpuSubset — NOT WEBGPU CONFORMANT`. Its queue is synchronous and bound to
//! the creating owner thread. The target present path is one double-buffered
//! SharedArrayBuffer. The back slot is the drawing buffer, and Atomics hand the
//! front slot to the main thread for Canvas 2D. There is no OffscreenCanvas
//! blit. The main thread does not receive pixel payloads by a copied
//! `postMessage`. That split is not this P0
//! queue. JavaScript/WebIDL objects, promises, error
//! scopes, and standard asynchronous queue semantics are explicitly outside
//! this P0 contract.

use std::error::Error;
use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;

use glgpu_core::{Extent2d, Framebuffer, PixelCoord, Rgba8, Triangle};
use glgpu_raster::{RasterError, RenderOptions, RenderOutput, render};
use glgpu_shader::{ShaderDiagnostic, ValidatedShaderModule, parse_and_validate_wgsl};
use glgpu_trace::PixelHistory;

/// One named ability in the explicit P0 support matrix.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Capability {
    Rgba8DepthClear,
    ScreenSpaceTriangleDraw,
    SelectedPixelHistory,
    SynchronousImmediateSubmit,
    SynchronousColorReadback,
    DeterministicTileParallelism,
    WgslParseAndValidate,
    WgslExecution,
    Compute,
    AsynchronousQueue,
    JavaScriptFacade,
    StandardWebGpuAdapter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupportStatus {
    Supported,
    Planned,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SupportEntry {
    pub capability: Capability,
    pub status: SupportStatus,
    pub detail: &'static str,
}

/// Immutable support matrix for this crate's P0 implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct SupportMatrix;

impl SupportMatrix {
    pub const fn entries(self) -> &'static [SupportEntry] {
        P0_SUPPORT_ENTRIES
    }

    pub fn get(self, capability: Capability) -> &'static SupportEntry {
        P0_SUPPORT_ENTRIES
            .iter()
            .find(|entry| entry.capability == capability)
            .expect("every public capability has a P0 support entry")
    }

    pub fn supports(self, capability: Capability) -> bool {
        self.get(capability).status == SupportStatus::Supported
    }

    pub fn require(self, capability: Capability) -> Result<(), Unsupported> {
        let entry = self.get(capability);
        if entry.status == SupportStatus::Supported {
            Ok(())
        } else {
            Err(Unsupported {
                capability,
                status: entry.status,
                reason: entry.detail,
            })
        }
    }
}

const P0_SUPPORT_ENTRIES: &[SupportEntry] = &[
    SupportEntry {
        capability: Capability::Rgba8DepthClear,
        status: SupportStatus::Supported,
        detail: "one RGBA8/depth clear at the start of a command buffer",
    },
    SupportEntry {
        capability: Capability::ScreenSpaceTriangleDraw,
        status: SupportStatus::Supported,
        detail: "ordered, pre-transformed CPU triangles",
    },
    SupportEntry {
        capability: Capability::SelectedPixelHistory,
        status: SupportStatus::Supported,
        detail: "one optional raster-stage pixel per command buffer; P0 omits pre-raster cull events",
    },
    SupportEntry {
        capability: Capability::SynchronousImmediateSubmit,
        status: SupportStatus::Supported,
        detail: "submit completes on the calling owner thread",
    },
    SupportEntry {
        capability: Capability::SynchronousColorReadback,
        status: SupportStatus::Supported,
        detail: "borrowed Rgba8 pixels or an owned RGBA byte copy",
    },
    SupportEntry {
        capability: Capability::DeterministicTileParallelism,
        status: if cfg!(feature = "parallel") {
            SupportStatus::Supported
        } else {
            SupportStatus::Unsupported
        },
        detail: if cfg!(feature = "parallel") {
            "tile-parallel raster is enabled for this build"
        } else {
            "rebuild glgpu-webgpu with its `parallel` feature"
        },
    },
    SupportEntry {
        capability: Capability::WgslParseAndValidate,
        status: SupportStatus::Supported,
        detail: "Naga 30 parse and validation behind a project-owned boundary",
    },
    SupportEntry {
        capability: Capability::WgslExecution,
        status: SupportStatus::Planned,
        detail: "P0 triangles are pre-transformed and do not execute WGSL",
    },
    SupportEntry {
        capability: Capability::Compute,
        status: SupportStatus::Planned,
        detail: "compute/workgroup execution starts after the render slice",
    },
    SupportEntry {
        capability: Capability::AsynchronousQueue,
        status: SupportStatus::Planned,
        detail: "owner-worker queue/Future semantics are a later gate",
    },
    SupportEntry {
        capability: Capability::JavaScriptFacade,
        status: SupportStatus::Planned,
        detail: "no JavaScript objects, WebIDL conversion, or Promise semantics",
    },
    SupportEntry {
        capability: Capability::StandardWebGpuAdapter,
        status: SupportStatus::Unsupported,
        detail: "this subset does not satisfy WebGPU core capability guarantees",
    },
];

/// Structured rejection for a capability outside the explicit subset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Unsupported {
    pub capability: Capability,
    pub status: SupportStatus,
    pub reason: &'static str,
}

impl fmt::Display for Unsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "capability {:?} is {:?}: {}",
            self.capability, self.status, self.reason
        )
    }
}

impl Error for Unsupported {}

/// Root of the non-standard Rust-native provider.
#[derive(Clone, Copy, Debug, Default)]
pub struct CpuGpuSubset {
    support: SupportMatrix,
}

impl CpuGpuSubset {
    pub const fn new() -> Self {
        Self {
            support: SupportMatrix,
        }
    }

    pub const fn support_matrix(&self) -> SupportMatrix {
        self.support
    }

    pub const fn request_adapter(&self) -> CpuAdapter {
        CpuAdapter {
            support: self.support,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CpuAdapter {
    support: SupportMatrix,
}

impl CpuAdapter {
    pub const fn support_matrix(&self) -> SupportMatrix {
        self.support
    }

    pub fn request_device(&self, descriptor: DeviceDescriptor) -> Result<CpuDevice, CpuGpuError> {
        if descriptor.extent.width == 0 || descriptor.extent.height == 0 {
            return Err(CpuGpuError::InvalidDescriptor(
                "device extent width and height must both be non-zero",
            ));
        }
        descriptor
            .extent
            .pixel_count()
            .map_err(|_| CpuGpuError::InvalidDescriptor("device extent is too large"))?;
        if descriptor.parallel {
            self.support
                .require(Capability::DeterministicTileParallelism)?;
        }

        Ok(CpuDevice {
            descriptor,
            support: self.support,
            queue: ImmediateMainOwnerQueue::new(descriptor.extent, descriptor.parallel),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceDescriptor {
    pub extent: Extent2d,
    /// Ask the raster backend to use its deterministic parallel executor.
    pub parallel: bool,
}

impl DeviceDescriptor {
    pub const fn new(extent: Extent2d) -> Self {
        Self {
            extent,
            parallel: false,
        }
    }

    pub const fn with_parallel(mut self, parallel: bool) -> Self {
        self.parallel = parallel;
        self
    }
}

pub struct CpuDevice {
    descriptor: DeviceDescriptor,
    support: SupportMatrix,
    queue: ImmediateMainOwnerQueue,
}

impl fmt::Debug for CpuDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CpuDevice")
            .field("descriptor", &self.descriptor)
            .field("last_completed_serial", &self.queue.last_completed_serial())
            .finish_non_exhaustive()
    }
}

impl CpuDevice {
    pub const fn descriptor(&self) -> DeviceDescriptor {
        self.descriptor
    }

    pub const fn support_matrix(&self) -> SupportMatrix {
        self.support
    }

    pub fn create_command_encoder(&self) -> CommandEncoder {
        CommandEncoder::new(self.descriptor.extent)
    }

    /// Parse and validate WGSL. P0 does not execute the returned module.
    pub fn create_shader_module(
        &self,
        source: impl Into<String>,
    ) -> Result<ValidatedShaderModule, ShaderDiagnostic> {
        parse_and_validate_wgsl(source)
    }

    pub const fn queue(&self) -> &ImmediateMainOwnerQueue {
        &self.queue
    }

    pub const fn queue_mut(&mut self) -> &mut ImmediateMainOwnerQueue {
        &mut self.queue
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ClearCommand {
    color: Rgba8,
    depth: f32,
}

/// Records the deliberately small P0 command surface.
#[derive(Debug)]
pub struct CommandEncoder {
    extent: Extent2d,
    clear: Option<ClearCommand>,
    triangles: Vec<Triangle>,
    selected_pixel: Option<PixelCoord>,
}

impl CommandEncoder {
    fn new(extent: Extent2d) -> Self {
        Self {
            extent,
            clear: None,
            triangles: Vec::new(),
            selected_pixel: None,
        }
    }

    /// Record the sole P0 clear. It must precede every draw.
    pub fn clear(&mut self, color: Rgba8, depth: f32) -> Result<(), CommandEncodingError> {
        if self.clear.is_some() {
            return Err(CommandEncodingError::DuplicateClear);
        }
        if !self.triangles.is_empty() {
            return Err(CommandEncodingError::ClearAfterDraw);
        }
        if !depth.is_finite() || !(0.0..=1.0).contains(&depth) {
            return Err(CommandEncodingError::InvalidClearDepth);
        }
        self.clear = Some(ClearCommand { color, depth });
        Ok(())
    }

    pub fn draw_triangle(&mut self, triangle: Triangle) -> Result<(), CommandEncodingError> {
        if self.clear.is_none() {
            return Err(CommandEncodingError::DrawBeforeClear);
        }
        self.triangles
            .try_reserve(1)
            .map_err(|_| CommandEncodingError::AllocationFailed)?;
        self.triangles.push(triangle);
        Ok(())
    }

    pub fn select_pixel(&mut self, pixel: PixelCoord) -> Result<(), CommandEncodingError> {
        if !self.extent.contains(pixel) {
            return Err(CommandEncodingError::SelectedPixelOutOfBounds {
                pixel,
                extent: self.extent,
            });
        }
        self.selected_pixel = Some(pixel);
        Ok(())
    }

    pub fn finish(self) -> Result<CommandBuffer, CommandEncodingError> {
        let clear = self.clear.ok_or(CommandEncodingError::MissingClear)?;
        Ok(CommandBuffer {
            clear,
            triangles: self.triangles,
            selected_pixel: self.selected_pixel,
        })
    }
}

/// Immutable commands submitted exactly once by value.
#[derive(Debug)]
pub struct CommandBuffer {
    clear: ClearCommand,
    triangles: Vec<Triangle>,
    selected_pixel: Option<PixelCoord>,
}

impl CommandBuffer {
    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    pub const fn selected_pixel(&self) -> Option<PixelCoord> {
        self.selected_pixel
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubmissionReport {
    pub serial: u64,
    pub stable_hash64: u64,
    pub triangle_count: usize,
    pub selected_pixel_recorded: bool,
}

struct CompletedSubmission {
    serial: u64,
    output: RenderOutput,
}

/// Synchronous P0 queue, deliberately `!Send` and `!Sync`.
///
/// `submit` does not emulate WebGPU queue timelines or promises. It executes
/// the complete command buffer before returning to its owner thread.
pub struct ImmediateMainOwnerQueue {
    extent: Extent2d,
    parallel: bool,
    next_serial: u64,
    completed: Option<CompletedSubmission>,
    _owner_thread: PhantomData<Rc<()>>,
}

impl fmt::Debug for ImmediateMainOwnerQueue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImmediateMainOwnerQueue")
            .field("extent", &self.extent)
            .field("parallel", &self.parallel)
            .field("next_serial", &self.next_serial)
            .field("last_completed_serial", &self.last_completed_serial())
            .finish_non_exhaustive()
    }
}

impl ImmediateMainOwnerQueue {
    fn new(extent: Extent2d, parallel: bool) -> Self {
        Self {
            extent,
            parallel,
            next_serial: 1,
            completed: None,
            _owner_thread: PhantomData,
        }
    }

    pub fn submit(
        &mut self,
        command_buffer: CommandBuffer,
    ) -> Result<SubmissionReport, CpuGpuError> {
        let serial = self.next_serial;
        let next_serial = serial.checked_add(1).ok_or(CpuGpuError::SerialExhausted)?;
        let triangle_count = command_buffer.triangles.len();
        let output = render(
            self.extent,
            &command_buffer.triangles,
            RenderOptions {
                clear_color: command_buffer.clear.color,
                clear_depth: command_buffer.clear.depth,
                selected_pixel: command_buffer.selected_pixel,
                parallel: self.parallel,
            },
        )?;
        let report = SubmissionReport {
            serial,
            stable_hash64: output.framebuffer.stable_hash64(),
            triangle_count,
            selected_pixel_recorded: output.pixel_history.is_some(),
        };
        self.completed = Some(CompletedSubmission { serial, output });
        self.next_serial = next_serial;
        Ok(report)
    }

    pub fn last_completed_serial(&self) -> Option<u64> {
        self.completed.as_ref().map(|submission| submission.serial)
    }

    pub fn readback_framebuffer(&self) -> Result<&Framebuffer, CpuGpuError> {
        self.completed
            .as_ref()
            .map(|submission| &submission.output.framebuffer)
            .ok_or(CpuGpuError::NoCompletedSubmission)
    }

    pub fn readback_color(&self) -> Result<&[Rgba8], CpuGpuError> {
        Ok(self.readback_framebuffer()?.color())
    }

    /// Make an owned, tightly packed RGBA8 byte copy. This is deliberately
    /// fallible. In an MT Wasm build the `Vec` still lives in shared linear
    /// memory, so the Canvas boundary performs one further non-shared staging
    /// copy before constructing `ImageData`.
    pub fn readback_rgba8(&self) -> Result<Vec<u8>, CpuGpuError> {
        let pixels = self.readback_color()?;
        let byte_len = pixels
            .len()
            .checked_mul(4)
            .ok_or(CpuGpuError::AllocationFailed)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(byte_len)
            .map_err(|_| CpuGpuError::AllocationFailed)?;
        for pixel in pixels {
            bytes.extend_from_slice(&pixel.to_array());
        }
        Ok(bytes)
    }

    pub fn selected_pixel_history(&self) -> Option<&PixelHistory> {
        self.completed
            .as_ref()
            .and_then(|submission| submission.output.pixel_history.as_ref())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CommandEncodingError {
    MissingClear,
    DuplicateClear,
    ClearAfterDraw,
    DrawBeforeClear,
    InvalidClearDepth,
    SelectedPixelOutOfBounds { pixel: PixelCoord, extent: Extent2d },
    AllocationFailed,
}

impl fmt::Display for CommandEncodingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingClear => f.write_str("a P0 command buffer requires one clear"),
            Self::DuplicateClear => f.write_str("P0 supports exactly one clear"),
            Self::ClearAfterDraw => f.write_str("the clear must precede all triangle draws"),
            Self::DrawBeforeClear => f.write_str("triangle draw recorded before the clear"),
            Self::InvalidClearDepth => {
                f.write_str("clear depth must be finite and in the inclusive range 0..=1")
            }
            Self::SelectedPixelOutOfBounds { pixel, extent } => write!(
                f,
                "selected pixel ({}, {}) is outside {}x{}",
                pixel.x, pixel.y, extent.width, extent.height
            ),
            Self::AllocationFailed => f.write_str("command recording allocation failed"),
        }
    }
}

impl Error for CommandEncodingError {}

#[derive(Debug)]
pub enum CpuGpuError {
    Unsupported(Unsupported),
    InvalidDescriptor(&'static str),
    NoCompletedSubmission,
    AllocationFailed,
    SerialExhausted,
    Raster(RasterError),
}

impl fmt::Display for CpuGpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(error) => error.fmt(f),
            Self::InvalidDescriptor(message) => write!(f, "invalid device descriptor: {message}"),
            Self::NoCompletedSubmission => f.write_str("the queue has no completed submission"),
            Self::AllocationFailed => f.write_str("readback allocation failed"),
            Self::SerialExhausted => f.write_str("submission serial space is exhausted"),
            Self::Raster(error) => write!(f, "raster submission failed: {error}"),
        }
    }
}

impl Error for CpuGpuError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Unsupported(error) => Some(error),
            Self::Raster(error) => Some(error),
            _ => None,
        }
    }
}

impl From<RasterError> for CpuGpuError {
    fn from(value: RasterError) -> Self {
        Self::Raster(value)
    }
}

impl From<Unsupported> for CpuGpuError {
    fn from(value: Unsupported) -> Self {
        Self::Unsupported(value)
    }
}
