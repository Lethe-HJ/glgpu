//! Double-buffered present slots for webgl-mock.
//!
//! The writable back slot is the logical drawing buffer. Publishing it with
//! atomics makes that slot the front slot the main thread may copy into
//! Canvas 2D. This module does not create an OffscreenCanvas and does not
//! transfer the buffer away.

use std::sync::Mutex;
use std::sync::atomic::Ordering;

#[cfg(target_has_atomic = "32")]
use std::sync::atomic::AtomicU32;

/// Wasm without the atomics feature still builds the main-thread demo.
/// That build never shares the slots. The worker build enables atomics.
#[cfg(not(target_has_atomic = "32"))]
mod single_thread_atomic {
    use super::Ordering;
    use std::cell::Cell;

    #[derive(Debug)]
    pub struct AtomicU32(Cell<u32>);

    impl AtomicU32 {
        pub const fn new(value: u32) -> Self {
            Self(Cell::new(value))
        }

        pub fn load(&self, _ordering: Ordering) -> u32 {
            self.0.get()
        }

        pub fn store(&self, value: u32, _ordering: Ordering) {
            self.0.set(value);
        }

        pub fn fetch_add(&self, value: u32, _ordering: Ordering) -> u32 {
            let current = self.0.get();
            self.0.set(current.wrapping_add(value));
            current
        }
    }
}

#[cfg(not(target_has_atomic = "32"))]
use single_thread_atomic::AtomicU32;

use crate::essl::parse_essl_subset;
use crate::framebuffer::{ContextError, DemoContext, Rgba8};
use crate::raster::{P0_MILESTONE_CLEAR, p0_milestone_clip_vertices};
use crate::vm::lower;

/// Two slots: one is presented, the other is drawn.
pub const SLOT_COUNT: u32 = 2;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PresentError {
    Context(ContextError),
    Shader(String),
    Command(String),
}

