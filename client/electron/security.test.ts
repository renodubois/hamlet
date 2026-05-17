import { describe, expect, it } from "vitest";
import { DEFAULT_RENDERER_DEV_ORIGIN, ELECTRON_WINDOW_TITLE } from "./constants";
import {
  createMainWindowOptions,
  createSecureWebPreferences,
  decideTopLevelNavigation,
  decideWindowOpen,
  installSessionPermissionPolicy,
  isTrustedRendererUrl,
  resolveRendererUrl,
  shouldAllowPermissionCheck,
  shouldAllowPermissionRequest,
  type SessionPermissionPolicy,
} from "./security";

describe("Electron renderer loading", () => {
  it("loads the fixed Electron renderer dev origin by default", () => {
    expect(resolveRendererUrl({}).href).toBe(`${DEFAULT_RENDERER_DEV_ORIGIN}/`);
  });

  it("accepts paths on the trusted renderer origin", () => {
    expect(isTrustedRendererUrl(`${DEFAULT_RENDERER_DEV_ORIGIN}/channels/1`)).toBe(true);
  });

  it("treats blank renderer URL overrides as the fixed packaged origin", () => {
    expect(resolveRendererUrl({ HAMLET_RENDERER_URL: "  " }).href).toBe(
      `${DEFAULT_RENDERER_DEV_ORIGIN}/`,
    );
  });

  it("treats malformed renderer URLs as untrusted", () => {
    expect(isTrustedRendererUrl("not a url")).toBe(false);
  });

  it("rejects origin-spoofing renderer URLs", () => {
    expect(isTrustedRendererUrl(`${DEFAULT_RENDERER_DEV_ORIGIN}.evil.test/channels/1`)).toBe(false);
    expect(isTrustedRendererUrl("http://127.0.0.1.evil.test:1422/")).toBe(false);
    expect(isTrustedRendererUrl("http://127.0.0.1:14220/")).toBe(false);
  });

  it("rejects untrusted renderer origins", () => {
    expect(() => resolveRendererUrl({ HAMLET_RENDERER_URL: "http://127.0.0.1:1420" })).toThrow(
      DEFAULT_RENDERER_DEV_ORIGIN,
    );
    expect(() => resolveRendererUrl({ HAMLET_RENDERER_URL: "http://localhost:1422" })).toThrow(
      DEFAULT_RENDERER_DEV_ORIGIN,
    );
    expect(() => resolveRendererUrl({ HAMLET_RENDERER_URL: "https://127.0.0.1:1422" })).toThrow(
      DEFAULT_RENDERER_DEV_ORIGIN,
    );
  });
});

describe("Electron shell URL policy", () => {
  it("allows top-level navigation within the trusted renderer origin", () => {
    expect(decideTopLevelNavigation(`${DEFAULT_RENDERER_DEV_ORIGIN}/channels/1`)).toEqual({
      action: "allow",
    });
  });

  it("opens valid external HTTP and HTTPS navigations in the system browser", () => {
    expect(decideTopLevelNavigation("https://example.com/docs?q=1")).toEqual({
      action: "open-external",
      url: "https://example.com/docs?q=1",
    });
    expect(decideTopLevelNavigation("http://example.test")).toEqual({
      action: "open-external",
      url: "http://example.test/",
    });
  });

  it("blocks unsafe or unknown navigation schemes", () => {
    for (const url of [
      "file:///etc/passwd",
      "javascript:alert(1)",
      "data:text/html,<h1>owned</h1>",
      "hamlet://channels/1",
      "mailto:security@example.test",
      "not a url",
    ]) {
      expect(decideTopLevelNavigation(url)).toEqual({ action: "block" });
    }
  });

  it("denies popups while still opening validated external links externally", () => {
    expect(decideWindowOpen(`${DEFAULT_RENDERER_DEV_ORIGIN}/channels/1`)).toEqual({
      action: "block",
    });
    expect(decideWindowOpen("https://example.com/invite")).toEqual({
      action: "open-external",
      url: "https://example.com/invite",
    });
    expect(decideWindowOpen("ftp://example.com/file.txt")).toEqual({ action: "block" });
  });
});

