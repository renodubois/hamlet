import { describe, expect, it, vi } from "vitest";
import {
  DISPLAY_CAPTURE_SOURCE_OPTIONS,
  DISPLAY_CAPTURE_TEST_AUTOMATION_ENV,
  DISPLAY_CAPTURE_TEST_FIRST_SOURCE,
  DISPLAY_CAPTURE_TEST_HAMLET_WINDOW,
  DISPLAY_CAPTURE_UNDER_TEST_ENV,
  createDisplayCapturePickerHtml,
  createDisplayCaptureSourceChoices,
  createTrustedDisplayMediaRequestHandler,
  parseDisplayCapturePickerAction,
  resolveDisplayCaptureTestAutomation,
  resolveTrustedDisplayMediaStreams,
  shouldAllowDisplayMediaRequest,
  type DisplayCaptureSource,
  type DisplayCaptureSourceChoice,
  type DisplayCaptureSourceImage,
  type GetDisplayCaptureSources,
} from "./display-capture";
import { resolveConfiguredRendererOrigin } from "./constants";

const TRUSTED_RENDERER_ORIGIN = resolveConfiguredRendererOrigin();
const UNTRUSTED_ORIGIN = "https://evil.example";

const trustedRequest = {
  frame: { routingId: 1 },
  securityOrigin: TRUSTED_RENDERER_ORIGIN,
  videoRequested: true,
  audioRequested: false,
  userGesture: true,
};

class FakeImage implements DisplayCaptureSourceImage {
  readonly resizeCalls: Array<{ width?: number; height?: number }> = [];

  constructor(
    private readonly dataUrl: string,
    private readonly empty = false,
  ) {}

  isEmpty(): boolean {
    return this.empty;
  }

  resize(options: { width?: number; height?: number }): DisplayCaptureSourceImage {
    this.resizeCalls.push(options);
    return this;
  }

  toDataURL(): string {
    return this.dataUrl;
  }
}

describe("Electron display capture policy", () => {
  it("allows trusted display-media requests with video only and a user gesture", () => {
    expect(shouldAllowDisplayMediaRequest(trustedRequest)).toBe(true);
    expect(shouldAllowDisplayMediaRequest({ ...trustedRequest, audioRequested: true })).toBe(false);
    expect(shouldAllowDisplayMediaRequest({ ...trustedRequest, videoRequested: false })).toBe(
      false,
    );
    expect(shouldAllowDisplayMediaRequest({ ...trustedRequest, userGesture: false })).toBe(false);
    expect(shouldAllowDisplayMediaRequest({ ...trustedRequest, frame: null })).toBe(false);
    expect(
      shouldAllowDisplayMediaRequest({ ...trustedRequest, securityOrigin: UNTRUSTED_ORIGIN }),
    ).toBe(false);
  });

  it("returns exactly the one fallback source selected by the picker", async () => {
    const sources = displayCaptureSources();
    const getSources = vi.fn<GetDisplayCaptureSources>(async () => sources);
    const picker = vi.fn(async (choices: readonly DisplayCaptureSourceChoice[]) => choices[1]);

    await expect(
      resolveTrustedDisplayMediaStreams(trustedRequest, { getSources, picker }),
    ).resolves.toEqual({ video: { id: "window:44:0", name: "Project notes" } });

    expect(getSources).toHaveBeenCalledWith(DISPLAY_CAPTURE_SOURCE_OPTIONS);
    expect(picker).toHaveBeenCalledTimes(1);
  });

  it("returns no stream for untrusted origins before listing fallback sources", async () => {
    const getSources = vi.fn<GetDisplayCaptureSources>(async () => displayCaptureSources());
    const picker = vi.fn(async (choices: readonly DisplayCaptureSourceChoice[]) => choices[0]);

    await expect(
      resolveTrustedDisplayMediaStreams(
        { ...trustedRequest, securityOrigin: UNTRUSTED_ORIGIN },
        { getSources, picker },
      ),
    ).resolves.toEqual({});

    expect(getSources).not.toHaveBeenCalled();
    expect(picker).not.toHaveBeenCalled();
  });

  it("treats fallback picker cancellation as no selected stream", async () => {
    await expect(
      resolveTrustedDisplayMediaStreams(trustedRequest, {
        getSources: async () => displayCaptureSources(),
        picker: async () => null,
      }),
    ).resolves.toEqual({});
  });

  it("does not accept a picker result that was not one of the presented choices", async () => {
    await expect(
      resolveTrustedDisplayMediaStreams(trustedRequest, {
        getSources: async () => displayCaptureSources(),
        picker: async (choices) => ({
          ...choices[0],
          index: 99,
          source: { ...choices[0].source, id: "screen:other:0" },
        }),
      }),
    ).resolves.toEqual({});
  });

  it("supports explicit test-only automation without invoking the fallback picker", async () => {
    const getSources = vi.fn<GetDisplayCaptureSources>(async () => displayCaptureSources());
    const picker = vi.fn(async (choices: readonly DisplayCaptureSourceChoice[]) => choices[0]);

    await expect(
      resolveTrustedDisplayMediaStreams(trustedRequest, {
        getSources,
        picker,
        testAutomation: DISPLAY_CAPTURE_TEST_HAMLET_WINDOW,
      }),
    ).resolves.toEqual({ video: trustedRequest.frame });

    expect(getSources).not.toHaveBeenCalled();
    expect(picker).not.toHaveBeenCalled();
  });

  it("supports first-source automation only when explicitly enabled", async () => {
    const getSources = vi.fn<GetDisplayCaptureSources>(async () => displayCaptureSources());
    const picker = vi.fn(async (choices: readonly DisplayCaptureSourceChoice[]) => choices[1]);

    await expect(
      resolveTrustedDisplayMediaStreams(trustedRequest, {
        getSources,
        picker,
        testAutomation: DISPLAY_CAPTURE_TEST_FIRST_SOURCE,
      }),
    ).resolves.toEqual({ video: { id: "screen:1:0", name: "Entire Screen" } });

    expect(getSources).toHaveBeenCalledTimes(1);
    expect(picker).not.toHaveBeenCalled();
  });

  it("maps test automation environment values deliberately", () => {
    expect(resolveDisplayCaptureTestAutomation({})).toBeNull();
    expect(
      resolveDisplayCaptureTestAutomation({
        [DISPLAY_CAPTURE_TEST_AUTOMATION_ENV]: "1",
      }),
    ).toBeNull();
    expect(
      resolveDisplayCaptureTestAutomation({
        [DISPLAY_CAPTURE_UNDER_TEST_ENV]: "1",
        [DISPLAY_CAPTURE_TEST_AUTOMATION_ENV]: "",
      }),
    ).toBeNull();
    expect(
      resolveDisplayCaptureTestAutomation({
        [DISPLAY_CAPTURE_UNDER_TEST_ENV]: "1",
        [DISPLAY_CAPTURE_TEST_AUTOMATION_ENV]: "1",
      }),
    ).toBe(DISPLAY_CAPTURE_TEST_HAMLET_WINDOW);
    expect(
      resolveDisplayCaptureTestAutomation({
        [DISPLAY_CAPTURE_UNDER_TEST_ENV]: "true",
        [DISPLAY_CAPTURE_TEST_AUTOMATION_ENV]: DISPLAY_CAPTURE_TEST_FIRST_SOURCE,
      }),
    ).toBe(DISPLAY_CAPTURE_TEST_FIRST_SOURCE);
    expect(
      resolveDisplayCaptureTestAutomation({
        [DISPLAY_CAPTURE_UNDER_TEST_ENV]: "1",
        [DISPLAY_CAPTURE_TEST_AUTOMATION_ENV]: "auto",
      }),
    ).toBeNull();
  });

  it("invokes the Electron callback with no stream when source resolution fails", async () => {
    const handler = createTrustedDisplayMediaRequestHandler({
      getSources: async () => {
        throw new Error("desktop capture unavailable");
      },
      picker: async () => null,
    });
    const callback = vi.fn();

    handler(trustedRequest as never, callback);
    await vi.waitFor(() => {
      expect(callback).toHaveBeenCalledWith({});
    });
  });
});