impl std::fmt::Display for PresentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Context(error) => write!(f, "{error}"),
            Self::Shader(message) | Self::Command(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for PresentError {}

/// Pixel slots plus the atomics that hand a finished frame to the presenter.
///
/// Bytes reserved for one ESSL command. The editor cap matches this size.
pub const COMMAND_CAP: usize = 256 * 1024;

/// `readable` is the slot the main thread may copy. The other slot is the
/// back slot. `held == 0` means the presenter is idle; otherwise `held - 1`
/// is the slot being copied. The command region lives in the same allocation
/// family as the slots so the main thread can write source bytes without
/// posting the pixel payload.
pub struct DoubleBuffer {
    width: u32,
    height: u32,
    slots: Mutex<[DemoContext; 2]>,
    command: Mutex<CommandBytes>,
    readable: AtomicU32,
    held: AtomicU32,
    frame_index: AtomicU32,
    command_len: AtomicU32,
    command_seq: AtomicU32,
}

impl DoubleBuffer {
    pub fn new(width: u32, height: u32) -> Result<Self, ContextError> {
        let first = DemoContext::new(width, height)?;
        let second = DemoContext::new(width, height)?;
        let command = CommandBytes::new(COMMAND_CAP);
        Ok(Self {
            width,
            height,
            slots: Mutex::new([first, second]),
            command: Mutex::new(command),
            readable: AtomicU32::new(0),
            held: AtomicU32::new(0),
            frame_index: AtomicU32::new(0),
            command_len: AtomicU32::new(0),
            command_seq: AtomicU32::new(0),
        })
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub fn frame_index(&self) -> u32 {
        self.frame_index.load(Ordering::Acquire)
    }

    pub fn readable_slot(&self) -> u32 {
        self.readable.load(Ordering::Acquire)
    }

    pub fn held(&self) -> bool {
        self.held.load(Ordering::Acquire) != 0
    }

    /// Address of the readable-slot atomic inside this process.
    ///
    /// On a shared Wasm memory this is the byte offset the main thread loads
    /// with `Atomics`.
    pub fn readable_atomic_addr(&self) -> *const AtomicU32 {
        &self.readable
    }

    pub fn frame_atomic_addr(&self) -> *const AtomicU32 {
        &self.frame_index
    }

    pub fn held_atomic_addr(&self) -> *const AtomicU32 {
        &self.held
    }

    pub fn command_len_addr(&self) -> *const AtomicU32 {
        &self.command_len
    }

    pub fn command_seq_addr(&self) -> *const AtomicU32 {
        &self.command_seq
    }

    /// First byte of the command region. The address is 8-byte aligned.
    pub fn command_ptr(&self) -> *const u8 {
        self.command.lock().expect("present command").ptr()
    }

    pub const fn command_cap(&self) -> usize {
        COMMAND_CAP
    }

    /// Copy a command into the shared region. The page does the same write
    /// through the exported pointer.
    pub fn set_command_bytes(&self, bytes: &[u8]) -> Result<(), PresentError> {
        self.command
            .lock()
            .expect("present command")
            .write(bytes)
            .map_err(PresentError::Command)?;
        self.command_len
            .store(bytes.len() as u32, Ordering::Release);
        self.command_seq.fetch_add(1, Ordering::Release);
        Ok(())
    }

    pub fn command_source(&self) -> Result<String, PresentError> {
        let len = self.command_len.load(Ordering::Acquire) as usize;
        if len > COMMAND_CAP {
            return Err(PresentError::Command(
                "ESSL command length is outside the shared region".to_owned(),
            ));
        }
        let bytes = self
            .command
            .lock()
            .expect("present command")
            .read(len)
            .map_err(PresentError::Command)?;
        String::from_utf8(bytes)
            .map_err(|_| PresentError::Command("ESSL command is not UTF-8".to_owned()))
    }

    /// Raster the ESSL command currently stored in the shared region.
    pub fn run_commanded_essl(&self) -> Result<(), PresentError> {
        let source = self.command_source()?;
        self.draw_back(|context| render_essl_demo(context, &source))
    }

    pub fn slot_ptr(&self, slot: u32) -> *const u8 {
        let slots = self.slots.lock().expect("present slots");
        slots[slot as usize % 2].pixel_ptr()
    }

    pub fn slot_byte_len(&self) -> usize {
        self.slots.lock().expect("present slots")[0].pixel_byte_len()
    }

    /// Mark `slot` busy so the worker will not draw into it yet.
    ///
    /// `held == 0` means the presenter is idle. Otherwise `held - 1` is the
    /// slot being copied to Canvas 2D.
    pub fn hold_slot(&self, slot: u32) {
        self.held.store(slot + 1, Ordering::Release);
    }

    /// Release the slot the presenter was copying.
    pub fn release_front(&self) {
        self.held.store(0, Ordering::Release);
    }

    /// Draw into the back slot, then publish it as the new front slot.
    ///
    /// The closure receives the slot's own [`DemoContext`]. Pixels are not
    /// copied into a second full-frame buffer.
    /// Draw into the back slot. The slot becomes readable only when `draw`
    /// returns `Ok`. A failed command does not publish a partial frame.
    pub fn draw_back<T, E>(
        &self,
        draw: impl FnOnce(&mut DemoContext) -> Result<T, E>,
    ) -> Result<T, E> {
        let back = self.wait_for_back_slot();
        let result = {
            let mut slots = self.slots.lock().expect("present slots");
            draw(&mut slots[back as usize])
        };
        if result.is_ok() {
            self.readable.store(back, Ordering::Release);
            self.frame_index.fetch_add(1, Ordering::Release);
        }
        result
    }

    pub fn front_pixel(&self, x: u32, y: u32) -> Option<Rgba8> {
        let slot = self.readable_slot() as usize;
        self.slots.lock().expect("present slots")[slot].pixel(x, y)
    }

    pub fn slot_pixel(&self, slot: u32, x: u32, y: u32) -> Option<Rgba8> {
        self.slots.lock().expect("present slots")[slot as usize % 2].pixel(x, y)
    }

    fn wait_for_back_slot(&self) -> u32 {
        loop {
            let front = self.readable.load(Ordering::Acquire) % SLOT_COUNT;
            let back = (front + 1) % SLOT_COUNT;
            let held = self.held.load(Ordering::Acquire);
            if held != 0 && held - 1 == back {
                std::hint::spin_loop();
                continue;
            }
            return back;
        }
    }
}

/// Clear and draw the demo ESSL triangle into an existing context.
pub fn render_essl_demo(context: &mut DemoContext, source: &str) -> Result<(), PresentError> {
    let unit = parse_essl_subset(source).map_err(|error| PresentError::Shader(error.message))?;
    let module = lower(&unit).map_err(|error| PresentError::Shader(error.message))?;
    let clips = p0_milestone_clip_vertices();
    let inputs = [
        vertex_inputs(clips[0]),
        vertex_inputs(clips[1]),
        vertex_inputs(clips[2]),
    ];
    let uniform_offer = [("u_scale", 1.0)];
    let uniforms = filter_bindings(&uniform_offer, &module.uniforms);
    let bound = [
        filter_bindings(&inputs[0], &module.inputs),
        filter_bindings(&inputs[1], &module.inputs),
        filter_bindings(&inputs[2], &module.inputs),
    ];
    context.clear(P0_MILESTONE_CLEAR);
    context
        .draw_shader_triangle(
            &module,
            "main",
            &uniforms,
            [&bound[0], &bound[1], &bound[2]],
        )
        .map_err(|error| PresentError::Shader(error.to_string()))?;
    Ok(())
}

fn vertex_inputs(vertex: crate::clip::ClipVertex) -> [(&'static str, f32); 7] {
    [
        ("a_x", vertex.position.x),
        ("a_y", vertex.position.y),
        ("a_z", vertex.position.z),
        ("a_w", vertex.position.w),
        ("a_r", vertex.color[0]),
        ("a_g", vertex.color[1]),
        ("a_b", vertex.color[2]),
    ]
}

struct CommandBytes {
    storage: Vec<u8>,
    offset: usize,
    cap: usize,
}

impl CommandBytes {
    fn new(cap: usize) -> Self {
        const ALIGN: usize = 8;
        let storage = vec![0_u8; cap + ALIGN];
        let addr = storage.as_ptr() as usize;
        let offset = (ALIGN - (addr % ALIGN)) % ALIGN;
        Self {
            storage,
            offset,
            cap,
        }
    }

    fn ptr(&self) -> *const u8 {
        self.storage.as_ptr().wrapping_add(self.offset)
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() > self.cap {
            return Err("ESSL command does not fit in the shared region".to_owned());
        }
        let start = self.offset;
        self.storage[start..start + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }

    fn read(&self, len: usize) -> Result<Vec<u8>, String> {
        if len > self.cap {
            return Err("ESSL command length is outside the shared region".to_owned());
        }
        let start = self.offset;
        Ok(self.storage[start..start + len].to_vec())
    }
}

fn filter_bindings<'a>(
    offered: &'a [(&'a str, f32)],
    symbols: &[crate::vm::DebugSymbol],
) -> Vec<(&'a str, f32)> {
    offered
        .iter()
        .copied()
        .filter(|(name, _)| symbols.iter().any(|symbol| symbol.name == *name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn publishing_the_back_slot_does_not_allocate_a_second_frame() {
        let buffer = DoubleBuffer::new(2, 1).expect("buffer");
        let back_ptr = buffer.slot_ptr(1);
        buffer
            .draw_back(|context| {
                context.clear(Rgba8::new(9, 8, 7, 255));
                Ok::<(), PresentError>(())
            })
            .expect("publish");

        assert_eq!(buffer.readable_slot(), 1);
        assert_eq!(buffer.frame_index(), 1);
        assert_eq!(buffer.slot_ptr(1), back_ptr);
        assert_eq!(buffer.front_pixel(0, 0), Some(Rgba8::new(9, 8, 7, 255)));
        assert_eq!(buffer.slot_pixel(0, 0, 0), Some(Rgba8::TRANSPARENT));

        buffer
            .draw_back(|context| {
                context.clear(Rgba8::new(1, 2, 3, 255));
                Ok::<(), PresentError>(())
            })
            .expect("publish");
        assert_eq!(buffer.readable_slot(), 0);
        assert_eq!(buffer.frame_index(), 2);
        assert_eq!(buffer.front_pixel(1, 0), Some(Rgba8::new(1, 2, 3, 255)));
        assert_eq!(buffer.slot_pixel(1, 0, 0), Some(Rgba8::new(9, 8, 7, 255)));
    }

    #[test]
    fn writer_waits_until_the_held_front_slot_is_released() {
        let buffer = Arc::new(DoubleBuffer::new(1, 1).expect("buffer"));
        buffer
            .draw_back(|context| {
                context.clear(Rgba8::new(4, 0, 0, 255));
                Ok::<(), PresentError>(())
            })
            .expect("publish");
        buffer.hold_slot(buffer.readable_slot());

        let writer = Arc::clone(&buffer);
        let thread = thread::spawn(move || {
            writer
                .draw_back(|context| {
                    context.clear(Rgba8::new(5, 0, 0, 255));
                    Ok::<(), PresentError>(())
                })
                .expect("publish");
            writer
                .draw_back(|context| {
                    context.clear(Rgba8::new(6, 0, 0, 255));
                    Ok::<(), PresentError>(())
                })
                .expect("publish");
        });

        thread::sleep(Duration::from_millis(40));
        assert_eq!(buffer.frame_index(), 2);
        buffer.release_front();
        thread.join().expect("writer");
        assert_eq!(buffer.frame_index(), 3);
        assert!(!buffer.held());
    }

    #[test]
    fn essl_demo_draws_into_the_published_back_slot() {
        let source = include_str!("../../../examples/webgl2-triangle/p0_vertex.essl");
        let buffer = DoubleBuffer::new(720, 480).expect("buffer");
        buffer
            .draw_back(|context| render_essl_demo(context, source))
            .expect("demo");
        assert_eq!(buffer.frame_index(), 1);
        assert_ne!(buffer.front_pixel(360, 80), Some(P0_MILESTONE_CLEAR));
        assert_eq!(buffer.front_pixel(0, 0), Some(P0_MILESTONE_CLEAR));
    }

    #[test]
    fn command_region_is_aligned_and_drives_the_back_slot() {
        let source = include_str!("../../../examples/webgl2-triangle/p0_vertex.essl");
        let buffer = DoubleBuffer::new(720, 480).expect("buffer");
        assert_eq!(buffer.command_ptr() as usize % 8, 0);
        assert_eq!(buffer.readable_atomic_addr() as usize % 4, 0);
        assert_eq!(buffer.held_atomic_addr() as usize % 4, 0);
        assert_eq!(buffer.frame_atomic_addr() as usize % 4, 0);
        assert_eq!(buffer.command_len_addr() as usize % 4, 0);
        assert_eq!(buffer.command_seq_addr() as usize % 4, 0);
        buffer
            .set_command_bytes(source.as_bytes())
            .expect("command");
        buffer.run_commanded_essl().expect("raster");
        assert_eq!(buffer.frame_index(), 1);
        assert_ne!(buffer.front_pixel(360, 80), Some(P0_MILESTONE_CLEAR));
        assert!(
            buffer
                .set_command_bytes(&vec![0_u8; COMMAND_CAP + 1])
                .is_err()
        );
    }
}
