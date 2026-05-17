import path from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  configureSingleInstanceLock,
  configureUserDataDirectory,
  focusExistingWindow,
  formatFatalStartupError,
  resolveUserDataDirectoryOverride,
  shouldAcquireSingleInstanceLock,
  shutdownLifecycleResources,
  type FocusableWindow,
  type ShutdownWindow,
  type SingleInstanceLockApp,
  type UserDataPathApp,
} from "./lifecycle";

afterEach(() => {
  vi.useRealTimers();
});

describe("Electron lifecycle profile isolation", () => {
  it("applies HAMLET_DATA_DIR to development and test userData paths", () => {
    const app = createUserDataPathApp(false);
    const ensuredDirectories: string[] = [];

    const configuredPath = configureUserDataDirectory(app, {
      env: { HAMLET_DATA_DIR: "profiles/alice" },
      ensureDirectory: (directory) => ensuredDirectories.push(directory),
    });

    const expectedPath = path.resolve("profiles/alice");
    expect(configuredPath).toBe(expectedPath);
    expect(ensuredDirectories).toEqual([expectedPath]);
    expect(app.setPaths).toEqual([{ name: "userData", path: expectedPath }]);
  });

  it("leaves packaged production on Electron's default userData path", () => {
    const app = createUserDataPathApp(true);

    expect(
      configureUserDataDirectory(app, {
        env: { HAMLET_DATA_DIR: "profiles/packaged" },
        ensureDirectory: () => {
          throw new Error("packaged overrides should not create directories");
        },
      }),
    ).toBeNull();
    expect(app.setPaths).toEqual([]);
    expect(
      resolveUserDataDirectoryOverride({ HAMLET_DATA_DIR: "profiles/packaged" }, app),
    ).toBeNull();
  });

  it("ignores missing or blank data directory overrides", () => {
    const app = createUserDataPathApp(false);

    expect(configureUserDataDirectory(app, { env: {} })).toBeNull();
    expect(configureUserDataDirectory(app, { env: { HAMLET_DATA_DIR: "  " } })).toBeNull();
    expect(app.setPaths).toEqual([]);
  });
});

describe("Electron lifecycle single-instance decisions", () => {
  it("requests and wires the single-instance lock only for packaged production", () => {
    const devApp = createSingleInstanceLockApp(false, true);
    expect(shouldAcquireSingleInstanceLock(devApp)).toBe(false);
    expect(configureSingleInstanceLock(devApp, () => undefined)).toBe(true);
    expect(devApp.requestCount).toBe(0);
    expect(devApp.listener).toBeNull();

    let focusCount = 0;
    const packagedApp = createSingleInstanceLockApp(true, true);
    expect(shouldAcquireSingleInstanceLock(packagedApp)).toBe(true);
    expect(
      configureSingleInstanceLock(packagedApp, () => {
        focusCount += 1;
      }),
    ).toBe(true);

    expect(packagedApp.requestCount).toBe(1);
    packagedApp.listener?.();
    expect(focusCount).toBe(1);
  });

  it("reports a failed packaged lock so the second process can exit before binding ports", () => {
    const app = createSingleInstanceLockApp(true, false);

    expect(configureSingleInstanceLock(app, () => undefined)).toBe(false);
    expect(app.requestCount).toBe(1);
    expect(app.listener).toBeNull();
  });
});

