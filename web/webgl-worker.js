// Worker kernel for webgl-mock. The page canvas stays on the main thread.
// Pixel slots and the ESSL command live in this module's shared memory.

let wasm;

async function boot() {
  const module = await import("./pkg-webgl-worker/glgpu_webgl_worker.js");
  await module.default();
  wasm = module;
  const memory = module.shared_memory();
  wasm.install(720, 480);
  const readableAddr = wasm.readable_addr();
  const frameAddr = wasm.frame_addr();
  const heldAddr = wasm.held_addr();
  const commandLenAddr = wasm.command_len_addr();
  const commandSeqAddr = wasm.command_seq_addr();
  const commandAddr = wasm.command_addr();
  for (const addr of [readableAddr, frameAddr, heldAddr, commandLenAddr, commandSeqAddr, commandAddr]) {
    if (addr % 4 !== 0) {
      throw new Error(`shared address ${addr} is not 4-byte aligned`);
    }
  }
  self.postMessage({
    kind: "ready",
    buffer: memory.buffer,
    readableAddr,
    frameAddr,
    heldAddr,
    commandLenAddr,
    commandSeqAddr,
    commandAddr,
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
  if (message.kind === "essl") {
    try {
      wasm.submit_essl();
      self.postMessage({ kind: "frame", frame: wasm.frame_index() });
    } catch (error) {
      self.postMessage({ kind: "error", message: String(error) });
    }
  }
};
