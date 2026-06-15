import { expect, test, type Browser, type Page } from "@playwright/test";
import { livekitUrl, rendererUrl, serverUrl } from "./test-config";

async function logIn(page: Page, username = "baipas") {
  await page.goto(rendererUrl);
  await page.getByPlaceholder("Server URL").fill(serverUrl);
  await page.getByPlaceholder("Username").fill(username);
  await page.getByPlaceholder("Password").fill("password");
  await page.getByRole("button", { name: /^sign in$/i }).click();

  await expect(page.getByRole("navigation", { name: /channels/i })).toBeVisible({
    timeout: 30_000,
  });
}

async function logInAsDevUser(page: Page) {
  await logIn(page, "baipas");
}

function captureVoiceDiagnostics(page: Page, diagnostics: string[], label = "page") {
  page.on("console", (message) => {
    diagnostics.push(`${label} console.${message.type()}: ${message.text()}`);
  });
  page.on("pageerror", (error) => {
    diagnostics.push(`${label} pageerror: ${error.message}`);
  });
  page.on("requestfailed", (request) => {
    diagnostics.push(
      `${label} requestfailed: ${request.method()} ${request.url()} ${request.failure()?.errorText}`,
    );
  });
  page.on("response", (response) => {
    if (response.url().includes("/voice/") || response.url().startsWith(livekitUrl)) {
      diagnostics.push(`${label} response: ${response.status()} ${response.url()}`);
    }
  });
}

type FakeCameraPrerequisiteResult =
  | { ok: true; videoTracks: number; stoppedVideoTracks: number }
  | { ok: false; reason: string };

type CameraTrackSnapshot = {
  videoTracks: number;
  liveVideoTracks: number;
  labels: string[];
};

async function ensureBrowserFakeCameraOrSkip(page: Page) {
  const result = await page.evaluate(async (): Promise<FakeCameraPrerequisiteResult> => {
    if (!navigator.mediaDevices?.getUserMedia) {
      return { ok: false, reason: "navigator.mediaDevices.getUserMedia is unavailable" };
    }

    let stream: MediaStream | undefined;
    try {
      stream = await navigator.mediaDevices.getUserMedia({ video: true, audio: false });
      const videoTracks = stream.getVideoTracks();
      videoTracks.forEach((track) => track.stop());
      const stoppedVideoTracks = videoTracks.filter((track) => track.readyState === "ended").length;
      if (videoTracks.length === 0) {
        return { ok: false, reason: "getUserMedia returned no video tracks" };
      }
      if (stoppedVideoTracks !== videoTracks.length) {
        return {
          ok: false,
          reason: `getUserMedia fake tracks did not stop cleanly (${stoppedVideoTracks}/${videoTracks.length})`,
        };
      }
      return { ok: true, videoTracks: videoTracks.length, stoppedVideoTracks };
    } catch (error: unknown) {
      stream?.getTracks().forEach((track) => track.stop());
      const reason =
        error instanceof DOMException || error instanceof Error
          ? `${error.name}: ${error.message}`
          : String(error);
      return { ok: false, reason };
    }
  });

  if (!result.ok) {
    test.skip(true, `Browser fake camera media is unavailable: ${result.reason}`);
  }
}

async function voiceFailureDetails(page: Page, diagnostics: string[]) {
  const alerts = await page
    .getByRole("alert")
    .allTextContents()
    .catch((error: unknown) => [`<could not read alerts: ${String(error)}>`]);
  const bodyText = await page
    .locator("body")
    .innerText({ timeout: 1_000 })
    .catch((error: unknown) => `<could not read body text: ${String(error)}>`);
  return [
    `URL: ${page.url()}`,
    `Alerts: ${alerts.length ? alerts.join(" | ") : "<none>"}`,
    "Recent browser diagnostics:",
    diagnostics.slice(-40).join("\n") || "<none>",
    "Visible body text:",
    bodyText,
  ].join("\n");
}

