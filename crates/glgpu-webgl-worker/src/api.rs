use std::sync::Mutex;

use glgpu_webgl::DoubleBuffer;
use wasm_bindgen::prelude::*;

static BUFFER: Mutex<Option<DoubleBuffer>> = Mutex::new(None);

fn with_buffer<R>(use_buffer: impl FnOnce(&DoubleBuffer) -> R) -> Result<R, JsValue> {
    let guard = BUFFER.lock().expect("present buffer");
    let buffer = guard
        .as_ref()
        .ok_or_else(|| JsValue::from_str("present buffer is not installed"))?;
    Ok(use_buffer(buffer))
}

#[wasm_bindgen]
pub fn shared_memory() -> JsValue {
    wasm_bindgen::memory()
}

#[wasm_bindgen]
pub fn install(width: u32, height: u32) -> Result<(), JsValue> {
    let buffer =
        DoubleBuffer::new(width, height).map_err(|error| JsValue::from_str(&error.to_string()))?;
    *BUFFER.lock().expect("present buffer") = Some(buffer);
    Ok(())
}

#[wasm_bindgen]
pub fn submit_essl() -> Result<(), JsValue> {
    with_buffer(|buffer| buffer.run_commanded_essl())?
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub fn frame_index() -> Result<u32, JsValue> {
    with_buffer(|buffer| buffer.frame_index())
}

#[wasm_bindgen]
pub fn readable_slot() -> Result<u32, JsValue> {
    with_buffer(|buffer| buffer.readable_slot())
}

#[wasm_bindgen]
pub fn slot_byte_len() -> Result<u32, JsValue> {
    with_buffer(|buffer| buffer.slot_byte_len() as u32)
}

#[wasm_bindgen]
pub fn slot_ptr(slot: u32) -> Result<u32, JsValue> {
    with_buffer(|buffer| buffer.slot_ptr(slot) as u32)
}

#[wasm_bindgen]
pub fn readable_addr() -> Result<u32, JsValue> {
    with_buffer(|buffer| buffer.readable_atomic_addr() as usize as u32)
}

#[wasm_bindgen]
pub fn frame_addr() -> Result<u32, JsValue> {
    with_buffer(|buffer| buffer.frame_atomic_addr() as usize as u32)
}

#[wasm_bindgen]
pub fn held_addr() -> Result<u32, JsValue> {
    with_buffer(|buffer| buffer.held_atomic_addr() as usize as u32)
}

#[wasm_bindgen]
pub fn command_addr() -> Result<u32, JsValue> {
    with_buffer(|buffer| buffer.command_ptr() as u32)
}

#[wasm_bindgen]
pub fn command_cap() -> Result<u32, JsValue> {
    with_buffer(|buffer| buffer.command_cap() as u32)
}

#[wasm_bindgen]
pub fn command_len_addr() -> Result<u32, JsValue> {
    with_buffer(|buffer| buffer.command_len_addr() as usize as u32)
}

#[wasm_bindgen]
pub fn command_seq_addr() -> Result<u32, JsValue> {
    with_buffer(|buffer| buffer.command_seq_addr() as usize as u32)
}

#[wasm_bindgen]
pub fn hold_slot(slot: u32) -> Result<(), JsValue> {
    with_buffer(|buffer| buffer.hold_slot(slot))
}

#[wasm_bindgen]
pub fn release_front() -> Result<(), JsValue> {
    with_buffer(|buffer| buffer.release_front())
}
