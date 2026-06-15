import type { Session } from "electron";
import { isTrustedRendererUrl } from "./security";

export const DISPLAY_CAPTURE_TEST_AUTOMATION_ENV = "HAMLET_ELECTRON_TEST_DISPLAY_CAPTURE";
export const DISPLAY_CAPTURE_UNDER_TEST_ENV = "HAMLET_ELECTRON_UNDER_TEST";
export const DISPLAY_CAPTURE_TEST_HAMLET_WINDOW = "hamlet-window";
export const DISPLAY_CAPTURE_TEST_FIRST_SOURCE = "first-source";

export const DISPLAY_CAPTURE_SOURCE_OPTIONS = {
  types: ["screen", "window"],
  thumbnailSize: { width: 320, height: 180 },
  fetchWindowIcons: true,
} satisfies DisplayCaptureSourceOptions;

const DISPLAY_CAPTURE_PICKER_SCHEME = "hamlet-display-capture";
const SAFE_IMAGE_DATA_URL_PATTERN = /^data:image\/(?:png|jpeg|webp);base64,[a-z\d+/=]+$/i;
const SCREEN_SOURCE_PREFIX = "screen:";
const WINDOW_SOURCE_PREFIX = "window:";
const CURRENT_PROCESS_WINDOW_SUFFIX = ":1";

type ElectronDisplayMediaRequestHandler = NonNullable<
  Parameters<Session["setDisplayMediaRequestHandler"]>[0]
>;

type ElectronDisplayMediaStreams = Parameters<Parameters<ElectronDisplayMediaRequestHandler>[1]>[0];

export interface DisplayMediaRequest {
  frame: object | null;
  securityOrigin: string;
  videoRequested: boolean;
  audioRequested: boolean;
  userGesture: boolean;
}

export interface DisplayMediaStreams {
  video?: unknown;
  audio?: unknown;
  enableLocalEcho?: boolean;
}

export type DisplayCaptureTestAutomation =
  | typeof DISPLAY_CAPTURE_TEST_HAMLET_WINDOW
  | typeof DISPLAY_CAPTURE_TEST_FIRST_SOURCE;

export interface DisplayCaptureSourceImage {
  isEmpty(): boolean;
  resize(options: { width?: number; height?: number }): DisplayCaptureSourceImage;
  toDataURL(): string;
}

export interface DisplayCaptureSource {
  id: string;
  name: string;
  display_id?: string;
  thumbnail?: DisplayCaptureSourceImage | null;
  appIcon?: DisplayCaptureSourceImage | null;
}

export interface DisplayCaptureSourceOptions {
  types: Array<"screen" | "window">;
  thumbnailSize: { width: number; height: number };
  fetchWindowIcons: boolean;
}

export interface DisplayCaptureSourceChoice {
  readonly index: number;
  readonly source: DisplayCaptureSource;
  readonly kind: "screen" | "window";
  readonly label: string;
  readonly description: string;
  readonly thumbnailDataUrl: string | null;
  readonly appIconDataUrl: string | null;
  readonly capturesHamletWindow: boolean;
}

export interface DisplayCapturePickerAction {
  action: "select" | "cancel";
  index?: number;
}

export type GetDisplayCaptureSources = (
  options: DisplayCaptureSourceOptions,
) => Promise<readonly DisplayCaptureSource[]>;

export type DisplayCapturePicker = (
  choices: readonly DisplayCaptureSourceChoice[],
  request: DisplayMediaRequest,
) => Promise<DisplayCaptureSourceChoice | null>;

export interface TrustedDisplayMediaHandlerOptions {
  getSources: GetDisplayCaptureSources;
  picker: DisplayCapturePicker;
  testAutomation?: DisplayCaptureTestAutomation | null;
  requireUserGesture?: boolean;
}

export function createTrustedDisplayMediaRequestHandler(
  options: TrustedDisplayMediaHandlerOptions,
): ElectronDisplayMediaRequestHandler {
  return (request, callback) => {
    void resolveTrustedDisplayMediaStreams(request, options)
      .then((streams) => callback(streams as ElectronDisplayMediaStreams))
      .catch(() => {
        callback({});
      });
  };
}

export async function resolveTrustedDisplayMediaStreams(
  request: DisplayMediaRequest,
  options: TrustedDisplayMediaHandlerOptions,
): Promise<DisplayMediaStreams> {
  if (!shouldAllowDisplayMediaRequest(request, options)) return {};

  if (options.testAutomation === DISPLAY_CAPTURE_TEST_HAMLET_WINDOW) {
    return request.frame === null ? {} : { video: request.frame };
  }

  const sources = await options.getSources(DISPLAY_CAPTURE_SOURCE_OPTIONS);
  const choices = createDisplayCaptureSourceChoices(sources);
  if (choices.length === 0) return {};

  const selectedChoice =
    options.testAutomation === DISPLAY_CAPTURE_TEST_FIRST_SOURCE
      ? choices[0]
      : await options.picker(choices, request);
  if (selectedChoice === null || !isKnownChoice(selectedChoice, choices)) return {};

  return {
    video: {
      id: selectedChoice.source.id,
      name: displayCaptureStreamName(selectedChoice),
    },
  };
}

