import type { BrowserWindowConstructorOptions, Session, WebPreferences } from "electron";
import { ELECTRON_WINDOW_TITLE, STATIC_RENDERER_ORIGIN } from "./constants";

export interface RendererEnvironment {
  HAMLET_RENDERER_URL?: string;
}

export type UrlPolicyDecision =
  | { action: "allow" }
  | { action: "open-external"; url: string }
  | { action: "block" };

export interface PermissionRequestDecisionInput {
  permission: string;
  requestingUrl?: string;
  securityOrigin?: string;
  mediaTypes?: readonly string[];
}

export interface PermissionCheckDecisionInput {
  permission: string;
  requestingOrigin?: string;
  requestingUrl?: string;
  securityOrigin?: string;
  mediaType?: string;
}

export type SessionPermissionPolicy = Pick<
  Session,
  | "setDevicePermissionHandler"
  | "setDisplayMediaRequestHandler"
  | "setPermissionCheckHandler"
  | "setPermissionRequestHandler"
>;

export function resolveRendererUrl(env: RendererEnvironment = process.env): URL {
  const rendererUrlOverride = env.HAMLET_RENDERER_URL?.trim();
  const url = parseRendererUrl(
    rendererUrlOverride === "" || rendererUrlOverride === undefined
      ? STATIC_RENDERER_ORIGIN
      : rendererUrlOverride,
  );
  if (!isTrustedRendererUrl(url)) {
    throw new Error(
      `Refusing to load untrusted renderer origin "${url.origin}". Expected "${STATIC_RENDERER_ORIGIN}".`,
    );
  }
  return url;
}

export function isTrustedRendererUrl(input: string | URL): boolean {
  try {
    const url = typeof input === "string" ? new URL(input) : input;
    return url.origin === STATIC_RENDERER_ORIGIN;
  } catch {
    return false;
  }
}

export function decideTopLevelNavigation(input: string | URL): UrlPolicyDecision {
  const url = parseUrl(input);
  if (!url) return { action: "block" };
  if (isTrustedRendererUrl(url)) return { action: "allow" };
  if (isHttpUrl(url)) return { action: "open-external", url: url.href };
  return { action: "block" };
}

export function decideWindowOpen(input: string | URL): UrlPolicyDecision {
  const decision = decideTopLevelNavigation(input);
  // The renderer is a single controlled app surface. Same-origin routes should
  // be handled in-place by the router, not by creating child BrowserWindows.
  if (decision.action === "allow") return { action: "block" };
  return decision;
}

export function shouldAllowPermissionRequest(input: PermissionRequestDecisionInput): boolean {
  if (!isTrustedPermissionOrigin(input)) return false;

  switch (input.permission) {
    case "media":
      return isAudioOnlyMediaRequest(input.mediaTypes);
    case "speaker-selection":
      return true;
    default:
      return false;
  }
}

export function shouldAllowPermissionCheck(input: PermissionCheckDecisionInput): boolean {
  if (!isTrustedPermissionOrigin(input)) return false;

  switch (input.permission) {
    case "media":
      return isMediaDeviceCheck(input.mediaType);
    case "speaker-selection":
      return true;
    default:
      return false;
  }
}

export function installSessionPermissionPolicy(session: SessionPermissionPolicy): void {
  session.setPermissionRequestHandler((_webContents, permission, callback, details) => {
    const mediaDetails = details as {
      requestingUrl?: string;
      securityOrigin?: string;
      mediaTypes?: readonly string[];
    };
    callback(
      shouldAllowPermissionRequest({
        permission,
        requestingUrl: mediaDetails.requestingUrl,
        securityOrigin: mediaDetails.securityOrigin,
        mediaTypes: mediaDetails.mediaTypes,
      }),
    );
  });

  session.setPermissionCheckHandler((_webContents, permission, requestingOrigin, details) =>
    shouldAllowPermissionCheck({
      permission,
      requestingOrigin,
      requestingUrl: details.requestingUrl,
      securityOrigin: details.securityOrigin,
      mediaType: details.mediaType,
    }),
  );

  // Device APIs such as HID/USB/serial and screen capture are not needed for
  // Hamlet voice. Keep them denied even if Chromium reaches these specialized
  // handlers without first consulting the generic permission handlers above.
  session.setDevicePermissionHandler(() => false);
  session.setDisplayMediaRequestHandler((_request, callback) => {
    callback({});
  });
}

export function createSecureWebPreferences(preloadPath: string): WebPreferences {
  return {
    preload: preloadPath,
    nodeIntegration: false,
    contextIsolation: true,
    sandbox: true,
    webSecurity: true,
    allowRunningInsecureContent: false,
    webviewTag: false,
  };
}

export function createMainWindowOptions(
  preloadPath: string,
  iconPath?: string,
): BrowserWindowConstructorOptions {
  return {
    title: ELECTRON_WINDOW_TITLE,
    width: 1280,
    height: 800,
    minWidth: 960,
    minHeight: 600,
    show: false,
    backgroundColor: "#ffffff",
    ...(iconPath === undefined ? {} : { icon: iconPath }),
    webPreferences: createSecureWebPreferences(preloadPath),
  };
}

function parseRendererUrl(value: string): URL {
  try {
    return new URL(value);
  } catch (cause) {
    throw new Error(`Invalid renderer URL "${value}".`, { cause });
  }
}

function parseUrl(input: string | URL): URL | null {
  try {
    return typeof input === "string" ? new URL(input) : input;
  } catch {
    return null;
  }
}

function isHttpUrl(url: URL): boolean {
  return url.protocol === "http:" || url.protocol === "https:";
}

function isTrustedPermissionOrigin(
  input: PermissionRequestDecisionInput | PermissionCheckDecisionInput,
): boolean {
  const origin =
    input.securityOrigin ??
    ("requestingOrigin" in input ? input.requestingOrigin : undefined) ??
    input.requestingUrl;
  return isTrustedRendererUrl(origin ?? "");
}

function isAudioOnlyMediaRequest(mediaTypes: readonly string[] | undefined): boolean {
  return (
    mediaTypes !== undefined &&
    mediaTypes.length > 0 &&
    mediaTypes.every((type) => type === "audio")
  );
}

function isMediaDeviceCheck(mediaType: string | undefined): boolean {
  // Electron reports device-enumeration checks as `unknown` (or leaves the
  // field absent in some Chromium paths). Allow those for the trusted renderer
  // so Voice Settings can list devices, but keep capture requests constrained
  // by the stricter request handler above.
  return mediaType === undefined || mediaType === "audio" || mediaType === "unknown";
}
