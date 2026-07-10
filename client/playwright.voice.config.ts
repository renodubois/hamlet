import { defineConfig, devices } from "@playwright/test";
import { hamletEnv } from "./playwright.env";

const rendererHost = hamletEnv.HAMLET_RENDERER_HOST ?? "127.0.0.1";
const rendererPort = hamletEnv.HAMLET_RENDERER_PORT ?? "1422";
const rendererUrl = hamletEnv.HAMLET_RENDERER_URL ?? `http://${rendererHost}:${rendererPort}`;

export default defineConfig({
  testDir: "./e2e",
  testMatch: "voice-browser.spec.ts",
  fullyParallel: false,
  forbidOnly: !!hamletEnv.CI,
  retries: 0,
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
        permissions: ["microphone", "camera"],
        launchOptions: {
          executablePath: hamletEnv.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH || undefined,
          args: [
            "--use-fake-device-for-media-stream",
            "--use-fake-ui-for-media-stream",
            "--enable-usermedia-screen-capturing",
            "--auto-select-desktop-capture-source=Entire screen",
            "--allow-http-screen-capture",
            "--autoplay-policy=no-user-gesture-required",
            // The browser repro is explicitly 127.0.0.1-based. Make any
            // accidental LiveKit `localhost` signaling URL fail like it does
            // on hosts where localhost resolves to an unreachable address.
            "--host-resolver-rules=MAP localhost 192.0.2.1,EXCLUDE 127.0.0.1",
          ],
        },
      },
    },
    {
      name: "firefox",
      use: {
        ...devices["Desktop Firefox"],
        launchOptions: {
          firefoxUserPrefs: {
            "media.navigator.permission.disabled": true,
            "media.navigator.streams.fake": true,
          },
        },
      },
    },
  ],

  webServer: {
    command: "pnpm run dev",
    url: rendererUrl,
    reuseExistingServer: !hamletEnv.CI,
    timeout: 60_000,
    env: {
      ...hamletEnv,
      HAMLET_RENDERER_HOST: rendererHost,
      HAMLET_RENDERER_PORT: rendererPort,
    },
  },
});
