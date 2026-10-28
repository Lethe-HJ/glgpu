//! webgl-mock: a WebGL2-style visualization running on the CPU.
//!
//! This crate is not native WebGL and not native WebGPU. It does not depend on
//! `glgpu-webgpu`, `glgpu-shader`, `glgpu-core`, or the other webgpu-mock
//! crates, and it does not call browser WebGL or WebGPU.
//!
//! Target split: the drawing buffer lives only in this Worker + Wasm backend.
//! The CPU framebuffer is authoritative. Present sends those pixels to the main
//! thread by SharedArrayBuffer or a zero-copy transferable ArrayBuffer. The
//! main thread presents with Canvas 2D. There is no OffscreenCanvas blit. The
//! pipeline does not draw to the visible page canvas. Pixel payloads are not
//! sent with a copied `postMessage`. The demo page still runs this crate as
//! single-threaded Wasm on the main thread. The Worker backend is not built yet.
//!
//! The compile path is ESSL source, then an AST, then self-authored bytecode,
//! then a CPU interpreter. There is no machine-code JIT. Instruction spans and
//! local snapshots are the hook for a later single-step debugger.
//!
//! Khronos glslang 16.6.0 is the ESSL accept/reject reference. Naga desktop
//! GLSL is not an ESSL frontend. The label stays `NOT WEBGL CONFORMANT`.
//! See `docs/webgl-mock.zh-CN.md`.

#![forbid(unsafe_code)]

mod clip;
mod essl;
mod framebuffer;
mod raster;
mod vm;

pub use clip::{
    ClipError, ClipOutcome, ClipPosition, ClipVertex, WindowPoint, WindowVertex,
    clip_point_to_window, clip_triangle, storage_y_from_window_y,
};
pub use essl::{
    Expr, FrontendError, Function, InterfaceSymbol, SourceSpan, Statement, TranslationUnit,
    parse_essl_subset,
};
pub use framebuffer::{ContextError, DemoContext, Rgba8};
pub use raster::{
    P0_MILESTONE_CLEAR, RasterError, ScreenTriangle, ScreenVertex, ShaderDrawError,
    ShaderTriangleDraw, p0_milestone_clip_vertices, p0_milestone_triangle,
};
pub use vm::{
    BytecodeFunction, BytecodeModule, CompileError, DebugStep, DebugSymbol, Dest, ExecTrace,
    Instruction, InterfaceSnapshot, LocalSnapshot, LowerError, Op, Operand, SymbolKind, VmError,
    compile_and_execute, execute, execute_bound, lower,
};

use std::error::Error;
use std::fmt;

/// Permanent banner. Do not rewrite it into a conformance claim.
pub const CONFORMANCE_BANNER: &str = "webgl-mock — NOT WEBGL CONFORMANT";

/// What webgl-mock is. Native WebGL and native WebGPU stay outside this crate.
pub const WEBGL_MOCK: &str = "webgl-mock is a WebGL2-style visualization running on the CPU. It is not native WebGL and not native WebGPU.";

/// Target thread split versus the demo page today.
pub const THREADING: &str = "Decided 2026-10-28: no OffscreenCanvas. The drawing buffer lives only in the Worker + Wasm backend. The CPU pixel framebuffer is authoritative for raster results, framebuffer hash, unit tests, and pixel debugging. Present sends those pixels to the main thread by SharedArrayBuffer or a zero-copy transferable ArrayBuffer. The main thread presents with Canvas 2D putImageData / ImageData. There is no OffscreenCanvas blit. The pipeline does not draw to the visible page canvas. Messaging is SharedArrayBuffer or zero-copy transfer only, not a copied postMessage of pixel payloads. Current demo: single-threaded Wasm on the main thread, then Canvas 2D putImageData onto the page canvas. The Worker backend is not built yet.";

/// Lowering and execution shape. JIT is intentionally absent.
pub const COMPILE_PATH: &str =
    "ESSL source -> AST -> self-authored bytecode IR -> CPU interpreter; no machine-code JIT";

/// External oracle. This crate does not invoke it.
pub const ESSL_REFERENCE: &str = "Khronos glslang 16.6.0 is the ESSL accept/reject reference. Naga desktop GLSL is not an ESSL frontend.";

/// The only backend webgl-mock is allowed to grow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Backend {
    /// CPU implementation in this crate. No native WebGL or native WebGPU API is linked.
    CpuFromScratch,
}

/// Specification profile this CPU stack follows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpecProfile {
    /// Khronos WebGL 2.0 as a CPU reference, not as a browser context.
    WebGl2,
}

/// Named rows in the webgl-mock support matrix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Capability {
    ContextCreation,
    Rgba8Clear,
    ScreenSpaceTriangle,
    Essl300Frontend,
    BytecodeIr,
    CpuInterpreter,
    MutableStateMachine,
    DrawCall,
    ReadPixels,
    ExplicitPresent,
    SingleStepDebugger,
    JavaScriptFacade,
    HostWebGlOrWebGpu,
}

