import { defineConfig, devices } from "@playwright/test";
import { hamletEnv } from "./playwright.env";

const serverUrl =
  hamletEnv.HAMLET_SERVER_URL ??
  hamletEnv.VITE_HAMLET_DEFAULT_SERVER_URL ??
  "http://127.0.0.1:3030";
const rendererHost = hamletEnv.HAMLET_RENDERER_HOST ?? "127.0.0.1";
const rendererPort = hamletEnv.HAMLET_RENDERER_PORT ?? "1422";
const rendererUrl = hamletEnv.HAMLET_RENDERER_URL ?? `http://${rendererHost}:${rendererPort}`;
const serverBindAddr =
  hamletEnv.HAMLET_BIND_ADDR ?? `127.0.0.1:${new URL(serverUrl).port || "3030"}`;

export default defineConfig({
  testDir: "./e2e",
  testIgnore: "**/*.electron.spec.ts",
  fullyParallel: false,
  forbidOnly: !!hamletEnv.CI,
  retries: hamletEnv.CI ? 2 : 0,
  workers: 1,
  reporter: hamletEnv.CI ? [["html", { open: "never" }], ["list"]] : "list",

  use: {
    baseURL: rendererUrl,
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
          executablePath: hamletEnv.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH || undefined,
        },
      },
    },
  ],

  webServer: [
    {
      command: "cargo run",
      cwd: "../server",
      url: `${serverUrl}/channels`,
      reuseExistingServer: !hamletEnv.CI,
      timeout: 180_000,
      stdout: "pipe",
      stderr: "pipe",
      env: {
        ...hamletEnv,
        HAMLET_BIND_ADDR: serverBindAddr,
      },
    },
    {
      command: "npm run dev",
      url: rendererUrl,
      reuseExistingServer: !hamletEnv.CI,
      timeout: 60_000,
      env: {
        ...hamletEnv,
        HAMLET_RENDERER_HOST: rendererHost,
        HAMLET_RENDERER_PORT: rendererPort,
        VITE_HAMLET_DEFAULT_SERVER_URL: serverUrl,
      },
    },
  ],
});
