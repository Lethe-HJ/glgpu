// Main-thread present for a worker that owns two pixel slots in one
// SharedArrayBuffer. The wake message carries no pixel payload. Command bytes
// are written into the worker buffer before the wake.

export function openPresentWorker(scriptUrl) {
  const state = {
    info: null,
    worker: null,
    pending: null,
    busy: false,
    failed: false,
  };

  function view() {
    return new Int32Array(state.info.buffer);
  }

  function presentFront() {
    const info = state.info;
    const canvas = document.querySelector("canvas.viewport");
    if (!info || !(info.buffer instanceof SharedArrayBuffer) || !canvas) {
      return null;
    }
    const atoms = view();
    const readable = Atomics.load(atoms, info.readableAddr / 4) >>> 0;
    const slot = readable === 0 ? info.slot0 : info.slot1;
    Atomics.store(atoms, info.heldAddr / 4, (readable + 1) >>> 0);
    const shared = new Uint8Array(info.buffer, slot, info.slotBytes);
    const copy = new Uint8ClampedArray(shared);
    try {
      const context = canvas.getContext("2d");
      const image = new ImageData(copy, canvas.width, canvas.height);
      context.putImageData(image, 0, 0);
    } finally {
      Atomics.store(atoms, info.heldAddr / 4, 0);
    }
    return copy;
  }

  function fail(message) {
    console.error("GLGPU present worker", message);
    state.failed = true;
    state.busy = false;
    state.pending = null;
    if (state.worker) {
      state.worker.terminate();
      state.worker = null;
    }
    if (typeof window.glgpuReplayLocal === "function") {
      window.glgpuReplayLocal();
    }
  }

  function pump() {
    if (!state.worker || !state.info || !state.pending || state.busy) {
      return;
    }
    const job = state.pending;
    state.pending = null;
    state.busy = true;
    try {
      job.write(state.info);
      Atomics.add(view(), state.info.commandSeqAddr / 4, 1);
      state.worker.postMessage(job.wake);
    } catch (error) {
      fail(error);
    }
  }

  if (globalThis.crossOriginIsolated && typeof SharedArrayBuffer === "function") {
    state.worker = new Worker(scriptUrl, { type: "module" });
    state.worker.onmessage = (event) => {
      const message = event.data;
      if (!message) {
        return;
      }
      if (message.kind === "ready") {
        if (!(message.buffer instanceof SharedArrayBuffer)) {
          fail("worker memory is not a SharedArrayBuffer");
          return;
        }
        state.info = message;
        pump();
        return;
      }
      if (message.kind === "frame") {
        let copy = null;
        try {
          copy = presentFront();
        } catch (error) {
          fail(error);
          return;
        }
        state.busy = false;
        if (typeof window.glgpuOnPresentedFrame === "function") {
          window.glgpuOnPresentedFrame(message, copy);
        }
        pump();
        return;
      }
      if (message.kind === "error") {
        fail(message.message);
      }
    };
    state.worker.postMessage({ kind: "boot" });
  }

  return {
    queueCommand(write, wake) {
      if (!state.worker || state.failed) {
        return false;
      }
      state.pending = { write, wake };
      pump();
      return true;
    },
  };
}
