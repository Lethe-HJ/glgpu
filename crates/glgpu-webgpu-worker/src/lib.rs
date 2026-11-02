//! Worker kernel for webgpu-mock.
//!
//! The device framebuffer still holds color and depth, because the depth plane
//! is not a present slot. After submit, packed color is written into the back
//! slot of one shared buffer. The main thread presents the front slot. This
//! crate does not touch the page canvas and does not use OffscreenCanvas.
//! `?threads=1` stays on the main-owner Rayon path in `glgpu-web`.

#![forbid(unsafe_code)]

mod session;
mod slots;

#[cfg(target_arch = "wasm32")]
mod api;

pub use session::PresentSession;
pub use slots::{ByteSlots, COMMAND_BYTES};
