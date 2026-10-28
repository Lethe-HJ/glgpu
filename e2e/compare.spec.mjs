import fs from "node:fs";
import path from "node:path";
import { test, expect } from "playwright/test";
import { PNG } from "pngjs";
import {
  COMPARE_ERROR_WEBGL,
  COMPARE_ERROR_WEBGPU,
  COMPARE_HEIGHT,
  COMPARE_WIDTH,
} from "../web/compare-scene.js";

const BYTE_LENGTH = COMPARE_WIDTH * COMPARE_HEIGHT * 4;

function writePng(file, rgba) {
  const png = new PNG({ width: COMPARE_WIDTH, height: COMPARE_HEIGHT });
  png.data.set(rgba);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, PNG.sync.write(png));
}

function writeArtifacts(name, reference, native) {
  const directory = path.resolve("target/e2e-compare", name);
  fs.mkdirSync(directory, { recursive: true });
  writePng(path.join(directory, "reference.png"), reference);
  writePng(path.join(directory, "native.png"), native);
  const diff = Buffer.alloc(BYTE_LENGTH);
  let mismatches = 0;
  let first = null;
  for (let index = 0; index < BYTE_LENGTH; index += 4) {
    const same =
      reference[index] === native[index] &&
      reference[index + 1] === native[index + 1] &&
      reference[index + 2] === native[index + 2] &&
      reference[index + 3] === native[index + 3];
    if (same) {
      diff.set(reference.subarray(index, index + 4), index);
    } else {
      diff.set([255, 0, 255, 255], index);
      mismatches += 1;
      if (!first) {
        const pixel = index / 4;
        const x = pixel % COMPARE_WIDTH;
        const y = Math.floor(pixel / COMPARE_WIDTH);
        first = `first: x=${x} y=${y} reference=${reference[index]},${reference[index + 1]},${reference[index + 2]},${reference[index + 3]} native=${native[index]},${native[index + 1]},${native[index + 2]},${native[index + 3]}`;
      }
    }
  }
  writePng(path.join(directory, "diff.png"), diff);
  fs.writeFileSync(
    path.join(directory, "mismatch.txt"),
    `mismatch count: ${mismatches}\n${first ?? "first: none"}\n`,
  );
  return mismatches;
}

async function readDisplayed(page, state) {
  return page.evaluate(async ({ state, width, height }) => {
    const slot = document.querySelector("#compare-slot");
    const source = slot.querySelector("canvas");
    if (source.width !== width || source.height !== height) {
      throw new Error(`canvas buffer is ${source.width}x${source.height}`);
    }
    const read = document.createElement("canvas");
    read.width = width;
    read.height = height;
    const context = read.getContext("2d", { willReadFrequently: true });
    context.imageSmoothingEnabled = false;
    context.drawImage(source, 0, 0);
    const drawn = Array.from(context.getImageData(0, 0, width, height).data);
    let direct = null;
    if (state === "reference") {
      const existing = source.getContext("2d");
      direct = Array.from(existing.getImageData(0, 0, width, height).data);
    }
    return { drawn, direct };
  }, { state, width: COMPARE_WIDTH, height: COMPARE_HEIGHT });
}

async function waitReady(page, state) {
  await page.waitForFunction(
    (expected) => {
      const slot = document.querySelector("#compare-slot");
      const ready = slot?.getAttribute("data-compare-ready");
      return slot?.getAttribute("data-compare-state") === expected && (ready === "true" || ready === "error");
    },
    state,
    { timeout: 30_000 },
  );
  const error = await page.locator("#compare-slot").getAttribute("data-compare-error");
  const ready = await page.locator("#compare-slot").getAttribute("data-compare-ready");
  if (ready === "error") {
    throw new Error(error || `${state} compare failed`);
  }
}

async function capturePage(page, pathname) {
  const errors = [];
  const onError = (error) => errors.push(String(error));
  page.on("pageerror", onError);
  await page.goto(pathname);
  expect(page.url().includes("threads=")).toBe(false);
  await waitReady(page, "reference");
  const referenceRead = await readDisplayed(page, "reference");
  expect(referenceRead.drawn.length).toBe(BYTE_LENGTH);
  expect(referenceRead.direct).toEqual(referenceRead.drawn);
  await page.getByRole("button", { name: "原生" }).click();
  await waitReady(page, "native");
  const nativeRead = await readDisplayed(page, "native");
  expect(nativeRead.drawn.length).toBe(BYTE_LENGTH);
  expect(nativeRead.direct).toBe(null);
  page.off("pageerror", onError);
  return {
    reference: Uint8Array.from(referenceRead.drawn),
    native: Uint8Array.from(nativeRead.drawn),
    errors,
  };
}

test("both pages match native RGBA exactly", async ({ page }) => {
  const failures = [];
  const pages = [
    { name: "webgpu", pathname: "/", nativeError: COMPARE_ERROR_WEBGPU },
    { name: "webgl", pathname: "/webgl.html", nativeError: COMPARE_ERROR_WEBGL },
  ];
  for (const entry of pages) {
    try {
      const captured = await capturePage(page, entry.pathname);
      const mismatches = writeArtifacts(entry.name, captured.reference, captured.native);
      if (mismatches !== 0) {
        failures.push(`${entry.name} has ${mismatches} mismatched pixels`);
      }
      if (captured.errors.length !== 0) {
        failures.push(`${entry.name} page errors: ${captured.errors.join("\n")}`);
      }
    } catch (error) {
      const message = String(error);
      failures.push(
        message.includes(entry.nativeError) ? `${entry.name}: ${entry.nativeError}` : `${entry.name}: ${message}`,
      );
    }
  }
  expect(failures, failures.join("\n")).toEqual([]);
});