describe("Electron lifecycle window focus and shutdown", () => {
  it("restores, shows, and focuses the first usable existing window", () => {
    const destroyed = new FakeFocusableWindow({ destroyed: true });
    const target = new FakeFocusableWindow({ minimized: true, visible: false });

    expect(focusExistingWindow([destroyed, target])).toBe(true);

    expect(destroyed.focusCalls).toBe(0);
    expect(target.restoreCalls).toBe(1);
    expect(target.showCalls).toBe(1);
    expect(target.focusCalls).toBe(1);
  });

  it("returns false when no window can be focused", () => {
    expect(focusExistingWindow([new FakeFocusableWindow({ destroyed: true })])).toBe(false);
    expect(focusExistingWindow([])).toBe(false);
  });

  it("closes live windows and the static renderer resource during shutdown", async () => {
    const activeWindow = new FakeShutdownWindow();
    const destroyedWindow = new FakeShutdownWindow({ destroyed: true });
    let serverCloseCalls = 0;

    await shutdownLifecycleResources(
      {
        windows: [activeWindow, destroyedWindow],
        staticRendererServer: {
          async close() {
            serverCloseCalls += 1;
          },
        },
      },
      { windowCloseTimeoutMs: 25 },
    );

    expect(activeWindow.closeCalls).toBe(1);
    expect(activeWindow.destroyCalls).toBe(0);
    expect(destroyedWindow.closeCalls).toBe(0);
    expect(serverCloseCalls).toBe(1);
  });

  it("destroys a shutdown window that does not close before the timeout", async () => {
    vi.useFakeTimers();
    const stuckWindow = new FakeShutdownWindow({ emitClosedOnClose: false });

    const shutdown = shutdownLifecycleResources(
      { windows: [stuckWindow], staticRendererServer: null },
      { windowCloseTimeoutMs: 50 },
    );

    await vi.advanceTimersByTimeAsync(50);
    await shutdown;

    expect(stuckWindow.closeCalls).toBe(1);
    expect(stuckWindow.destroyCalls).toBe(1);
  });

  it("formats startup failures with a clear Hamlet launch prefix", () => {
    expect(formatFatalStartupError(new Error("Static renderer port is already in use."))).toBe(
      "Hamlet could not start. Static renderer port is already in use.",
    );
    expect(formatFatalStartupError("unknown failure")).toBe(
      "Hamlet could not start. unknown failure",
    );
  });
});

interface FakeUserDataPathApp extends UserDataPathApp {
  setPaths: { name: "userData"; path: string }[];
}

interface FakeSingleInstanceLockApp extends SingleInstanceLockApp {
  listener: (() => void) | null;
  requestCount: number;
}

function createUserDataPathApp(isPackaged: boolean): FakeUserDataPathApp {
  const app: FakeUserDataPathApp = {
    isPackaged,
    setPaths: [],
    setPath(name, userDataPath) {
      app.setPaths.push({ name, path: userDataPath });
    },
  };
  return app;
}

function createSingleInstanceLockApp(
  isPackaged: boolean,
  lockResult: boolean,
): FakeSingleInstanceLockApp {
  const app: FakeSingleInstanceLockApp = {
    isPackaged,
    listener: null,
    requestCount: 0,
    requestSingleInstanceLock() {
      app.requestCount += 1;
      return lockResult;
    },
    on(_event, listener) {
      app.listener = listener;
    },
  };
  return app;
}

class FakeFocusableWindow implements FocusableWindow {
  focusCalls = 0;
  restoreCalls = 0;
  showCalls = 0;

  private destroyed: boolean;
  private minimized: boolean;
  private visible: boolean;

  constructor(options: { destroyed?: boolean; minimized?: boolean; visible?: boolean } = {}) {
    this.destroyed = options.destroyed ?? false;
    this.minimized = options.minimized ?? false;
    this.visible = options.visible ?? true;
  }

  isDestroyed(): boolean {
    return this.destroyed;
  }

  isMinimized(): boolean {
    return this.minimized;
  }

  isVisible(): boolean {
    return this.visible;
  }

  restore(): void {
    this.restoreCalls += 1;
    this.minimized = false;
  }

  show(): void {
    this.showCalls += 1;
    this.visible = true;
  }

  focus(): void {
    this.focusCalls += 1;
  }
}

class FakeShutdownWindow implements ShutdownWindow {
  closeCalls = 0;
  destroyCalls = 0;

  private closedListener: (() => void) | null = null;
  private destroyed: boolean;
  private readonly emitClosedOnClose: boolean;

  constructor(options: { destroyed?: boolean; emitClosedOnClose?: boolean } = {}) {
    this.destroyed = options.destroyed ?? false;
    this.emitClosedOnClose = options.emitClosedOnClose ?? true;
  }

  isDestroyed(): boolean {
    return this.destroyed;
  }

  close(): void {
    this.closeCalls += 1;
    if (!this.emitClosedOnClose) return;
    this.destroyed = true;
    this.closedListener?.();
  }

  destroy(): void {
    this.destroyCalls += 1;
    this.destroyed = true;
    this.closedListener?.();
  }

  once(_event: "closed", listener: () => void): void {
    this.closedListener = listener;
  }
}
