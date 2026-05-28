import { expect, test, type Page } from "@playwright/test";

const SERVER_URL = "http://127.0.0.1:3030";

async function logInAsDevUser(page: Page) {
  await page.goto("/");
  await page.getByPlaceholder("Server URL").fill(SERVER_URL);
  await page.getByPlaceholder("Username").fill("baipas");
  await page.getByPlaceholder("Password").fill("password");
  await page.getByRole("button", { name: /^sign in$/i }).click();

  await expect(page.getByRole("navigation", { name: /channels/i })).toBeVisible({
    timeout: 30_000,
  });
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

test("browser dev user can join the seeded voice channel via LiveKit", async ({ page }) => {
  test.setTimeout(120_000);

  const diagnostics: string[] = [];
  page.on("console", (message) => {
    diagnostics.push(`console.${message.type()}: ${message.text()}`);
  });
  page.on("pageerror", (error) => {
    diagnostics.push(`pageerror: ${error.message}`);
  });
  page.on("requestfailed", (request) => {
    diagnostics.push(
      `requestfailed: ${request.method()} ${request.url()} ${request.failure()?.errorText}`,
    );
  });
  page.on("response", (response) => {
    if (response.url().includes("/voice/") || response.url().includes(":7880")) {
      diagnostics.push(`response: ${response.status()} ${response.url()}`);
    }
  });

  await logInAsDevUser(page);

  const voiceButton = page.getByRole("button", { name: /^Join voice channel voice$/i });
  await expect(voiceButton).toBeVisible();

  const tokenResponsePromise = page.waitForResponse(
    (response) =>
      response.url().includes("/voice/token/") && response.request().method() === "POST",
  );
  await voiceButton.click();

  const tokenResponse = await tokenResponsePromise;
  expect(tokenResponse.ok(), `voice token response status ${tokenResponse.status()}`).toBe(true);
  const tokenPayload = (await tokenResponse.json()) as { url?: string; room?: string };
  expect(tokenPayload.room).toMatch(/^channel-\d+$/);
  expect(
    tokenPayload.url,
    "LiveKit signaling URL must use the browser-repro 127.0.0.1 loopback host",
  ).toBe("ws://127.0.0.1:7880");

  try {
    await expect(page.getByRole("group", { name: /^Voice controls$/i })).toBeVisible({
      timeout: 45_000,
    });
    await expect(page.getByRole("button", { name: /^Mute microphone$/i })).toBeVisible();
    await expect(
      page.getByRole("list", { name: /^Participants in voice$/i }).getByText("baipas"),
    ).toBeVisible({ timeout: 45_000 });
    await expect(page.getByRole("alert")).toHaveCount(0);
  } catch (error) {
    throw new Error(
      `Voice join did not complete in browser.\n${await voiceFailureDetails(
        page,
        diagnostics,
      )}\nOriginal error: ${String(error)}`,
    );
  }
});
