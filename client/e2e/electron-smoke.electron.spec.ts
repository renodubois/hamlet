import { expect, test as base, type ElectronApplication, type Page } from "@playwright/test";
import {
  externalUrls,
  firstHamletWindow,
  installExternalUrlCapture,
  launchBuiltElectronApp,
  loginAsSeededUser,
  openGeneralChannel,
  sendMessage,
} from "./electron-helpers";

type ElectronFixtures = {
  electronApp: ElectronApplication;
  appWindow: Page;
};

const test = base.extend<ElectronFixtures>({
  electronApp: async ({ browserName: _browserName }, use, testInfo) => {
    const launched = await launchBuiltElectronApp(testInfo.outputPath("electron-user-data"));
    try {
      await use(launched.app);
    } finally {
      await launched.close();
    }
  },

  appWindow: async ({ electronApp }, use) => {
    await use(await firstHamletWindow(electronApp));
  },
});

test("logs in, auto-navigates, sends a message, clears the composer, and reloads a deep route", async ({
  appWindow,
}) => {
  await loginAsSeededUser(appWindow);

  await expect(appWindow).toHaveURL(/\/channel\/\d+$/);
  await expect(
    appWindow.getByRole("navigation", { name: /channels/i }).getByText("# general"),
  ).toBeVisible();
  await expect(appWindow.getByRole("heading", { name: /^#\s*\S/i })).toBeVisible();

  await openGeneralChannel(appWindow);

  const marker = `electron smoke ${Date.now()}`;
  await sendMessage(appWindow, marker);
  await expect(appWindow.getByPlaceholder(/send a new message/i)).toHaveValue("");

  const deepRoute = appWindow.url();
  await appWindow.reload();
  await expect(appWindow).toHaveURL(deepRoute);
  await expect(appWindow.locator("aside").getByText("baipas")).toBeVisible({ timeout: 30_000 });
  await expect(appWindow.getByRole("heading", { name: /^#\s*general$/i })).toBeVisible();
  await expect(appWindow.getByText(marker)).toBeVisible({ timeout: 10_000 });
});

test("opens external links outside the app window and blocks unsafe navigation", async ({
  electronApp,
  appWindow,
}) => {
  await installExternalUrlCapture(electronApp);
  await loginAsSeededUser(appWindow);
  await openGeneralChannel(appWindow);

  const channelUrl = appWindow.url();
  const externalUrl = `https://example.com/hamlet-electron-smoke-${Date.now()}`;
  await sendMessage(appWindow, externalUrl);

  await appWindow.getByRole("link", { name: externalUrl }).click();
  await expect.poll(() => externalUrls(electronApp), { timeout: 5_000 }).toContain(externalUrl);
  expect(electronApp.windows()).toHaveLength(1);
  await expect(appWindow).toHaveURL(channelUrl);

  const unsafeUrl = "file:///tmp/hamlet-electron-unsafe-navigation";
  await appWindow.evaluate((url) => {
    const link = document.createElement("a");
    link.href = url;
    link.textContent = "unsafe navigation smoke";
    document.body.append(link);
    link.click();
  }, unsafeUrl);
  await appWindow.waitForTimeout(500);

  await expect(appWindow).toHaveURL(channelUrl);
  expect(electronApp.windows()).toHaveLength(1);
  await expect.poll(() => externalUrls(electronApp)).not.toContain(unsafeUrl);
});

test("opens the voice settings media-permission path without crashing", async ({
  electronApp,
  appWindow,
}) => {
  await loginAsSeededUser(appWindow);

  await appWindow.getByRole("button", { name: /settings/i }).click();
  const dialog = appWindow.getByRole("dialog", { name: /settings/i });
  await expect(dialog).toBeVisible();

  await dialog.getByRole("tab", { name: /voice & video/i }).click();
  await expect(dialog.getByRole("tab", { name: /voice & video/i })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(dialog.getByLabel(/input device/i)).toBeVisible();
  await expect(dialog.getByRole("button", { name: /test microphone/i })).toBeVisible();

  await expect(
    dialog
      .getByRole("status")
      .or(dialog.getByRole("alert"))
      .or(dialog.getByText(/system default/i))
      .first(),
  ).toBeVisible();
  expect(electronApp.windows()).toHaveLength(1);
});