async function joinSeededVoiceChannel(page: Page, diagnostics: string[], username = "baipas") {
  const voiceButton = page.getByRole("button", { name: /^Join voice channel voice$/i });
  await expect(voiceButton).toBeVisible();

  const tokenResponsePromise = page.waitForResponse(
    (response) =>
      response.url().includes("/voice/token/") && response.request().method() === "POST",
  );
  await voiceButton.click();

  const tokenResponse = await tokenResponsePromise;
  if (tokenResponse.status() === 503) {
    test.skip(true, "LiveKit is not configured for this E2E environment.");
  }
  expect(tokenResponse.ok(), `voice token response status ${tokenResponse.status()}`).toBe(true);
  const tokenPayload = (await tokenResponse.json()) as { url?: string; room?: string };
  const tokenPathParts = new URL(tokenResponse.url()).pathname.split("/");
  const channelId = Number(tokenPathParts[tokenPathParts.length - 1]);
  expect(Number.isSafeInteger(channelId), `voice token URL should include channel id`).toBe(true);
  expect(tokenPayload.room).toMatch(/^channel-\d+$/);
  expect(
    tokenPayload.url,
    "LiveKit signaling URL must use the browser-repro 127.0.0.1 loopback host",
  ).toBe(livekitUrl);

  try {
    await expect(page.getByRole("group", { name: /^Voice controls$/i })).toBeVisible({
      timeout: 45_000,
    });
    await expect(page.getByRole("button", { name: /^Mute microphone$/i })).toBeVisible();
    await expect(
      page.getByRole("list", { name: /^Participants in voice$/i }).getByText(username),
    ).toBeVisible({ timeout: 45_000 });
    await expect(page.getByRole("alert")).toHaveCount(0);
  } catch (error) {
    const details = await voiceFailureDetails(page, diagnostics);
    if (/\bNot supported\b/i.test(details)) {
      test.skip(true, `LiveKit/browser media is unsupported in this E2E environment.\n${details}`);
    }
    throw new Error(
      `Voice join did not complete in browser.\n${details}\nOriginal error: ${String(error)}`,
    );
  }

  return { channelId };
}

async function waitForScreenShareStart(
  page: Page,
): Promise<{ ok: true } | { ok: false; message: string }> {
  const stopButton = page.getByRole("button", { name: /^Stop sharing screen$/i });
  const deadline = Date.now() + 45_000;
  let lastAlert = "";

  while (Date.now() < deadline) {
    if (await stopButton.isVisible().catch(() => false)) return { ok: true };

    const alerts = await page
      .getByRole("alert")
      .allTextContents()
      .catch(() => [] as string[]);
    lastAlert = alerts.join(" | ");
    if (/screen share|display|capture|permission|not supported|denied|canceled/i.test(lastAlert)) {
      return { ok: false, message: lastAlert };
    }
    await page.waitForTimeout(500);
  }

  return { ok: false, message: lastAlert || "timed out waiting for sharing state" };
}

async function startScreenShareOrSkip(page: Page, diagnostics: string[]) {
  await page.getByRole("button", { name: /^Share screen$/i }).click();
  const result = await waitForScreenShareStart(page);
  if (!result.ok) {
    const details = await voiceFailureDetails(page, diagnostics);
    test.skip(
      true,
      `Chromium desktop-capture automation could not start screen share: ${result.message}\n${details}`,
    );
  }

  await expect(page.getByRole("button", { name: /^Stop sharing screen$/i })).toBeVisible();
  await expect(page.getByText(/^Sharing screen$/)).toBeVisible();
}

async function stopScreenShare(page: Page) {
  await page.getByRole("button", { name: /^Stop sharing screen$/i }).click();
  await expect(page.getByRole("button", { name: /^Share screen$/i })).toBeVisible({
    timeout: 30_000,
  });
  await expect(page.getByText(/^Sharing screen$/)).toHaveCount(0);
}

async function waitForCameraStart(
  page: Page,
): Promise<{ ok: true } | { ok: false; message: string }> {
  const stopButton = page.getByRole("button", { name: /^Turn off camera$/i });
  const deadline = Date.now() + 45_000;
  let lastAlert = "";

  while (Date.now() < deadline) {
    if (await stopButton.isVisible().catch(() => false)) return { ok: true };

    const alerts = await page
      .getByRole("alert")
      .allTextContents()
      .catch(() => [] as string[]);
    lastAlert = alerts.join(" | ");
    if (
      /camera|permission|not supported|denied|not found|device|overconstrained/i.test(lastAlert)
    ) {
      return { ok: false, message: lastAlert };
    }
    await page.waitForTimeout(500);
  }

  return { ok: false, message: lastAlert || "timed out waiting for camera state" };
}

