//! Two color slots plus a small command region for webgpu-mock present.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

/// Packed triangle command: 21 little-endian `f32` values, then selected `x` and `y`.
pub const COMMAND_BYTES: usize = 21 * 4 + 8;
const COMMAND_CAP: usize = 256;

pub struct ByteSlots {
    width: u32,
    height: u32,
    slots: Mutex<[Vec<u8>; 2]>,
    command: Mutex<CommandBytes>,
    readable: AtomicU32,
    held: AtomicU32,
    frame_index: AtomicU32,
    command_len: AtomicU32,
    command_seq: AtomicU32,
}

impl ByteSlots {
    pub fn new(width: u32, height: u32) -> Result<Self, String> {
        let bytes = byte_len(width, height)?;
        Ok(Self {
            width,
            height,
            slots: Mutex::new([vec![0; bytes], vec![0; bytes]]),
            command: Mutex::new(CommandBytes::new(COMMAND_CAP)),
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

    pub fn slot_ptr(&self, slot: u32) -> *const u8 {
        self.slots.lock().expect("present slots")[slot as usize % 2].as_ptr()
    }

    pub fn slot_byte_len(&self) -> usize {
        self.slots.lock().expect("present slots")[0].len()
    }

    pub fn command_ptr(&self) -> *const u8 {
        self.command.lock().expect("present command").ptr()
    }

    pub const fn command_cap(&self) -> usize {
        COMMAND_CAP
    }

    pub fn hold_slot(&self, slot: u32) {
        self.held.store(slot + 1, Ordering::Release);
    }

    pub fn release_front(&self) {
        self.held.store(0, Ordering::Release);
    }

    pub fn set_command_bytes(&self, bytes: &[u8]) -> Result<(), String> {
        self.command.lock().expect("present command").write(bytes)?;
        self.command_len
            .store(bytes.len() as u32, Ordering::Release);
        self.command_seq.fetch_add(1, Ordering::Release);
        Ok(())
    }

    pub fn command_bytes(&self) -> Result<Vec<u8>, String> {
        let len = self.command_len.load(Ordering::Acquire) as usize;
        self.command.lock().expect("present command").read(len)
    }

    pub fn publish_bytes(&self, bytes: &[u8]) -> Result<(), String> {
        self.draw_back(|slot| {
            if slot.len() != bytes.len() {
                return Err("present slot size does not match the framebuffer".to_owned());
            }
            slot.copy_from_slice(bytes);
            Ok(())
        })
    }

    pub fn front_bytes(&self) -> Vec<u8> {
        let slot = self.readable_slot() as usize % 2;
        self.slots.lock().expect("present slots")[slot].clone()
    }

    fn draw_back<T, E>(&self, draw: impl FnOnce(&mut [u8]) -> Result<T, E>) -> Result<T, E> {
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

    fn wait_for_back_slot(&self) -> u32 {
        loop {
            let front = self.readable.load(Ordering::Acquire) % 2;
            let back = (front + 1) % 2;
            let held = self.held.load(Ordering::Acquire);
            if held != 0 && held - 1 == back {
                std::hint::spin_loop();
                continue;
            }
            return back;
        }
    }
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
            return Err("triangle command does not fit in the shared region".to_owned());
        }
        let start = self.offset;
        self.storage[start..start + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }

    fn read(&self, len: usize) -> Result<Vec<u8>, String> {
        if len > self.cap {
            return Err("triangle command length is outside the shared region".to_owned());
        }
        let start = self.offset;
        Ok(self.storage[start..start + len].to_vec())
    }
}

fn byte_len(width: u32, height: u32) -> Result<usize, String> {
    if width == 0 || height == 0 {
        return Err("present slots need a non-zero extent".to_owned());
    }
    usize::try_from(u64::from(width) * u64::from(height) * 4)
        .map_err(|_| "present slot byte length overflows".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn publishing_color_swaps_slots_without_moving_the_back_pointer() {
        let slots = ByteSlots::new(2, 1).expect("slots");
        let back_ptr = slots.slot_ptr(1);
        slots
            .publish_bytes(&[9, 8, 7, 255, 1, 2, 3, 255])
            .expect("publish");
        assert_eq!(slots.readable_slot(), 1);
        assert_eq!(slots.frame_index(), 1);
        assert_eq!(slots.slot_ptr(1), back_ptr);
        assert_eq!(slots.front_bytes(), vec![9, 8, 7, 255, 1, 2, 3, 255]);
        assert_eq!(slots.command_ptr() as usize % 8, 0);
        assert_eq!(slots.readable_atomic_addr() as usize % 4, 0);
    }

    #[test]
    fn writer_waits_until_the_held_front_slot_is_released() {
        let slots = Arc::new(ByteSlots::new(1, 1).expect("slots"));
        slots.publish_bytes(&[4, 0, 0, 255]).expect("publish");
        slots.hold_slot(slots.readable_slot());
        let writer = Arc::clone(&slots);
        let thread = thread::spawn(move || {
            writer.publish_bytes(&[5, 0, 0, 255]).expect("publish");
            writer.publish_bytes(&[6, 0, 0, 255]).expect("publish");
        });
        thread::sleep(Duration::from_millis(40));
        assert_eq!(slots.frame_index(), 2);
        slots.release_front();
        thread.join().expect("writer");
        assert_eq!(slots.frame_index(), 3);
        assert!(!slots.held());
    }
}
