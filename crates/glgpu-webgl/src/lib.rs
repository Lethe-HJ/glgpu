//! CPU-only WebGL 2 learning track.
//!
//! This crate is a separate stack from the WebGPU path. It does not depend on
//! `glgpu-webgpu`, `glgpu-shader`, `glgpu-core`, or the other WebGPU-track
//! crates, and it does not call browser WebGL or WebGPU.
//!
//! The compile path is ESSL source, then an AST, then self-authored bytecode,
//! then a CPU interpreter. There is no machine-code JIT. Instruction spans and
//! local snapshots are the hook for a later single-step debugger.
//!
//! Khronos glslang 16.6.0 is the ESSL accept/reject reference. Naga desktop
//! GLSL is not an ESSL frontend. The label stays `NOT WEBGL CONFORMANT`.
//! See `docs/webgl2-learning-track.zh-CN.md`.

#![forbid(unsafe_code)]

mod essl;
mod framebuffer;
mod raster;
mod vm;

pub use essl::{
    Expr, FrontendError, Function, InterfaceSymbol, SourceSpan, Statement, TranslationUnit,
    parse_essl_subset,
};
pub use framebuffer::{ContextError, LearningContext, Rgba8};
pub use raster::{
    P0_MILESTONE_CLEAR, RasterError, ScreenTriangle, ScreenVertex, p0_milestone_triangle,
};
pub use vm::{
    BytecodeFunction, BytecodeModule, CompileError, DebugStep, DebugSymbol, ExecTrace, Instruction,
    LocalSnapshot, LowerError, Op, SymbolKind, VmError, compile_and_execute, execute, lower,
};

use std::error::Error;
use std::fmt;

/// Permanent banner for this track. Do not rewrite it into a conformance claim.
pub const CONFORMANCE_BANNER: &str = "CpuWebGl2LearningTrack — NOT WEBGL CONFORMANT";

/// What this track is for.
pub const LEARNING_GOAL: &str = "Implement a complete WebGL2-style stack from scratch on the CPU for learning, with no browser-native WebGL or WebGPU.";

/// Lowering and execution shape. JIT is intentionally absent.
pub const COMPILE_PATH: &str =
    "ESSL source -> AST -> self-authored bytecode IR -> CPU interpreter; no machine-code JIT";

/// External oracle. This crate does not invoke it.
pub const ESSL_REFERENCE: &str = "Khronos glslang 16.6.0 is the ESSL accept/reject reference. Naga desktop GLSL is not an ESSL frontend.";

/// The only backend this track is allowed to grow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Backend {
    /// Software reference written in this crate. No host GPU API is linked.
    CpuFromScratch,
}

/// Specification profile under study.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LearningProfile {
    /// Khronos WebGL 2.0 studied as a CPU reference, not as a browser context.
    WebGl2,
}

/// Named rows in the learning-track support matrix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrackCapability {
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

impl TrackCapability {
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

/// Honest support state. Partial means a learning subset exists and is not the WebGL entry point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupportStatus {
    Supported,
    Partial,
    NotStarted,
}

/// Returned when a caller asks the matrix to treat a gap as supported.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityBlocked {
    pub capability: TrackCapability,
    pub status: SupportStatus,
    pub reason: &'static str,
}

impl fmt::Display for CapabilityBlocked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "WebGL2 learning capability {:?} is {:?}: {}",
            self.capability, self.status, self.reason
        )
    }
}

impl Error for CapabilityBlocked {}

/// Entry point for the parallel WebGL2 learning track.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WebGl2LearningTrack;

impl WebGl2LearningTrack {
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

    pub const fn profile(self) -> LearningProfile {
        LearningProfile::WebGl2
    }

    pub const fn conformant(self) -> bool {
        false
    }

