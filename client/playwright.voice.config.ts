import { defineConfig, devices } from "@playwright/test";

const rendererUrl = "http://127.0.0.1:1422";

export default defineConfig({
  testDir: "./e2e",
  testMatch: "voice-browser.spec.ts",
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: 0,
  workers: 1,
  reporter: process.env.CI ? [["html", { open: "never" }], ["list"]] : "list",

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
        permissions: ["microphone"],
        launchOptions: {
          executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH || undefined,
          args: [
            "--use-fake-device-for-media-stream",
            "--use-fake-ui-for-media-stream",
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
    command: "npm run dev",
    url: rendererUrl,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