async function rememberLocalCameraTrack(page: Page) {
  const snapshot = await page.evaluate((): CameraTrackSnapshot => {
    const video = document.querySelector(
      'video[aria-label="Your camera video"]',
    ) as HTMLVideoElement | null;
    const stream = video?.srcObject instanceof MediaStream ? video.srcObject : null;
    const tracks = stream?.getVideoTracks() ?? [];
    const testWindow = window as typeof window & {
      __hamletCameraSmokeTrack?: MediaStreamTrack;
    };
    if (tracks[0]) testWindow.__hamletCameraSmokeTrack = tracks[0];
    return {
      videoTracks: tracks.length,
      liveVideoTracks: tracks.filter((track) => track.readyState === "live").length,
      labels: tracks.map((track) => track.label),
    };
  });

  expect(snapshot.videoTracks, `local camera tracks: ${snapshot.labels.join(", ")}`).toBe(1);
  expect(snapshot.liveVideoTracks).toBe(1);
}

async function rememberedLocalCameraTrackState(page: Page) {
  return page.evaluate(() => {
    const testWindow = window as typeof window & {
      __hamletCameraSmokeTrack?: MediaStreamTrack;
    };
    return testWindow.__hamletCameraSmokeTrack?.readyState ?? "missing";
  });
}

async function startCameraOrSkip(page: Page, diagnostics: string[]) {
  await page.getByRole("button", { name: /^Turn on camera$/i }).click();
  const result = await waitForCameraStart(page);
  if (!result.ok) {
    const details = await voiceFailureDetails(page, diagnostics);
    if (/permission|denied|not found|no camera|getUserMedia|not supported|device/i.test(details)) {
      test.skip(
        true,
        `Browser fake camera media could not start through LiveKit: ${result.message}\n${details}`,
      );
    }
    throw new Error(`Camera did not start in browser.\n${details}`);
  }

  await expect(page.getByRole("region", { name: /local camera preview/i })).toBeVisible({
    timeout: 30_000,
  });
  await rememberLocalCameraTrack(page);
  await expect(page.getByText(/^Camera on$/).first()).toBeVisible();
}

async function stopCamera(page: Page) {
  await page.getByRole("button", { name: /^Turn off camera$/i }).click();
  await expect(page.getByRole("button", { name: /^Turn on camera$/i })).toBeVisible({
    timeout: 30_000,
  });
  await expect(page.getByRole("region", { name: /local camera preview/i })).toHaveCount(0);
  await expect.poll(() => rememberedLocalCameraTrackState(page), { timeout: 30_000 }).toBe("ended");
}

async function installControlledSseTap(page: Page) {
  await page.context().addInitScript(() => {
    const NativeEventSource = window.EventSource;
    const sources = new Set<EventSource>();

    class ControlledEventSource extends NativeEventSource {
      constructor(url: string | URL, eventSourceInitDict?: EventSourceInit) {
        super(url, eventSourceInitDict);
        sources.add(this);
      }

      override close() {
        sources.delete(this);
        super.close();
      }
    }

    const testWindow = window as typeof window & {
      __hamletPushSse?: (event: unknown) => void;
    };
    testWindow.__hamletPushSse = (event: unknown) => {
      const message = new MessageEvent("message", { data: JSON.stringify(event) });
      sources.forEach((source) => source.onmessage?.(message));
    };
    window.EventSource = ControlledEventSource;
  });
}

async function pushControlledSse(page: Page, event: unknown) {
  await page.evaluate((payload) => {
    const testWindow = window as typeof window & {
      __hamletPushSse?: (event: unknown) => void;
    };
    if (!testWindow.__hamletPushSse) throw new Error("controlled SSE tap is not installed");
    testWindow.__hamletPushSse(payload);
  }, event);
}

async function currentUserId(page: Page): Promise<number> {
  return page.evaluate(async (url) => {
    const response = await fetch(`${url}/me`, { credentials: "include" });
    const user = (await response.json()) as { id: number };
    return user.id;
  }, serverUrl);
}