    pub const fn banner(self) -> &'static str {
        CONFORMANCE_BANNER
    }

    /// Create a CPU learning framebuffer.
    ///
    /// The result is not a WebGL context. `require(ContextCreation)` stays
    /// blocked because the matrix row is only partial.
    pub fn create_context(self, width: u32, height: u32) -> Result<LearningContext, ContextError> {
        LearningContext::new(width, height)
    }

    pub const fn status(self, capability: TrackCapability) -> SupportStatus {
        match capability {
            TrackCapability::Rgba8Clear => SupportStatus::Supported,
            TrackCapability::ContextCreation
            | TrackCapability::ScreenSpaceTriangle
            | TrackCapability::Essl300Frontend
            | TrackCapability::BytecodeIr
            | TrackCapability::CpuInterpreter => SupportStatus::Partial,
            TrackCapability::MutableStateMachine
            | TrackCapability::DrawCall
            | TrackCapability::ReadPixels
            | TrackCapability::ExplicitPresent
            | TrackCapability::SingleStepDebugger
            | TrackCapability::JavaScriptFacade
            | TrackCapability::HostWebGlOrWebGpu => SupportStatus::NotStarted,
        }
    }

    pub const fn detail(self, capability: TrackCapability) -> &'static str {
        match capability {
            TrackCapability::ContextCreation => {
                "CPU learning framebuffer only; not a WebGL2RenderingContext"
            }
            TrackCapability::Rgba8Clear => "RGBA8 clear of the learning framebuffer",
            TrackCapability::ScreenSpaceTriangle => {
                "one pre-transformed top-left triangle; not drawArrays or clip space"
            }
            TrackCapability::Essl300Frontend => {
                "subset lexer/parser for #version 300 es; glslang is the external oracle and is not called"
            }
            TrackCapability::BytecodeIr => {
                "self-authored bytecode with statement spans; not a full shader ISA"
            }
            TrackCapability::CpuInterpreter => {
                "CPU interpreter for load_const and move, with a local step trace; no JIT"
            }
            TrackCapability::MutableStateMachine => "the WebGL state machine is not started",
            TrackCapability::DrawCall => "drawArrays and drawElements are not implemented",
            TrackCapability::ReadPixels => "synchronous readPixels is not implemented",
            TrackCapability::ExplicitPresent => {
                "CPU framebuffer present to Canvas 2D is not wired on this track"
            }
            TrackCapability::SingleStepDebugger => {
                "spans and local snapshots exist, but there is no debugger UI or pause control"
            }
            TrackCapability::JavaScriptFacade => {
                "no JavaScript WebGL facade; these objects are not browser WebGL objects"
            }
            TrackCapability::HostWebGlOrWebGpu => {
                "host WebGLRenderingContext and navigator.gpu are forbidden backends"
            }
        }
    }

    /// Succeeds only for [`SupportStatus::Supported`].
    ///
    /// Partial rows have their own methods. They are not a promise that the
    /// corresponding WebGL entry point exists.
    pub const fn require(self, capability: TrackCapability) -> Result<(), CapabilityBlocked> {
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
    fn track_stays_cpu_only_and_non_conformant() {
        let track = WebGl2LearningTrack::new();

        assert_eq!(track.backend(), Backend::CpuFromScratch);
        assert_eq!(track.profile(), LearningProfile::WebGl2);
        assert!(!track.conformant());
        assert_eq!(track.banner(), CONFORMANCE_BANNER);
        assert!(track.banner().contains("NOT WEBGL CONFORMANT"));
        assert!(LEARNING_GOAL.contains("from scratch on the CPU"));
        assert!(LEARNING_GOAL.contains("no browser-native WebGL or WebGPU"));
        assert!(COMPILE_PATH.contains("CPU interpreter"));
        assert!(COMPILE_PATH.contains("no machine-code JIT"));
        assert!(ESSL_REFERENCE.contains("glslang 16.6.0"));
        assert!(ESSL_REFERENCE.contains("Naga desktop GLSL is not an ESSL frontend"));
        const {
            assert!(!WebGl2LearningTrack::ALLOWS_HOST_WEBGL);
            assert!(!WebGl2LearningTrack::ALLOWS_HOST_WEBGPU);
            assert!(!WebGl2LearningTrack::ALLOWS_NAVIGATOR_GPU);
            assert!(!WebGl2LearningTrack::ALLOWS_WEBGL_RENDERING_CONTEXT);
        }
    }

    #[test]
    fn matrix_matches_the_learning_slice() {
        let track = WebGl2LearningTrack::new();
        assert_eq!(TrackCapability::ALL.len(), 13);
        assert_eq!(
            track.status(TrackCapability::Rgba8Clear),
            SupportStatus::Supported
        );
        assert!(track.require(TrackCapability::Rgba8Clear).is_ok());

        for capability in [
            TrackCapability::ContextCreation,
            TrackCapability::ScreenSpaceTriangle,
            TrackCapability::Essl300Frontend,
            TrackCapability::BytecodeIr,
            TrackCapability::CpuInterpreter,
        ] {
            assert_eq!(track.status(capability), SupportStatus::Partial);
            let blocked = track
                .require(capability)
                .expect_err("partial is not supported");
            assert_eq!(blocked.status, SupportStatus::Partial);
        }

        for capability in [
            TrackCapability::MutableStateMachine,
            TrackCapability::DrawCall,
            TrackCapability::ReadPixels,
            TrackCapability::ExplicitPresent,
            TrackCapability::SingleStepDebugger,
            TrackCapability::JavaScriptFacade,
            TrackCapability::HostWebGlOrWebGpu,
        ] {
            assert_eq!(track.status(capability), SupportStatus::NotStarted);
            let blocked = track.require(capability).expect_err("absent");
            assert_eq!(blocked.status, SupportStatus::NotStarted);
            assert!(!blocked.reason.is_empty());
        }

        let context = track.create_context(2, 2).expect("cpu context");
        assert!(context.banner().contains("NOT WEBGL CONFORMANT"));
        assert_eq!(track.create_context(0, 4), Err(ContextError::EmptyExtent));
    }
}
