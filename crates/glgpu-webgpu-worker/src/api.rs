use std::cell::RefCell;

use wasm_bindgen::prelude::*;

use crate::PresentSession;

thread_local! {
    static SESSION: RefCell<Option<PresentSession>> = const { RefCell::new(None) };
}

fn with_session<R>(use_session: impl FnOnce(&PresentSession) -> R) -> Result<R, JsValue> {
    SESSION.with(|session| {
        session
            .borrow()
            .as_ref()
            .map(use_session)
            .ok_or_else(|| JsValue::from_str("present session is not installed"))
    })
}

fn with_session_mut<R>(use_session: impl FnOnce(&mut PresentSession) -> R) -> Result<R, JsValue> {
    SESSION.with(|session| {
        session
            .borrow_mut()
            .as_mut()
            .map(use_session)
            .ok_or_else(|| JsValue::from_str("present session is not installed"))
    })
}

#[wasm_bindgen]
pub fn shared_memory() -> JsValue {
    wasm_bindgen::memory()
}

#[wasm_bindgen]
pub fn install(width: u32, height: u32) -> Result<(), JsValue> {
    let session = PresentSession::new(width, height).map_err(|error| JsValue::from_str(&error))?;
    SESSION.with(|slot| *slot.borrow_mut() = Some(session));
    Ok(())
}

#[wasm_bindgen]
pub fn submit_triangle() -> Result<(), JsValue> {
    with_session_mut(|session| session.submit_command())?.map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen]
pub fn frame_index() -> Result<u32, JsValue> {
    with_session(|session| session.slots().frame_index())
}

#[wasm_bindgen]
pub fn last_serial() -> Result<f64, JsValue> {
    with_session(|session| session.last_serial() as f64)
}

#[wasm_bindgen]
pub fn last_hash() -> Result<String, JsValue> {
    with_session(|session| format!("{:016x}", session.last_hash()))
}

#[wasm_bindgen]
pub fn last_history() -> Result<String, JsValue> {
    with_session(|session| session.last_history().to_owned())
}

#[wasm_bindgen]
pub fn slot_byte_len() -> Result<u32, JsValue> {
    with_session(|session| session.slots().slot_byte_len() as u32)
}

#[wasm_bindgen]
pub fn slot_ptr(slot: u32) -> Result<u32, JsValue> {
    with_session(|session| session.slots().slot_ptr(slot) as u32)
}

#[wasm_bindgen]
pub fn readable_addr() -> Result<u32, JsValue> {
    with_session(|session| session.slots().readable_atomic_addr() as usize as u32)
}

#[wasm_bindgen]
pub fn frame_addr() -> Result<u32, JsValue> {
    with_session(|session| session.slots().frame_atomic_addr() as usize as u32)
}

#[wasm_bindgen]
pub fn held_addr() -> Result<u32, JsValue> {
    with_session(|session| session.slots().held_atomic_addr() as usize as u32)
}

#[wasm_bindgen]
pub fn command_addr() -> Result<u32, JsValue> {
    with_session(|session| session.slots().command_ptr() as u32)
}

#[wasm_bindgen]
pub fn command_cap() -> Result<u32, JsValue> {
    with_session(|session| session.slots().command_cap() as u32)
}

#[wasm_bindgen]
pub fn command_len_addr() -> Result<u32, JsValue> {
    with_session(|session| session.slots().command_len_addr() as usize as u32)
}

#[wasm_bindgen]
pub fn command_seq_addr() -> Result<u32, JsValue> {
    with_session(|session| session.slots().command_seq_addr() as usize as u32)
}
