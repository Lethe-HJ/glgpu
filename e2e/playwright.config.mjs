import { defineConfig } from "playwright/test";

export default defineConfig({
  testDir: ".",
  timeout: 120_000,
  workers: 1,
  reporter: "line",
  use: {
    baseURL: process.env.GLGPU_E2E_BASE,
    headless: true,
    viewport: { width: 1280, height: 800 },
    deviceScaleFactor: 1,
    channel: "chrome",
    launchOptions: {
      args: ["--enable-unsafe-webgpu"],
    },
  },
});
