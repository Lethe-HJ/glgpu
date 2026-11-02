//! Raster one screen-space triangle and publish its color into the back slot.

use std::fmt::Write as _;

use glgpu_core::{Extent2d, PixelCoord, Rgba8, ScreenVertex, Triangle};
use glgpu_trace::PixelEventOutcome;
use glgpu_webgpu::{CpuDevice, CpuGpuSubset, DeviceDescriptor};

use crate::slots::{ByteSlots, COMMAND_BYTES};

const CLEAR: Rgba8 = Rgba8::new(13, 18, 30, 255);

pub struct PresentSession {
    device: CpuDevice,
    slots: ByteSlots,
    last_serial: u64,
    last_hash: u64,
    last_history: String,
}

impl PresentSession {
    pub fn new(width: u32, height: u32) -> Result<Self, String> {
        let slots = ByteSlots::new(width, height)?;
        let device = CpuGpuSubset::new()
            .request_adapter()
            .request_device(DeviceDescriptor::new(Extent2d::new(width, height)))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            device,
            slots,
            last_serial: 0,
            last_hash: 0,
            last_history: String::new(),
        })
    }

    pub fn slots(&self) -> &ByteSlots {
        &self.slots
    }

    pub fn last_serial(&self) -> u64 {
        self.last_serial
    }

    pub fn last_hash(&self) -> u64 {
        self.last_hash
    }

    pub fn last_history(&self) -> &str {
        &self.last_history
    }

    pub fn submit_command(&mut self) -> Result<(), String> {
        let bytes = self.slots.command_bytes()?;
        if bytes.len() != COMMAND_BYTES {
            return Err(format!(
                "triangle command is {} bytes; expected {COMMAND_BYTES}",
                bytes.len()
            ));
        }
        let mut coords = [0_f32; 21];
        for (index, coord) in coords.iter_mut().enumerate() {
            *coord = f32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap());
        }
        let selected_x = u32::from_le_bytes(bytes[84..88].try_into().unwrap());
        let selected_y = u32::from_le_bytes(bytes[88..92].try_into().unwrap());
        self.submit_triangle(&coords, selected_x, selected_y)
    }

    pub fn submit_triangle(
        &mut self,
        coords: &[f32],
        selected_x: u32,
        selected_y: u32,
    ) -> Result<(), String> {
        if coords.len() != 21 {
            return Err("triangle command must hold 21 floats".to_owned());
        }
        let vertices = [
            screen_vertex(coords, 0)?,
            screen_vertex(coords, 1)?,
            screen_vertex(coords, 2)?,
        ];
        let selected = PixelCoord::new(selected_x, selected_y);
        let mut encoder = self.device.create_command_encoder();
        encoder
            .clear(CLEAR, 1.0)
            .map_err(|error| error.to_string())?;
        encoder
            .select_pixel(selected)
            .map_err(|error| error.to_string())?;
        encoder
            .draw_triangle(Triangle::from_vertices(
                1,
                vertices[0],
                vertices[1],
                vertices[2],
            ))
            .map_err(|error| error.to_string())?;
        let command = encoder.finish().map_err(|error| error.to_string())?;
        let report = self
            .device
            .queue_mut()
            .submit(command)
            .map_err(|error| error.to_string())?;
        let packed = self
            .device
            .queue()
            .readback_rgba8()
            .map_err(|error| error.to_string())?;
        self.slots.publish_bytes(&packed)?;
        self.last_serial = report.serial;
        self.last_hash = report.stable_hash64;
        self.last_history = history_text(
            self.device
                .queue()
                .selected_pixel_history()
                .ok_or_else(|| "selected-pixel history was not produced".to_owned())?,
        );
        Ok(())
    }
}

fn screen_vertex(coords: &[f32], index: usize) -> Result<ScreenVertex, String> {
    let base = index * 7;
    let chunk = coords
        .get(base..base + 7)
        .ok_or_else(|| "triangle vertex is incomplete".to_owned())?;
    Ok(ScreenVertex::new(
        chunk[0],
        chunk[1],
        chunk[2],
        Rgba8::new(
            channel(chunk[3]),
            channel(chunk[4]),
            channel(chunk[5]),
            channel(chunk[6]),
        ),
    ))
}

fn channel(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn history_text(history: &glgpu_trace::PixelHistory) -> String {
    let mut text = format!(
        "pixel ({}, {})\nfinal rgba({}, {}, {}, {}) · depth {:.6}\n",
        history.pixel.x,
        history.pixel.y,
        history.final_color.r,
        history.final_color.g,
        history.final_color.b,
        history.final_color.a,
        history.final_depth,
    );
    if history.events.is_empty() {
        text.push_str("no triangle reached this pixel");
        return text;
    }
    for event in &history.events {
        let outcome = match event.outcome {
            PixelEventOutcome::OutsideCoverage => "OutsideCoverage",
            PixelEventOutcome::DepthRejected => "DepthRejected",
            PixelEventOutcome::Written => "Written",
        };
        let _ = writeln!(
            text,
            "primitive {} · {outcome} · candidate depth {:?} · rgba({}, {}, {}, {})",
            event.primitive_id,
            event.candidate_depth,
            event.color_after.r,
            event.color_after.g,
            event.color_after.b,
            event.color_after.a,
        );
    }
    text
}

#[cfg(test)]
fn pack_triangle_command(coords: &[f32; 21], x: u32, y: u32) -> [u8; COMMAND_BYTES] {
    let mut bytes = [0_u8; COMMAND_BYTES];
    for (index, value) in coords.iter().enumerate() {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes[84..88].copy_from_slice(&x.to_le_bytes());
    bytes[88..92].copy_from_slice(&y.to_le_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commanded_triangle_lands_in_the_front_slot() {
        let mut session = PresentSession::new(8, 8).expect("session");
        let mut coords = [0_f32; 21];
        let raw = [
            (1.0, 1.0, 255.0, 0.0, 0.0),
            (7.0, 1.0, 255.0, 0.0, 0.0),
            (1.0, 7.0, 255.0, 0.0, 0.0),
        ];
        for (index, (x, y, r, g, b)) in raw.into_iter().enumerate() {
            let base = index * 7;
            coords[base] = x;
            coords[base + 1] = y;
            coords[base + 2] = 0.5;
            coords[base + 3] = r;
            coords[base + 4] = g;
            coords[base + 5] = b;
            coords[base + 6] = 255.0;
        }
        session
            .slots()
            .set_command_bytes(&pack_triangle_command(&coords, 2, 2))
            .expect("command");
        session.submit_command().expect("submit");
        assert_eq!(session.slots().frame_index(), 1);
        assert_eq!(session.last_serial(), 1);
        let front = session.slots().front_bytes();
        let offset = (2 * 8 + 2) * 4;
        assert_eq!(&front[offset..offset + 4], &[255, 0, 0, 255]);
        assert!(session.last_history().contains("pixel (2, 2)"));
        assert_ne!(session.last_hash(), 0);
    }
}
