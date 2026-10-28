import {
  COMPARE_HEIGHT,
  COMPARE_STATUS_EDITOR,
  COMPARE_STATUS_REFERENCE,
  COMPARE_WIDTH,
  compareClearBytes,
  compareVertexFloats,
} from "./compare-scene.js";

export function mountCanvasSource() {
  return mountCanvas.toString();
}

export function setComparePending(slot, state) {
  slot.dataset.compareState = state;
  delete slot.dataset.compareReady;
  delete slot.dataset.compareError;
}

export function setCompareReady(slot, state) {
  slot.dataset.compareState = state;
  slot.dataset.compareReady = "true";
  delete slot.dataset.compareError;
}

export function setCompareError(slot, state, message) {
  slot.dataset.compareState = state;
  slot.dataset.compareReady = "error";
  slot.dataset.compareError = message;
}

export function mountCanvas(slot, width, height) {
  slot.replaceChildren();
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  canvas.className = "viewport";
  canvas.style.width = `${width}px`;
  canvas.style.height = `${height}px`;
  canvas.style.transform = "none";
  canvas.style.zoom = "1";
  canvas.setAttribute("aria-label", "GLGPU compare framebuffer");
  slot.append(canvas);
  return canvas;
}

function paint(slot, width, height, pixels, state, status) {
  const canvas = mountCanvas(slot, width, height);
  const context = canvas.getContext("2d", { alpha: false, willReadFrequently: true });
  context.putImageData(new ImageData(new Uint8ClampedArray(pixels), width, height), 0, 0);
  document.getElementById("compare-status").textContent = status;
  document.getElementById("viewport-badge").textContent = `RGBA8 · ${width}×${height}`;
  setCompareReady(slot, state);
}

export function presentReference(slot, compareFrameRgba8) {
  setComparePending(slot, "reference");
  try {
    const pixels = compareFrameRgba8(
      COMPARE_WIDTH,
      COMPARE_HEIGHT,
      compareClearBytes(),
      compareVertexFloats(),
    );
    paint(slot, COMPARE_WIDTH, COMPARE_HEIGHT, pixels, "reference", COMPARE_STATUS_REFERENCE);
  } catch (error) {
    setCompareError(slot, "reference", String(error));
    throw error;
  }
}

export function presentEditorFrame(slot, width, height, pixels) {
  paint(slot, width, height, pixels, "editor", COMPARE_STATUS_EDITOR);
}

export function readCanvasPixel(x, y) {
  const canvas = document.querySelector("#compare-slot canvas");
  const context = canvas.getContext("2d");
  return new Uint8Array(context.getImageData(x, y, 1, 1).data);
}