impl Capability {
    pub const ALL: [Self; 13] = [
        Self::ContextCreation,
        Self::Rgba8Clear,
        Self::ScreenSpaceTriangle,
        Self::Essl300Frontend,
        Self::BytecodeIr,
        Self::CpuInterpreter,
        Self::MutableStateMachine,
        Self::DrawCall,
        Self::ReadPixels,
        Self::ExplicitPresent,
        Self::SingleStepDebugger,
        Self::JavaScriptFacade,
        Self::HostWebGlOrWebGpu,
    ];
}

/// Honest support state. Partial means a CPU subset exists and is not the WebGL entry point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupportStatus {
    Supported,
    Partial,
    NotStarted,
}

/// Returned when a caller asks the matrix to treat a gap as supported.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityBlocked {
    pub capability: Capability,
    pub status: SupportStatus,
    pub reason: &'static str,
}

impl fmt::Display for CapabilityBlocked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "webgl-mock capability {:?} is {:?}: {}",
            self.capability, self.status, self.reason
        )
    }
}

impl Error for CapabilityBlocked {}

/// Entry point for the parallel webgl-mock stack.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WebGlMock;

impl WebGlMock {
    pub const ALLOWS_HOST_WEBGL: bool = false;
    pub const ALLOWS_HOST_WEBGPU: bool = false;
    pub const ALLOWS_NAVIGATOR_GPU: bool = false;
    pub const ALLOWS_WEBGL_RENDERING_CONTEXT: bool = false;

    pub const fn new() -> Self {
        Self
    }

    pub const fn backend(self) -> Backend {
        Backend::CpuFromScratch
    }

    pub const fn profile(self) -> SpecProfile {
        SpecProfile::WebGl2
    }

    pub const fn conformant(self) -> bool {
        false
    }

