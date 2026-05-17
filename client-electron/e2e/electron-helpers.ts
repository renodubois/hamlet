import {
  _electron as electron,
  expect,
  type ElectronApplication,
  type Page,
} from "@playwright/test";
import { mkdir, rm } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const clientElectronRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const builtAppPath = clientElectronRoot;
const rendererOriginPattern = /^http:\/\/127\.0\.0\.1:1422(?:\/|$)/;
const electronMediaSwitches = [
  "--use-fake-device-for-media-stream",
  "--use-fake-ui-for-media-stream",
  "--autoplay-policy=no-user-gesture-required",
] as const;

export interface LaunchedElectronApp {
  app: ElectronApplication;
  close: () => Promise<void>;
}

export async function launchBuiltElectronApp(userDataDir: string): Promise<LaunchedElectronApp> {
  await mkdir(userDataDir, { recursive: true });

  try {
    const app = await electron.launch({
      cwd: clientElectronRoot,
      args: [...electronMediaSwitches, builtAppPath],
      env: electronEnvironment({
        ELECTRON_DISABLE_SECURITY_WARNINGS: "true",
        HAMLET_DATA_DIR: userDataDir,
        HAMLET_RENDERER_URL: undefined,
      }),
    });

    return {
      app,
      close: () => closeLaunchedApp(app, userDataDir),
    };
  } catch (error) {
    await rm(userDataDir, { recursive: true, force: true });
    throw error;
  }
}

export async function launchPackagedElectronApp(
  executablePath: string,
): Promise<LaunchedElectronApp> {
  const app = await electron.launch({
    executablePath,
    cwd: path.dirname(executablePath),
    args: [...electronMediaSwitches],
    env: electronEnvironment({
      ELECTRON_DISABLE_SECURITY_WARNINGS: "true",
      HAMLET_RENDERER_URL: undefined,
    }),
  });

  return {
    app,
    close: () => closeElectronApp(app),
  };
}

export async function firstHamletWindow(app: ElectronApplication): Promise<Page> {
  const page = await app.firstWindow();
  await page.waitForLoadState("domcontentloaded");
  await expect(page).toHaveURL(rendererOriginPattern, { timeout: 30_000 });
  return page;
}

export async function loginAsSeededUser(page: Page): Promise<void> {
  await expect(page.getByRole("heading", { name: /sign in/i })).toBeVisible({
    timeout: 30_000,
  });

  await page.getByPlaceholder("Server URL").fill("http://127.0.0.1:3030");
  await page.getByPlaceholder("Username").fill("baipas");
  await page.getByPlaceholder("Password").fill("password");
  await page.getByRole("button", { name: /sign in/i }).click();

  await expect(page.locator("aside").getByText("baipas")).toBeVisible({ timeout: 30_000 });
  await expect(page.getByRole("navigation", { name: /channels/i })).toBeVisible();
}

export async function openGeneralChannel(page: Page): Promise<void> {
  const generalLink = page.getByRole("navigation", { name: /channels/i }).getByText("# general");
  await expect(generalLink).toBeVisible({ timeout: 10_000 });

  const generalHref = await generalLink.getAttribute("href");
  if (!generalHref) throw new Error("general channel link is missing an href");

  await page.goto(new URL(generalHref, page.url()).href);
  await expect(page).toHaveURL(new RegExp(`${escapeRegExp(generalHref)}$`));
  await expect(page.getByRole("heading", { name: /^#\s*general$/i })).toBeVisible();
}

export async function sendMessage(page: Page, text: string): Promise<void> {
  const input = page.getByPlaceholder(/send a new message/i);
  await input.fill(text);
  await input.press("Enter");
  await expect(page.getByText(text)).toBeVisible({ timeout: 10_000 });
}

export async function installExternalUrlCapture(app: ElectronApplication): Promise<void> {
  await app.evaluate(({ shell }) => {
    const global = globalThis as typeof globalThis & {
      __hamletExternalUrls?: string[];
    };
    global.__hamletExternalUrls = [];

    const shellWithMutableOpenExternal = shell as {
      openExternal: (url: string, options?: unknown) => Promise<void>;
    };

    shellWithMutableOpenExternal.openExternal = async (url: string) => {
      global.__hamletExternalUrls?.push(url);
    };
  });
}

export async function externalUrls(app: ElectronApplication): Promise<string[]> {
  return app.evaluate(() => {
    const global = globalThis as typeof globalThis & {
      __hamletExternalUrls?: string[];
    };
    return [...(global.__hamletExternalUrls ?? [])];
  });
}

function electronEnvironment(
  overrides: Record<string, string | undefined>,
): Record<string, string> {
  const env: Record<string, string> = {};

  for (const [key, value] of Object.entries(process.env)) {
    if (value !== undefined) env[key] = value;
  }

  for (const [key, value] of Object.entries(overrides)) {
    if (value === undefined) delete env[key];
    else env[key] = value;
  }

  return env;
}

async function closeLaunchedApp(app: ElectronApplication, userDataDir: string): Promise<void> {
  try {
    await closeElectronApp(app);
  } finally {
    await rm(userDataDir, { recursive: true, force: true });
  }
}

async function closeElectronApp(app: ElectronApplication): Promise<void> {
  try {
    await app.close();
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (!message.includes("Application has been closed")) throw error;
  }
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
