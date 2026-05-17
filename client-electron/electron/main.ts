import { app, BrowserWindow, dialog, shell, session } from "electron";
import path from "node:path";
import { ELECTRON_WINDOW_TITLE } from "./constants";
import {
  configureSingleInstanceLock,
  configureUserDataDirectory,
  focusExistingWindow,
  formatFatalStartupError,
  shutdownLifecycleResources,
} from "./lifecycle";
import {
  createMainWindowOptions,
  decideTopLevelNavigation,
  decideWindowOpen,
  installSessionPermissionPolicy,
  resolveRendererUrl,
} from "./security";
import {
  resolveRendererDistPath,
  shouldServeStaticRenderer,
  startStaticRendererServer,
  type StaticRendererServer,
} from "./static-server";

let mainWindow: BrowserWindow | null = null;
let staticRendererServer: StaticRendererServer | null = null;
let cleanQuitStarted = false;

interface TopLevelNavigationEvent {
  preventDefault: () => void;
}

export async function createMainWindow(): Promise<BrowserWindow> {
  const rendererUrl = resolveRendererUrl();
  const window = new BrowserWindow(createMainWindowOptions(preloadScriptPath(), appIconPath()));
  mainWindow = window;

  window.once("ready-to-show", () => {
    if (!window.isDestroyed()) window.show();
  });

  window.on("closed", () => {
    if (mainWindow === window) mainWindow = null;
  });

  window.webContents.setWindowOpenHandler(({ url }) => {
    const decision = decideWindowOpen(url);
    if (decision.action === "open-external") openExternalUrl(decision.url);
    return { action: "deny" };
  });

  window.webContents.on("will-navigate", (event, url) => {
    handleTopLevelNavigation(event, url);
  });
  window.webContents.on("will-redirect", (event, url, _isInPlace, isMainFrame) => {
    handleTopLevelNavigation(event, url, isMainFrame);
  });

  await window.loadURL(rendererUrl.href);
  return window;
}

async function startApp(): Promise<void> {
  app.setName(ELECTRON_WINDOW_TITLE);
  configureUserDataDirectory(app);

  if (!configureSingleInstanceLock(app, handleSecondInstanceLaunch)) {
    requestCleanQuit(0);
    return;
  }

  await app.whenReady();
  installSessionPermissionPolicy(session.defaultSession);
  if (shouldServeStaticRenderer()) {
    staticRendererServer = await startStaticRendererServer({
      rootDir: resolveRendererDistPath(app.getAppPath()),
    });
  }
  await createMainWindow();

  app.on("activate", () => {
    if (!cleanQuitStarted && BrowserWindow.getAllWindows().length === 0) void createMainWindow();
  });
}

function preloadScriptPath(): string {
  return path.join(__dirname, "preload.cjs");
}

function appIconPath(): string {
  return path.join(app.getAppPath(), "packaging", "icons", "icon.png");
}

function handleTopLevelNavigation(
  event: TopLevelNavigationEvent,
  url: string,
  isMainFrame = true,
): void {
  if (!isMainFrame) return;

  const decision = decideTopLevelNavigation(url);
  if (decision.action === "allow") return;

  event.preventDefault();
  if (decision.action === "open-external") openExternalUrl(decision.url);
}

function openExternalUrl(url: string): void {
  void shell.openExternal(url).catch((error: unknown) => {
    console.error(`Failed to open external URL: ${url}`, error);
  });
}

function handleSecondInstanceLaunch(): void {
  if (cleanQuitStarted) return;

  const windows =
    mainWindow === null
      ? BrowserWindow.getAllWindows()
      : [mainWindow, ...BrowserWindow.getAllWindows().filter((window) => window !== mainWindow)];

  if (!focusExistingWindow(windows) && app.isReady()) void createMainWindow();
}

function requestCleanQuit(exitCode: number): void {
  if (cleanQuitStarted) return;
  cleanQuitStarted = true;
  process.exitCode = exitCode;

  void shutdownAppResources()
    .catch((error: unknown) => {
      process.exitCode = 1;
      console.error("Failed to shut down Hamlet cleanly.", error);
    })
    .finally(() => {
      app.exit(typeof process.exitCode === "number" ? process.exitCode : exitCode);
    });
}

function shutdownAppResources(): Promise<void> {
  const server = staticRendererServer;
  staticRendererServer = null;
  mainWindow = null;

  return shutdownLifecycleResources({
    windows: BrowserWindow.getAllWindows(),
    staticRendererServer: server,
  });
}

function reportFatalStartupError(error: unknown): void {
  const message = formatFatalStartupError(error);
  console.error(message);
  if (error instanceof Error && error.stack !== undefined) console.error(error.stack);

  try {
    if (app.isReady()) dialog.showErrorBox(ELECTRON_WINDOW_TITLE, message);
  } catch (reportError: unknown) {
    console.error("Failed to show Hamlet startup error dialog.", reportError);
  }
}

app.on("before-quit", (event) => {
  event.preventDefault();
  if (cleanQuitStarted) return;
  requestCleanQuit(typeof process.exitCode === "number" ? process.exitCode : 0);
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") requestCleanQuit(0);
});

void startApp().catch((error: unknown) => {
  reportFatalStartupError(error);
  requestCleanQuit(1);
});
