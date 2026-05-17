import { mkdirSync } from "node:fs";
import path from "node:path";

const DEFAULT_WINDOW_CLOSE_TIMEOUT_MS = 2_000;

export interface LifecycleEnvironment {
  HAMLET_DATA_DIR?: string;
}

export interface UserDataPathApp {
  readonly isPackaged: boolean;
  setPath(name: "userData", path: string): void;
}

export interface SingleInstanceLockApp {
  readonly isPackaged: boolean;
  requestSingleInstanceLock(): boolean;
  on(event: "second-instance", listener: (...args: unknown[]) => void): unknown;
}

export interface FocusableWindow {
  isDestroyed(): boolean;
  isMinimized(): boolean;
  isVisible(): boolean;
  restore(): void;
  show(): void;
  focus(): void;
}

export interface ShutdownWindow {
  isDestroyed(): boolean;
  close(): void;
  destroy(): void;
  once(event: "closed", listener: () => void): unknown;
}

export interface ShutdownResource {
  close(): Promise<void>;
}

export interface ShutdownLifecycleResources {
  windows: readonly ShutdownWindow[];
  staticRendererServer: ShutdownResource | null;
}

export interface ShutdownLifecycleOptions {
  windowCloseTimeoutMs?: number;
}

export interface ConfigureUserDataDirectoryOptions {
  env?: LifecycleEnvironment;
  ensureDirectory?: (directory: string) => void;
}

export function resolveUserDataDirectoryOverride(
  env: LifecycleEnvironment,
  app: Pick<UserDataPathApp, "isPackaged">,
): string | null {
  const rawOverride = env.HAMLET_DATA_DIR;
  if (rawOverride === undefined || rawOverride.trim() === "") return null;

  // Packaged production keeps Electron's default userData path so the app has
  // one stable profile and one single-instance lock. Development and tests can
  // still opt into isolated profiles with the existing HAMLET_DATA_DIR convention.
  if (app.isPackaged) return null;

  return path.resolve(rawOverride);
}

export function configureUserDataDirectory(
  app: UserDataPathApp,
  options: ConfigureUserDataDirectoryOptions = {},
): string | null {
  const userDataDirectory = resolveUserDataDirectoryOverride(options.env ?? process.env, app);
  if (userDataDirectory === null) return null;

  const ensureDirectory = options.ensureDirectory ?? ensureDirectoryExists;
  ensureDirectory(userDataDirectory);
  app.setPath("userData", userDataDirectory);
  return userDataDirectory;
}

export function shouldAcquireSingleInstanceLock(
  app: Pick<SingleInstanceLockApp, "isPackaged">,
): boolean {
  return app.isPackaged;
}

export function configureSingleInstanceLock(
  app: SingleInstanceLockApp,
  onSecondInstance: () => void,
): boolean {
  if (!shouldAcquireSingleInstanceLock(app)) return true;
  if (!app.requestSingleInstanceLock()) return false;

  app.on("second-instance", onSecondInstance);
  return true;
}

export function focusExistingWindow(windows: readonly FocusableWindow[]): boolean {
  const window = windows.find((candidate) => !candidate.isDestroyed());
  if (window === undefined) return false;

  if (window.isMinimized()) window.restore();
  if (!window.isVisible()) window.show();
  window.focus();
  return true;
}

export async function shutdownLifecycleResources(
  resources: ShutdownLifecycleResources,
  options: ShutdownLifecycleOptions = {},
): Promise<void> {
  const windowCloseTimeoutMs = options.windowCloseTimeoutMs ?? DEFAULT_WINDOW_CLOSE_TIMEOUT_MS;
  const closeWindows = resources.windows.map((window) =>
    closeShutdownWindow(window, windowCloseTimeoutMs),
  );
  const closeServer = resources.staticRendererServer?.close() ?? Promise.resolve();

  await Promise.all([...closeWindows, closeServer]);
}

export function formatFatalStartupError(error: unknown): string {
  const detail = error instanceof Error ? error.message : String(error);
  return `Hamlet could not start. ${detail}`;
}

function ensureDirectoryExists(directory: string): void {
  mkdirSync(directory, { recursive: true });
}

function closeShutdownWindow(window: ShutdownWindow, timeoutMs: number): Promise<void> {
  if (window.isDestroyed()) return Promise.resolve();

  return new Promise<void>((resolve) => {
    let settled = false;
    const timeout = setTimeout(() => {
      if (!window.isDestroyed()) window.destroy();
      finish();
    }, timeoutMs);

    const finish = () => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      resolve();
    };

    window.once("closed", finish);
    window.close();
    if (window.isDestroyed()) finish();
  });
}