    pub const fn banner(self) -> &'static str {
        CONFORMANCE_BANNER
    }

    /// Create a CPU demo framebuffer for webgl-mock.
    ///
    /// The result is not a WebGL context. `require(ContextCreation)` stays
    /// blocked because the matrix row is only partial.
    pub fn create_context(self, width: u32, height: u32) -> Result<DemoContext, ContextError> {
        DemoContext::new(width, height)
    }

    pub const fn status(self, capability: Capability) -> SupportStatus {
        match capability {
            Capability::Rgba8Clear => SupportStatus::Supported,
            Capability::ContextCreation
            | Capability::ScreenSpaceTriangle
            | Capability::Essl300Frontend
            | Capability::BytecodeIr
            | Capability::CpuInterpreter => SupportStatus::Partial,
            Capability::MutableStateMachine
            | Capability::DrawCall
            | Capability::ReadPixels
            | Capability::ExplicitPresent
            | Capability::SingleStepDebugger
            | Capability::JavaScriptFacade
            | Capability::HostWebGlOrWebGpu => SupportStatus::NotStarted,
        }
    }

    pub const fn detail(self, capability: Capability) -> &'static str {
        match capability {
            Capability::ContextCreation => {
                "webgl-mock demo framebuffer only; not native WebGL2RenderingContext"
            }
            Capability::Rgba8Clear => "RGBA8 clear of the CPU framebuffer",
            Capability::ScreenSpaceTriangle => {
                "WebGL clip volume -w..w, bottom-left window, transitional top-left storage; shader outputs can drive one triangle; not drawArrays"
            }
            Capability::Essl300Frontend => {
                "subset lexer/parser for #version 300 es, scalar uniforms, ins, outs, and one multiplication; glslang is not called"
            }
            Capability::BytecodeIr => {
                "self-authored bytecode with statement spans, including uniform, input, output, and mul ops"
            }
            Capability::CpuInterpreter => {
                "CPU interpreter for load_const, move, mul, and uniform/input/output traffic, with spans and local snapshots; no JIT"
            }
            Capability::MutableStateMachine => "the WebGL state machine is not started",
            Capability::DrawCall => "drawArrays and drawElements are not implemented",
            Capability::ReadPixels => "synchronous readPixels is not implemented",
            Capability::ExplicitPresent => {
                "DemoContext has no present method. The current demo copies RGBA8 onto the page canvas with Canvas 2D putImageData on the main thread. The target keeps that CPU framebuffer authoritative and presents it from the main thread. There is no OffscreenCanvas blit. This copy is not native WebGL present"
            }
            Capability::SingleStepDebugger => {
                "spans, local snapshots, and interface snapshots exist, but there is no single-step pause UI. The debug page runs the whole frame; it does not step"
            }
            Capability::JavaScriptFacade => {
                "no JavaScript WebGL facade. The target main thread would expose that facade and reach a Worker + Wasm backend through SharedArrayBuffer or zero-copy transfer only, not a copied postMessage of pixel payloads. Today the demo shell runs webgl-mock on the main thread. These objects are not native WebGL objects"
            }
            Capability::HostWebGlOrWebGpu => {
                "native WebGLRenderingContext and navigator.gpu are forbidden backends"
            }
        }
    }

    /// Succeeds only for [`SupportStatus::Supported`].
    ///
    /// Partial rows have their own methods. They are not a promise that the
    /// corresponding WebGL entry point exists.
    pub const fn require(self, capability: Capability) -> Result<(), CapabilityBlocked> {
        match self.status(capability) {
            SupportStatus::Supported => Ok(()),
            status => Err(CapabilityBlocked {
                capability,
                status,
                reason: self.detail(capability),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_stack_stays_non_conformant() {
        let cpu = WebGlMock::new();

        assert_eq!(cpu.backend(), Backend::CpuFromScratch);
        assert_eq!(cpu.profile(), SpecProfile::WebGl2);
        assert!(!cpu.conformant());
        assert_eq!(cpu.banner(), CONFORMANCE_BANNER);
        assert!(cpu.banner().contains("webgl-mock"));
        assert!(cpu.banner().contains("NOT WEBGL CONFORMANT"));
        assert!(!cpu.banner().to_ascii_lowercase().contains("learning"));
        assert!(WEBGL_MOCK.contains("webgl-mock"));
        assert!(WEBGL_MOCK.contains("running on the CPU"));
        assert!(WEBGL_MOCK.contains("not native WebGL"));
        assert!(WEBGL_MOCK.contains("not native WebGPU"));
        assert!(!WEBGL_MOCK.to_ascii_lowercase().contains("learning"));
        assert!(THREADING.contains("no OffscreenCanvas"));
        assert!(THREADING.contains("There is no OffscreenCanvas blit"));
        assert!(THREADING.contains("authoritative"));
        assert!(THREADING.contains("transferable ArrayBuffer"));
        assert!(THREADING.contains("main thread"));
        assert!(THREADING.contains("Canvas 2D"));
        assert!(THREADING.contains("SharedArrayBuffer"));
        assert!(THREADING.contains("not a copied postMessage"));
        assert!(THREADING.contains("Worker backend is not built yet"));
        assert!(!THREADING.contains("Scheme C"));
        assert!(!THREADING.contains("ImageBitmap"));
        assert!(COMPILE_PATH.contains("CPU interpreter"));
        assert!(COMPILE_PATH.contains("no machine-code JIT"));
        assert!(ESSL_REFERENCE.contains("glslang 16.6.0"));
        assert!(ESSL_REFERENCE.contains("Naga desktop GLSL is not an ESSL frontend"));
        const {
            assert!(!WebGlMock::ALLOWS_HOST_WEBGL);
            assert!(!WebGlMock::ALLOWS_HOST_WEBGPU);
            assert!(!WebGlMock::ALLOWS_NAVIGATOR_GPU);
            assert!(!WebGlMock::ALLOWS_WEBGL_RENDERING_CONTEXT);
        }
    }

    #[test]
    fn matrix_matches_the_cpu_slice() {
        let cpu = WebGlMock::new();
        assert_eq!(Capability::ALL.len(), 13);
        assert_eq!(cpu.status(Capability::Rgba8Clear), SupportStatus::Supported);
        assert!(cpu.require(Capability::Rgba8Clear).is_ok());

        for capability in [
            Capability::ContextCreation,
            Capability::ScreenSpaceTriangle,
            Capability::Essl300Frontend,
            Capability::BytecodeIr,
            Capability::CpuInterpreter,
        ] {
            assert_eq!(cpu.status(capability), SupportStatus::Partial);
            let blocked = cpu
                .require(capability)
                .expect_err("partial is not supported");
            assert_eq!(blocked.status, SupportStatus::Partial);
        }

        for capability in [
            Capability::MutableStateMachine,
            Capability::DrawCall,
            Capability::ReadPixels,
            Capability::ExplicitPresent,
            Capability::SingleStepDebugger,
            Capability::JavaScriptFacade,
            Capability::HostWebGlOrWebGpu,
        ] {
            assert_eq!(cpu.status(capability), SupportStatus::NotStarted);
            let blocked = cpu.require(capability).expect_err("absent");
            assert_eq!(blocked.status, SupportStatus::NotStarted);
            assert!(!blocked.reason.is_empty());
        }

        let context = cpu.create_context(2, 2).expect("cpu context");
        assert!(context.banner().contains("NOT WEBGL CONFORMANT"));
        assert_eq!(cpu.create_context(0, 4), Err(ContextError::EmptyExtent));
        assert!(
            cpu.detail(Capability::ScreenSpaceTriangle)
                .contains("-w..w")
        );
        assert!(
            cpu.detail(Capability::ScreenSpaceTriangle)
                .contains("bottom-left")
        );
        assert!(cpu.detail(Capability::DrawCall).contains("not implemented"));
        assert!(cpu.detail(Capability::CpuInterpreter).contains("no JIT"));
    }
}
