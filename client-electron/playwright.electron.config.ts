import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  testMatch: "**/electron-smoke.electron.spec.ts",
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: process.env.CI ? [["html", { open: "never" }], ["list"]] : "list",
  timeout: 90_000,

  use: {
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },

  webServer: {
    command: "cargo run",
    cwd: "../server",
    url: "http://127.0.0.1:3030/channels",
    reuseExistingServer: !process.env.CI,
    timeout: 180_000,
    stdout: "pipe",
    stderr: "pipe",
  },
});
