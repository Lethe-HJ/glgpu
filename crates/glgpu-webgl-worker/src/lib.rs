//! Worker kernel for webgl-mock.
//!
//! The logical drawing buffer is the back slot of one shared buffer. Command
//! bytes live in that same buffer. The main thread presents the front slot
//! with Canvas 2D. This crate does not touch the page canvas and does not use
//! OffscreenCanvas.

#![forbid(unsafe_code)]

#[cfg(target_arch = "wasm32")]
mod api;