async function newVoicePage(browser: Browser, username: string, diagnostics: string[]) {
  const context = await browser.newContext({ permissions: ["microphone", "camera"] });
  await context.grantPermissions(["microphone", "camera"], {
    origin: new URL(rendererUrl).origin,
  });
  const page = await context.newPage();
  await installControlledSseTap(page);
  captureVoiceDiagnostics(page, diagnostics, username);
  await logIn(page, username);
  return { context, page };
}

test("browser dev user can join the seeded voice channel via LiveKit", async ({ page }) => {
  test.setTimeout(120_000);

  const diagnostics: string[] = [];
  captureVoiceDiagnostics(page, diagnostics);

  await logInAsDevUser(page);
  await joinSeededVoiceChannel(page, diagnostics);
});

test("browser can start and stop camera with fake media after joining voice", async ({ page }) => {
  test.setTimeout(150_000);

  const diagnostics: string[] = [];
  captureVoiceDiagnostics(page, diagnostics);

  await logInAsDevUser(page);
  await ensureBrowserFakeCameraOrSkip(page);
  await joinSeededVoiceChannel(page, diagnostics);
  await startCameraOrSkip(page, diagnostics);
  await stopCamera(page);
});

test("Chromium can start and stop screen share after joining voice", async ({
  browserName,
  page,
}) => {
  test.skip(
    browserName !== "chromium",
    "Chromium desktop-capture automation is the supported browser screen-share smoke path.",
  );
  test.setTimeout(150_000);

  const diagnostics: string[] = [];
  captureVoiceDiagnostics(page, diagnostics);

  await logInAsDevUser(page);
  await joinSeededVoiceChannel(page, diagnostics);
  await startScreenShareOrSkip(page, diagnostics);
  await stopScreenShare(page);
});

test("Chromium clients discover, watch, stop watching, and receive controlled screen-share updates", async ({
  browser,
  browserName,
  page,
}) => {
  test.skip(
    browserName !== "chromium",
    "Chromium desktop-capture automation is the supported browser screen-share smoke path.",
  );
  test.setTimeout(180_000);

  const diagnostics: string[] = [];
  captureVoiceDiagnostics(page, diagnostics, "baipas");

  const viewer = await newVoicePage(browser, "teo", diagnostics);
  try {
    await logInAsDevUser(page);
    const sharerJoin = await joinSeededVoiceChannel(page, diagnostics, "baipas");
    await joinSeededVoiceChannel(viewer.page, diagnostics, "teo");

    await expect(
      viewer.page.getByRole("list", { name: /^Participants in voice$/i }).getByText("baipas"),
    ).toBeVisible({ timeout: 45_000 });

    await startScreenShareOrSkip(page, diagnostics);

    const sharerUserId = await currentUserId(page);
    const stream = {
      channel_id: sharerJoin.channelId,
      sharer_user_id: sharerUserId,
      username: "baipas",
      display_name: "Baipas E2E",
      avatar_url: null,
      participant_identity: String(sharerUserId),
      track_sid: `TR_e2e_${Date.now()}`,
      track_name: "screen",
      source: "screen_share" as const,
      started_at: Math.floor(Date.now() / 1000),
    };
    await pushControlledSse(viewer.page, { kind: "screen_share_started", data: stream });

    const viewerShelf = viewer.page.getByRole("region", {
      name: /active screen shares in voice/i,
    });
    await expect(viewerShelf.getByText("Baipas E2E's screen")).toBeVisible({ timeout: 45_000 });

    await viewerShelf.getByRole("button", { name: /watch Baipas E2E's screen share/i }).click();
    await expect(
      viewer.page.getByRole("region", { name: /screen share viewer for Baipas E2E/i }),
    ).toBeVisible({ timeout: 30_000 });

    await viewer.page
      .getByRole("button", { name: /stop watching Baipas E2E's screen share/i })
      .click();
    await expect(
      viewer.page.getByRole("region", { name: /screen share viewer for Baipas E2E/i }),
    ).toHaveCount(0);

    await stopScreenShare(page);
    await pushControlledSse(viewer.page, {
      kind: "screen_share_stopped",
      data: {
        channel_id: stream.channel_id,
        sharer_user_id: stream.sharer_user_id,
        participant_identity: stream.participant_identity,
        track_sid: stream.track_sid,
      },
    });
    await expect(viewerShelf.getByText("Baipas E2E's screen")).toHaveCount(0, {
      timeout: 45_000,
    });
  } finally {
    await viewer.context.close();
  }
});
