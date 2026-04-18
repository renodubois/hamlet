import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: process.env.CI ? [["html", { open: "never" }], ["list"]] : "list",

  use: {
    baseURL: "http://localhost:1420",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },

  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        // Honor a pre-installed Chromium binary when one is available (e.g.
        // Claude Code's web sandbox bakes Playwright browsers under /opt and
        // blocks the normal download from cdn.playwright.dev). Unset → use
        // Playwright's regular download-and-cache behavior.
        launchOptions: {
          executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH || undefined,
        },
      },
    },
  ],

  webServer: [
    {
      command: "cargo run",
      cwd: "../server",
      url: "http://localhost:3030/channels",
      reuseExistingServer: !process.env.CI,
      timeout: 180_000,
      stdout: "pipe",
      stderr: "pipe",
    },
    {
      command: "npm run dev",
      url: "http://localhost:1420",
      reuseExistingServer: !process.env.CI,
      timeout: 60_000,
    },
  ],
});
