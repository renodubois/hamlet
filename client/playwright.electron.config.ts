import { defineConfig } from "@playwright/test";
import { hamletEnv } from "./playwright.env";

const serverUrl =
  hamletEnv.HAMLET_SERVER_URL ??
  hamletEnv.VITE_HAMLET_DEFAULT_SERVER_URL ??
  "http://127.0.0.1:3030";
const serverPort = new URL(serverUrl).port || "3030";
const serverBindAddr = hamletEnv.HAMLET_BIND_ADDR ?? `127.0.0.1:${serverPort}`;

export default defineConfig({
  testDir: "./e2e",
  testMatch: "**/electron-smoke.electron.spec.ts",
  fullyParallel: false,
  forbidOnly: !!hamletEnv.CI,
  retries: hamletEnv.CI ? 2 : 0,
  workers: 1,
  reporter: hamletEnv.CI ? [["html", { open: "never" }], ["list"]] : "list",
  timeout: 90_000,

  use: {
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },

  webServer: {
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
});
