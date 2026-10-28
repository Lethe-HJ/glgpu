import {
  COMPARE_CLEAR,
  COMPARE_ERROR_WEBGPU,
  COMPARE_HEIGHT,
  COMPARE_STATUS_NATIVE,
  COMPARE_VERTICES,
  COMPARE_WIDTH,
} from "./compare-scene.js";
import { mountCanvas, setCompareError, setComparePending, setCompareReady } from "./compare-present.js";

export const WEBGPU_FLIP_Y = false;

export function triangleVertexData() {
  const data = new Float32Array(COMPARE_VERTICES.length * 6);
  COMPARE_VERTICES.forEach((vertex, index) => {
    const offset = index * 6;
    data[offset] = vertex.ndcX;
    data[offset + 1] = WEBGPU_FLIP_Y ? -vertex.ndcY : vertex.ndcY;
    data[offset + 2] = vertex.r;
    data[offset + 3] = vertex.g;
    data[offset + 4] = vertex.b;
    data[offset + 5] = vertex.a;
  });
  return data;
}

export function webgpuShaderSource() {
  const clipZ = COMPARE_VERTICES[0].clipZ;
  const clipW = COMPARE_VERTICES[0].clipW;
  return `
struct VertexOutput {
  @builtin(position) position: vec4f,
  @location(0) color: vec4f,
};
@vertex
fn vs_main(@location(0) ndc: vec2f, @location(1) color: vec4f) -> VertexOutput {
  var output: VertexOutput;
  output.position = vec4f(ndc.x, ndc.y, ${clipZ}, ${clipW});
  output.color = color;
  return output;
}
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {
  return input.color;
}
`;
}

export async function presentNativeWebgpu(slot) {
  setComparePending(slot, "native");
  try {
    if (!navigator.gpu) {
      throw new Error(COMPARE_ERROR_WEBGPU);
    }
    const adapter = await navigator.gpu.requestAdapter({ powerPreference: "high-performance" });
    if (!adapter) {
      throw new Error(COMPARE_ERROR_WEBGPU);
    }
    const device = await adapter.requestDevice();
    const format = navigator.gpu.getPreferredCanvasFormat();
    if (format !== "rgba8unorm" && format !== "bgra8unorm") {
      throw new Error(COMPARE_ERROR_WEBGPU);
    }
    const canvas = mountCanvas(slot, COMPARE_WIDTH, COMPARE_HEIGHT);
    const context = canvas.getContext("webgpu");
    if (!context) {
      throw new Error(COMPARE_ERROR_WEBGPU);
    }
    context.configure({ device, format, alphaMode: "opaque" });
    const shader = device.createShaderModule({ code: webgpuShaderSource() });
    const pipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: {
        module: shader,
        entryPoint: "vs_main",
        buffers: [
          {
            arrayStride: 24,
            attributes: [
              { shaderLocation: 0, offset: 0, format: "float32x2" },
              { shaderLocation: 1, offset: 8, format: "float32x4" },
            ],
          },
        ],
      },
      fragment: {
        module: shader,
        entryPoint: "fs_main",
        targets: [{ format }],
      },
      primitive: { topology: "triangle-list", cullMode: "none" },
      multisample: { count: 1 },
    });
    const vertices = triangleVertexData();
    const buffer = device.createBuffer({
      size: vertices.byteLength,
      usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
    });
    device.queue.writeBuffer(buffer, 0, vertices);
    const encoder = device.createCommandEncoder();
    const pass = encoder.beginRenderPass({
      colorAttachments: [
        {
          view: context.getCurrentTexture().createView(),
          clearValue: {
            r: COMPARE_CLEAR[0] / 255,
            g: COMPARE_CLEAR[1] / 255,
            b: COMPARE_CLEAR[2] / 255,
            a: COMPARE_CLEAR[3] / 255,
          },
          loadOp: "clear",
          storeOp: "store",
        },
      ],
    });
    pass.setPipeline(pipeline);
    pass.setVertexBuffer(0, buffer);
    pass.draw(3);
    pass.end();
    device.queue.submit([encoder.finish()]);
    await device.queue.onSubmittedWorkDone();
    document.getElementById("compare-status").textContent = COMPARE_STATUS_NATIVE;
    document.getElementById("viewport-badge").textContent = "RGBA8 · 320×240";
    setCompareReady(slot, "native");
  } catch (error) {
    const message = String(error && error.message ? error.message : error);
    setCompareError(
      slot,
      "native",
      message.includes(COMPARE_ERROR_WEBGPU) ? COMPARE_ERROR_WEBGPU : message,
    );
  }
}