export function shouldAllowDisplayMediaRequest(
  request: DisplayMediaRequest,
  options: Pick<TrustedDisplayMediaHandlerOptions, "requireUserGesture"> = {},
): boolean {
  return (
    request.frame !== null &&
    request.videoRequested &&
    !request.audioRequested &&
    (options.requireUserGesture === false || request.userGesture) &&
    isTrustedRendererUrl(request.securityOrigin)
  );
}

export function createDisplayCaptureSourceChoices(
  sources: readonly DisplayCaptureSource[],
): DisplayCaptureSourceChoice[] {
  return sources.map((source, index) => {
    const kind = displayCaptureSourceKind(source);
    const capturesHamletWindow = isHamletWindowSource(source);
    const label = displayCaptureSourceLabel(source, kind, index, capturesHamletWindow);

    return {
      index,
      source,
      kind,
      label,
      description: displayCaptureSourceDescription(source, kind, capturesHamletWindow),
      thumbnailDataUrl: safeImageDataUrl(source.thumbnail, { width: 320, height: 180 }),
      appIconDataUrl: safeImageDataUrl(source.appIcon, { width: 32, height: 32 }),
      capturesHamletWindow,
    };
  });
}

export function createDisplayCapturePickerHtml(
  choices: readonly DisplayCaptureSourceChoice[],
): string {
  const choiceCards = choices.map(displayCaptureChoiceCardHtml).join("\n");
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'unsafe-inline'; script-src 'unsafe-inline'; form-action 'none'; base-uri 'none'" />
  <title>Share screen</title>
  <style>
    :root { color-scheme: dark; font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
    * { box-sizing: border-box; }
    body { margin: 0; min-height: 100vh; background: #111827; color: #f9fafb; }
    main { padding: 24px; display: flex; min-height: 100vh; flex-direction: column; gap: 18px; }
    h1 { margin: 0; font-size: 20px; line-height: 1.3; }
    p { margin: 0; color: #cbd5e1; font-size: 13px; line-height: 1.5; }
    .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(196px, 1fr)); gap: 14px; overflow: auto; padding: 2px; }
    .choice { border: 0; border-radius: 14px; background: #1f2937; color: inherit; cursor: pointer; display: flex; flex-direction: column; gap: 10px; min-height: 228px; padding: 10px; text-align: left; }
    .choice:hover, .choice:focus-visible { outline: 2px solid #60a5fa; outline-offset: 2px; background: #273449; }
    .thumbnail { align-items: center; background: #0f172a; border: 1px solid #374151; border-radius: 10px; color: #94a3b8; display: flex; font-size: 12px; justify-content: center; min-height: 110px; overflow: hidden; }
    .thumbnail img { display: block; height: 100%; max-height: 128px; object-fit: cover; width: 100%; }
    .meta { display: grid; gap: 4px; }
    .title-row { align-items: center; display: flex; gap: 8px; min-width: 0; }
    .title-row img { flex: none; height: 18px; width: 18px; }
    .title { font-size: 14px; font-weight: 700; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .description { color: #cbd5e1; font-size: 12px; line-height: 1.4; }
    .warning { color: #fde68a; }
    footer { display: flex; justify-content: flex-end; }
    .cancel { border: 1px solid #4b5563; border-radius: 10px; background: transparent; color: #f9fafb; cursor: pointer; font: inherit; padding: 8px 14px; }
    .cancel:hover, .cancel:focus-visible { outline: 2px solid #94a3b8; outline-offset: 2px; background: #1f2937; }
  </style>
</head>
<body>
  <main>
    <header>
      <h1>Choose what to share</h1>
      <p>Hamlet will receive one selected screen or window. Choosing the Hamlet window can create a mirror effect.</p>
    </header>
    <section class="grid" aria-label="Available screens and windows">
      ${choiceCards}
    </section>
    <footer>
      <button class="cancel" type="button" data-action="cancel">Cancel</button>
    </footer>
  </main>
  <script>
    (() => {
      const navigate = (action, index) => {
        const suffix = action === "select" ? "?index=" + encodeURIComponent(String(index)) : "";
        window.location.href = "${DISPLAY_CAPTURE_PICKER_SCHEME}://" + action + suffix;
      };
      document.addEventListener("click", (event) => {
        const button = event.target.closest("button[data-action]");
        if (!button) return;
        const action = button.dataset.action;
        if (action === "cancel") navigate("cancel");
        if (action === "select") navigate("select", button.dataset.index);
      });
      document.addEventListener("keydown", (event) => {
        if (event.key === "Escape") navigate("cancel");
      });
    })();
  </script>
</body>
</html>`;
}

export function parseDisplayCapturePickerAction(input: string): DisplayCapturePickerAction | null {
  let url: URL;
  try {
    url = new URL(input);
  } catch {
    return null;
  }

  if (url.protocol !== `${DISPLAY_CAPTURE_PICKER_SCHEME}:`) return null;
  const action = url.hostname;
  if (action === "cancel") return { action: "cancel" };
  if (action !== "select") return null;

  const rawIndex = url.searchParams.get("index");
  if (rawIndex === null || !/^\d+$/.test(rawIndex)) return null;
  const index = Number(rawIndex);
  if (!Number.isSafeInteger(index)) return null;
  return { action: "select", index };
}

export function resolveDisplayCaptureTestAutomation(
  env: Record<string, string | undefined> = process.env,
): DisplayCaptureTestAutomation | null {
  if (!isTruthyTestFlag(env[DISPLAY_CAPTURE_UNDER_TEST_ENV])) return null;

  const value = env[DISPLAY_CAPTURE_TEST_AUTOMATION_ENV]?.trim().toLowerCase();
  if (value === undefined || value === "") return null;
  if (value === "1" || value === "true" || value === "frame") {
    return DISPLAY_CAPTURE_TEST_HAMLET_WINDOW;
  }
  if (value === DISPLAY_CAPTURE_TEST_HAMLET_WINDOW) return DISPLAY_CAPTURE_TEST_HAMLET_WINDOW;
  if (value === DISPLAY_CAPTURE_TEST_FIRST_SOURCE) return DISPLAY_CAPTURE_TEST_FIRST_SOURCE;
  return null;
}

function isTruthyTestFlag(value: string | undefined): boolean {
  const normalized = value?.trim().toLowerCase();
  return normalized === "1" || normalized === "true";
}

function displayCaptureChoiceCardHtml(choice: DisplayCaptureSourceChoice): string {
  const thumbnail = choice.thumbnailDataUrl
    ? `<img src="${choice.thumbnailDataUrl}" alt="" />`
    : `<span>${choice.kind === "screen" ? "Screen" : "Window"}</span>`;
  const icon = choice.appIconDataUrl ? `<img src="${choice.appIconDataUrl}" alt="" />` : "";
  const warningClass = choice.capturesHamletWindow ? " warning" : "";
  const ariaLabel = choice.capturesHamletWindow
    ? `Share ${choice.label}. This may create a mirror effect.`
    : `Share ${choice.label}`;

  return `<button class="choice" type="button" data-action="select" data-index="${choice.index}" aria-label="${escapeHtml(ariaLabel)}">
  <span class="thumbnail">${thumbnail}</span>
  <span class="meta">
    <span class="title-row">${icon}<span class="title">${escapeHtml(choice.label)}</span></span>
    <span class="description${warningClass}">${escapeHtml(choice.description)}</span>
  </span>
</button>`;
}

function displayCaptureSourceKind(source: DisplayCaptureSource): "screen" | "window" {
  if (source.id.startsWith(SCREEN_SOURCE_PREFIX)) return "screen";
  return "window";
}

function displayCaptureSourceLabel(
  source: DisplayCaptureSource,
  kind: "screen" | "window",
  index: number,
  capturesHamletWindow: boolean,
): string {
  const trimmedName = source.name.trim();
  const fallback = kind === "screen" ? `Screen ${index + 1}` : `Window ${index + 1}`;
  const baseLabel = trimmedName === "" ? fallback : trimmedName;
  if (!capturesHamletWindow) return baseLabel;
  return baseLabel.toLowerCase().includes("hamlet") ? baseLabel : `${baseLabel} (Hamlet window)`;
}

function displayCaptureSourceDescription(
  source: DisplayCaptureSource,
  kind: "screen" | "window",
  capturesHamletWindow: boolean,
): string {
  if (capturesHamletWindow) return "Hamlet window — sharing it may create a mirror effect.";
  if (kind === "screen") {
    const displayId = source.display_id?.trim();
    return displayId ? `Screen display ${displayId}` : "Entire screen";
  }
  return "Application window";
}

function displayCaptureStreamName(choice: DisplayCaptureSourceChoice): string {
  const sourceName = choice.source.name.trim();
  return sourceName === "" ? choice.label : sourceName;
}

function isHamletWindowSource(source: DisplayCaptureSource): boolean {
  return (
    source.id.startsWith(WINDOW_SOURCE_PREFIX) && source.id.endsWith(CURRENT_PROCESS_WINDOW_SUFFIX)
  );
}

function safeImageDataUrl(
  image: DisplayCaptureSourceImage | null | undefined,
  size: { width: number; height: number },
): string | null {
  if (image == null) return null;

  try {
    if (image.isEmpty()) return null;
    const dataUrl = image.resize(size).toDataURL();
    return SAFE_IMAGE_DATA_URL_PATTERN.test(dataUrl) ? dataUrl : null;
  } catch {
    return null;
  }
}

function isKnownChoice(
  selectedChoice: DisplayCaptureSourceChoice,
  choices: readonly DisplayCaptureSourceChoice[],
): boolean {
  return choices.some(
    (choice) =>
      choice.index === selectedChoice.index && choice.source.id === selectedChoice.source.id,
  );
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}
