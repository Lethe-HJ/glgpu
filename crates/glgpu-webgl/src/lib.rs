//! CPU-only WebGL 2 learning-track skeleton.
//!
//! The learning goal matches the WebGPU path: implement a WebGL2-style stack
//! from scratch on the CPU, from nothing to a complete educational subset.
//! This crate must not call or wrap browser-native WebGL or WebGPU. That
//! includes `WebGLRenderingContext`, `canvas.getContext("webgl2")`, and
//! `navigator.gpu`.
//!
//! Nothing in the support matrix is implemented. ADR 0001 still defers the
//! product WebGL gate to P7a+. ADR 0003 only opens this parallel scaffold.
//! See `docs/webgl2-learning-track.zh-CN.md`.

#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt;

/// Permanent banner for this track. Do not rewrite it into a conformance claim.
pub const CONFORMANCE_BANNER: &str = "CpuWebGl2LearningTrack — NOT WEBGL CONFORMANT";

/// What this track is for. The Chinese track note is the narrative source.
pub const LEARNING_GOAL: &str = "Implement a complete WebGL2-style stack from scratch on the CPU for learning, with no browser-native WebGL or WebGPU.";

/// The only backend this track is allowed to grow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Backend {
    /// Software reference written in this repository. No host GPU API is linked.
    CpuFromScratch,
}

/// Specification profile under study. Selecting it enables no entry points.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LearningProfile {
    /// Khronos WebGL 2.0, studied as a CPU reference rather than a browser context.
    WebGl2,
}

/// Named gaps in the learning-track support matrix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrackCapability {
    ContextCreation,
    Essl300Frontend,
    MutableStateMachine,
    DrawCall,
    ReadPixels,
    ExplicitPresent,
    JavaScriptFacade,
    HostWebGlOrWebGpu,
}

impl TrackCapability {
    /// Every capability the skeleton knows about.
    ///
    /// `status` matches on the enum, so a new variant cannot compile until it
    /// is classified. This list must stay aligned with that match.
    pub const ALL: [Self; 8] = [
        Self::ContextCreation,
        Self::Essl300Frontend,
        Self::MutableStateMachine,
        Self::DrawCall,
        Self::ReadPixels,
        Self::ExplicitPresent,
        Self::JavaScriptFacade,
        Self::HostWebGlOrWebGpu,
    ];
}

/// Support state recorded for the learning track.
///
/// `NotStarted` is the only state this scaffold is allowed to report.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupportStatus {
    /// No implementation exists. Callers must treat the capability as absent.
    NotStarted,
}

/// Structured refusal returned for every learning-track entry point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NotStarted {
    pub capability: TrackCapability,
    pub reason: &'static str,
}

impl fmt::Display for NotStarted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "WebGL2 learning track has not started {:?}: {}",
            self.capability, self.reason
        )
    }
}

impl Error for NotStarted {}

/// Marker for the parallel WebGL2 learning track.
///
/// Constructing this value allocates no GL state and touches no browser API.
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

    pub const fn status(self, capability: TrackCapability) -> SupportStatus {
        match capability {
            TrackCapability::ContextCreation
            | TrackCapability::Essl300Frontend
            | TrackCapability::MutableStateMachine
            | TrackCapability::DrawCall
            | TrackCapability::ReadPixels
            | TrackCapability::ExplicitPresent
            | TrackCapability::JavaScriptFacade
            | TrackCapability::HostWebGlOrWebGpu => SupportStatus::NotStarted,
        }
    }

    pub const fn detail(self, capability: TrackCapability) -> &'static str {
        match capability {
            TrackCapability::ContextCreation => {
                "no CPU WebGL2 context exists yet; do not wrap canvas.getContext"
            }
            TrackCapability::Essl300Frontend => {
                "ESSL 3.00 parser and lowering are not started; Naga GLSL is not a substitute"
            }
            TrackCapability::MutableStateMachine => {
                "the WebGL state machine is not started and stays a P7b product concern"
            }
            TrackCapability::DrawCall => "drawArrays and drawElements are not implemented",
            TrackCapability::ReadPixels => "synchronous readPixels is not implemented",
            TrackCapability::ExplicitPresent => {
                "CPU framebuffer present to Canvas 2D is not wired on this track"
            }
            TrackCapability::JavaScriptFacade => {
                "no JavaScript WebGL façade; objects here would not be browser WebGL objects"
            }
            TrackCapability::HostWebGlOrWebGpu => {
                "host WebGLRenderingContext and navigator.gpu are forbidden backends"
            }
        }
    }

    pub const fn require(self, capability: TrackCapability) -> Result<(), NotStarted> {
        match self.status(capability) {
            SupportStatus::NotStarted => Err(NotStarted {
                capability,
                reason: self.detail(capability),
            }),
        }
    }

    /// Context creation is refused until a later change defines its behavior.
    pub const fn create_context(self) -> Result<(), NotStarted> {
        self.require(TrackCapability::ContextCreation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_is_a_cpu_learning_skeleton() {
        let track = WebGl2LearningTrack::new();

        assert_eq!(track.backend(), Backend::CpuFromScratch);
        assert_eq!(track.profile(), LearningProfile::WebGl2);
        assert!(!track.conformant());
        assert_eq!(track.banner(), CONFORMANCE_BANNER);
        assert!(track.banner().contains("NOT WEBGL CONFORMANT"));
        assert!(LEARNING_GOAL.contains("from scratch on the CPU"));
        assert!(LEARNING_GOAL.contains("no browser-native WebGL or WebGPU"));
        const {
            assert!(!WebGl2LearningTrack::ALLOWS_HOST_WEBGL);
            assert!(!WebGl2LearningTrack::ALLOWS_HOST_WEBGPU);
            assert!(!WebGl2LearningTrack::ALLOWS_NAVIGATOR_GPU);
            assert!(!WebGl2LearningTrack::ALLOWS_WEBGL_RENDERING_CONTEXT);
        }
    }

    #[test]
    fn every_capability_is_not_started() {
        let track = WebGl2LearningTrack::new();

        assert_eq!(TrackCapability::ALL.len(), 8);
        for capability in TrackCapability::ALL {
            assert_eq!(track.status(capability), SupportStatus::NotStarted);
            let err = track
                .require(capability)
                .expect_err("the scaffold refuses every capability");
            assert_eq!(err.capability, capability);
            assert!(!err.reason.is_empty());
        }
        assert!(track.create_context().is_err());
    }
}