describe("Electron fallback display-capture picker model", () => {
  it("creates recognizable labels, safe thumbnails/icons, and Hamlet-window warnings", () => {
    const screenThumbnail = new FakeImage("data:image/png;base64,aaaa");
    const windowIcon = new FakeImage("data:image/png;base64,bbbb");
    const unsafeIcon = new FakeImage("data:text/html;base64,PHNjcmlwdD4=");

    const choices = createDisplayCaptureSourceChoices([
      {
        id: "screen:1:0",
        name: "Entire Screen",
        display_id: "display-1",
        thumbnail: screenThumbnail,
      },
      {
        id: "window:42:1",
        name: "Hamlet Electron Alpha",
        appIcon: windowIcon,
      },
      {
        id: "window:77:0",
        name: "   ",
        appIcon: unsafeIcon,
      },
    ]);

    expect(choices[0]).toMatchObject({
      kind: "screen",
      label: "Entire Screen",
      description: "Screen display display-1",
      thumbnailDataUrl: "data:image/png;base64,aaaa",
      capturesHamletWindow: false,
    });
    expect(screenThumbnail.resizeCalls).toContainEqual({ width: 320, height: 180 });
    expect(choices[1]).toMatchObject({
      kind: "window",
      label: "Hamlet Electron Alpha",
      description: "Hamlet window — sharing it may create a mirror effect.",
      appIconDataUrl: "data:image/png;base64,bbbb",
      capturesHamletWindow: true,
    });
    expect(windowIcon.resizeCalls).toContainEqual({ width: 32, height: 32 });
    expect(choices[2]).toMatchObject({
      kind: "window",
      label: "Window 3",
      appIconDataUrl: null,
    });
  });

  it("renders sanitized picker HTML without exposing desktop source ids or IPC hooks", () => {
    const choices = createDisplayCaptureSourceChoices([
      {
        id: "screen:1:0",
        name: "Main <Screen>",
        thumbnail: new FakeImage("data:image/png;base64,cccc"),
      },
    ]);
    const html = createDisplayCapturePickerHtml(choices);

    expect(html).toContain("Main &lt;Screen&gt;");
    expect(html).toContain("data:image/png;base64,cccc");
    expect(html).not.toContain("screen:1:0");
    expect(html).not.toMatch(/ipcRenderer|desktopCapturer|require\(|nodeIntegration/);
  });

  it("parses only picker select and cancel URLs", () => {
    expect(parseDisplayCapturePickerAction("hamlet-display-capture://select?index=2")).toEqual({
      action: "select",
      index: 2,
    });
    expect(parseDisplayCapturePickerAction("hamlet-display-capture://cancel")).toEqual({
      action: "cancel",
    });
    expect(parseDisplayCapturePickerAction("https://example.test/select?index=2")).toBeNull();
    expect(parseDisplayCapturePickerAction("hamlet-display-capture://select?index=-1")).toBeNull();
    expect(parseDisplayCapturePickerAction("hamlet-display-capture://select?index=abc")).toBeNull();
  });
});

function displayCaptureSources(): DisplayCaptureSource[] {
  return [
    { id: "screen:1:0", name: "Entire Screen", display_id: "display-1" },
    { id: "window:44:0", name: "Project notes" },
  ];
}
