// Worker kernel for webgpu-mock. The page canvas stays on the main thread.
// The triangle command and the color slots live in this module's shared memory.
// Depth stays in the device framebuffer inside this worker.

let wasm;

function exportAddrs() {
  return {
    readableAddr: wasm.readable_addr(),
    frameAddr: wasm.frame_addr(),
    heldAddr: wasm.held_addr(),
    commandLenAddr: wasm.command_len_addr(),
    commandSeqAddr: wasm.command_seq_addr(),
    commandAddr: wasm.command_addr(),
  };
}

async function boot() {
  const module = await import("./pkg-webgpu-worker/glgpu_webgpu_worker.js");
  await module.default();
  wasm = module;
  const memory = module.shared_memory();
  wasm.install(720, 480);
  const addrs = exportAddrs();
  for (const addr of Object.values(addrs)) {
    if (addr % 4 !== 0) {
      throw new Error(`shared address ${addr} is not 4-byte aligned`);
    }
  }
  self.postMessage({
    kind: "ready",
    buffer: memory.buffer,
    ...addrs,
    commandCap: wasm.command_cap(),
    slotBytes: wasm.slot_byte_len(),
    slot0: wasm.slot_ptr(0),
    slot1: wasm.slot_ptr(1),
  });
}

self.onmessage = (event) => {
  const message = event.data;
  if (!message || message.kind === "boot") {
    boot().catch((error) => {
      self.postMessage({ kind: "error", message: String(error) });
    });
    return;
  }
  if (message.kind === "triangle") {
    try {
      wasm.submit_triangle();
      self.postMessage({
        kind: "frame",
        frame: wasm.frame_index(),
        serial: wasm.last_serial(),
        hash: wasm.last_hash(),
        history: wasm.last_history(),
      });
    } catch (error) {
      self.postMessage({ kind: "error", message: String(error) });
    }
  }
};