describe("Electron permission policy", () => {
  const trustedRequest = {
    requestingUrl: `${DEFAULT_RENDERER_DEV_ORIGIN}/channels/voice`,
    securityOrigin: DEFAULT_RENDERER_DEV_ORIGIN,
  };

  it("allows only trusted audio media requests needed by voice", () => {
    expect(
      shouldAllowPermissionRequest({
        ...trustedRequest,
        permission: "media",
        mediaTypes: ["audio"],
      }),
    ).toBe(true);

    expect(
      shouldAllowPermissionRequest({
        ...trustedRequest,
        permission: "media",
        mediaTypes: ["video"],
      }),
    ).toBe(false);
    expect(
      shouldAllowPermissionRequest({
        ...trustedRequest,
        permission: "media",
        mediaTypes: ["audio", "video"],
      }),
    ).toBe(false);
    expect(
      shouldAllowPermissionRequest({
        ...trustedRequest,
        permission: "media",
      }),
    ).toBe(false);
  });

  it("allows trusted media-device checks but rejects camera checks", () => {
    expect(
      shouldAllowPermissionCheck({
        permission: "media",
        requestingOrigin: DEFAULT_RENDERER_DEV_ORIGIN,
        mediaType: "audio",
      }),
    ).toBe(true);
    expect(
      shouldAllowPermissionCheck({
        permission: "media",
        requestingOrigin: DEFAULT_RENDERER_DEV_ORIGIN,
        mediaType: "unknown",
      }),
    ).toBe(true);
    expect(
      shouldAllowPermissionCheck({
        permission: "media",
        requestingOrigin: DEFAULT_RENDERER_DEV_ORIGIN,
        mediaType: "video",
      }),
    ).toBe(false);
  });

  it("allows trusted speaker selection requests for recoverable output-device settings", () => {
    expect(
      shouldAllowPermissionRequest({
        ...trustedRequest,
        permission: "speaker-selection",
      }),
    ).toBe(true);
    expect(
      shouldAllowPermissionCheck({
        permission: "speaker-selection",
        requestingOrigin: DEFAULT_RENDERER_DEV_ORIGIN,
      }),
    ).toBe(true);
  });

  it("denies trusted origins for unrelated capabilities by default", () => {
    for (const permission of [
      "clipboard-read",
      "display-capture",
      "fullscreen",
      "geolocation",
      "notifications",
      "openExternal",
      "window-management",
    ]) {
      expect(
        shouldAllowPermissionRequest({
          ...trustedRequest,
          permission,
          mediaTypes: ["audio"],
        }),
      ).toBe(false);
      expect(
        shouldAllowPermissionCheck({
          permission,
          requestingOrigin: DEFAULT_RENDERER_DEV_ORIGIN,
          mediaType: "audio",
        }),
      ).toBe(false);
    }
  });

  it("denies media permissions from untrusted origins", () => {
    expect(
      shouldAllowPermissionRequest({
        permission: "media",
        requestingUrl: "https://evil.example/voice",
        securityOrigin: "https://evil.example",
        mediaTypes: ["audio"],
      }),
    ).toBe(false);
    expect(
      shouldAllowPermissionCheck({
        permission: "media",
        requestingOrigin: "https://evil.example",
        mediaType: "audio",
      }),
    ).toBe(false);
  });

  it("uses the requesting security origin instead of the embedding app origin", () => {
    expect(
      shouldAllowPermissionRequest({
        permission: "media",
        requestingUrl: `${DEFAULT_RENDERER_DEV_ORIGIN}/channels/voice`,
        securityOrigin: "https://evil.example",
        mediaTypes: ["audio"],
      }),
    ).toBe(false);
    expect(
      shouldAllowPermissionCheck({
        permission: "media",
        requestingOrigin: DEFAULT_RENDERER_DEV_ORIGIN,
        securityOrigin: "https://evil.example",
        mediaType: "audio",
      }),
    ).toBe(false);
  });

  it("installs Electron session handlers for media allowlists and default-denies", () => {
    type CapturedPermissionRequestHandler = (
      webContents: never,
      permission: string,
      callback: (granted: boolean) => void,
      details: {
        isMainFrame: boolean;
        requestingUrl: string;
        securityOrigin?: string;
        mediaTypes?: string[];
      },
    ) => void;
    type CapturedPermissionCheckHandler = (
      webContents: null,
      permission: string,
      requestingOrigin: string,
      details: { isMainFrame: boolean },
    ) => boolean;

    let requestHandler: CapturedPermissionRequestHandler | null | undefined;
    let checkHandler: CapturedPermissionCheckHandler | null | undefined;
    let deviceDecision: boolean | undefined;
    let displayDecision: unknown;

    const fakeSession: SessionPermissionPolicy = {
      setPermissionRequestHandler: (handler) => {
        requestHandler = handler as CapturedPermissionRequestHandler | null;
      },
      setPermissionCheckHandler: (handler) => {
        checkHandler = handler as CapturedPermissionCheckHandler | null;
      },
      setDevicePermissionHandler: (handler) => {
        deviceDecision = handler?.({} as never);
      },
      setDisplayMediaRequestHandler: (handler) => {
        handler?.(
          {
            frame: null,
            securityOrigin: DEFAULT_RENDERER_DEV_ORIGIN,
            videoRequested: true,
            audioRequested: true,
            userGesture: true,
          },
          (streams) => {
            displayDecision = streams;
          },
        );
      },
    };

    installSessionPermissionPolicy(fakeSession);

    let microphoneDecision: boolean | undefined;
    requestHandler?.(
      {} as never,
      "media",
      (granted) => {
        microphoneDecision = granted;
      },
      {
        isMainFrame: true,
        requestingUrl: `${DEFAULT_RENDERER_DEV_ORIGIN}/channels/voice`,
        securityOrigin: DEFAULT_RENDERER_DEV_ORIGIN,
        mediaTypes: ["audio"],
      },
    );

    expect(microphoneDecision).toBe(true);
    expect(
      checkHandler?.(null, "geolocation", DEFAULT_RENDERER_DEV_ORIGIN, { isMainFrame: true }),
    ).toBe(false);
    expect(deviceDecision).toBe(false);
    expect(displayDecision).toEqual({});
  });
});

describe("Electron BrowserWindow security defaults", () => {
  it("keeps Node and insecure content out of the renderer", () => {
    expect(createSecureWebPreferences("/tmp/preload.cjs")).toMatchObject({
      preload: "/tmp/preload.cjs",
      nodeIntegration: false,
      contextIsolation: true,
      sandbox: true,
      webSecurity: true,
      allowRunningInsecureContent: false,
      webviewTag: false,
    });
  });

  it("uses alpha metadata, icons, and secure preferences when constructing the app window", () => {
    expect(createMainWindowOptions("/tmp/preload.cjs", "/tmp/icon.png")).toMatchObject({
      title: ELECTRON_WINDOW_TITLE,
      icon: "/tmp/icon.png",
      backgroundColor: "#ffffff",
      webPreferences: {
        preload: "/tmp/preload.cjs",
        nodeIntegration: false,
        contextIsolation: true,
        sandbox: true,
        webSecurity: true,
        allowRunningInsecureContent: false,
        webviewTag: false,
      },
    });
  });
});
