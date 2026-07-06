import { test, expect, type Locator, type Page } from "@playwright/test";
import { fileURLToPath } from "node:url";
import { rendererUrl, serverUrl } from "./test-config";

const tinyPngPath = fileURLToPath(new URL("./fixtures/tiny.png", import.meta.url));
const serverOrigin = new URL(serverUrl).origin;
const rendererRoot = new URL("/", rendererUrl).toString();

type LocatorRoot = Page | Locator;

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function waitForMessageSse(page: Page): Promise<void> {
  return page
    .waitForResponse((response) => {
      const url = new URL(response.url());
      return url.origin === serverOrigin && url.pathname === "/messages/subscribe";
    })
    .then(() => undefined);
}

async function loginAndOpenGeneral(page: Page, username = "baipas") {
  await page.goto(rendererRoot);

  await page.getByPlaceholder("Server URL").fill(serverUrl);
  await page.getByPlaceholder("Username").fill(username);
  await page.getByPlaceholder("Password").fill("password");

  const initialSse = waitForMessageSse(page);
  await page.getByRole("button", { name: /sign in/i }).click();
  await expect(page.locator("aside").getByText(username)).toBeVisible({ timeout: 30_000 });
  await initialSse;

  const generalLink = page
    .getByRole("navigation", { name: /channels/i })
    .getByRole("link", { name: "general" });
  const generalHref = await generalLink.getAttribute("href");
  expect(generalHref).toBeTruthy();
  if (!generalHref) throw new Error("general link is missing an href");

  const resubscribed = waitForMessageSse(page);
  await page.goto(new URL(generalHref, rendererUrl).toString());
  await expect(page.getByRole("heading", { name: /^#\s*general$/i })).toBeVisible();
  await resubscribed;
}

async function attachTinyPhoto(root: LocatorRoot) {
  await root.locator('input[type="file"][aria-label="Photo files"]').setInputFiles(tinyPngPath);
  await expect(root.getByRole("img", { name: /selected photo 1: tiny\.png/i })).toBeVisible();
}

async function expectPhotoMessage(root: LocatorRoot, marker: string, author: string) {
  const row = root.locator(".group").filter({ hasText: marker }).last();
  await expect(row).toBeVisible({ timeout: 20_000 });

  const thumbnail = row.getByRole("img", {
    name: new RegExp(`photo attachment from ${escapeRegExp(author)}`, "i"),
  });
  await expect(thumbnail).toBeVisible({ timeout: 20_000 });
  await expect
    .poll(() =>
      thumbnail.evaluate(
        (image) => image instanceof HTMLImageElement && image.complete && image.naturalWidth > 0,
      ),
    )
    .toBe(true);

  const openButton = row.getByRole("button", {
    name: new RegExp(`open photo attachment from ${escapeRegExp(author)}`, "i"),
  });
  await expect(openButton).toBeVisible();
  return { row, thumbnail, openButton };
}

test("uploads a channel photo and renders the server thumbnail", async ({ page }) => {
  test.setTimeout(90_000);
  await loginAndOpenGeneral(page);

  const marker = `photo upload smoke ${Date.now()}`;
  await attachTinyPhoto(page);
  await page.getByPlaceholder(/send a new message/i).fill(marker);
  await page.getByRole("button", { name: /^send$/i }).click();

  const { openButton } = await expectPhotoMessage(page, marker, "baipas");
  await openButton.click();
  const dialog = page.getByRole("dialog", { name: /photo attachment from baipas/i });
  await expect(dialog).toBeVisible();
  const fullSizeImage = dialog.getByRole("img", { name: /photo attachment from baipas/i });
  await expect(fullSizeImage).toBeVisible();
  await expect
    .poll(() =>
      fullSizeImage.evaluate(
        (image) => image instanceof HTMLImageElement && image.complete && image.naturalWidth > 0,
      ),
    )
    .toBe(true);
});

test("uploads a thread reply photo and renders it in the open panel", async ({ page }) => {
  test.setTimeout(90_000);
  await loginAndOpenGeneral(page);

  const rootMarker = `thread photo root ${Date.now()}`;
  await page.getByPlaceholder(/send a new message/i).fill(rootMarker);
  await page.getByPlaceholder(/send a new message/i).press("Enter");
  const rootRow = page.locator(".group").filter({ hasText: rootMarker }).last();
  await expect(rootRow).toBeVisible({ timeout: 20_000 });

  await rootRow.getByRole("button", { name: /reply in thread to message by baipas/i }).click();
  const panel = page.getByRole("complementary", { name: /thread panel/i });
  await expect(panel.getByText(rootMarker)).toBeVisible();

  const replyMarker = `thread photo reply ${Date.now()}`;
  await attachTinyPhoto(panel);
  await panel.getByRole("textbox", { name: /thread reply/i }).fill(replyMarker);
  await panel.getByRole("button", { name: /^send$/i }).click();

  await expectPhotoMessage(panel, replyMarker, "baipas");
});

test("delivers a channel photo to a second renderer session over SSE", async ({ browser }) => {
  test.setTimeout(120_000);
  const receiverContext = await browser.newContext();
  const senderContext = await browser.newContext();
  const receiver = await receiverContext.newPage();
  const sender = await senderContext.newPage();

  try {
    await loginAndOpenGeneral(receiver, "teo");
    await loginAndOpenGeneral(sender, "baipas");

    const marker = `two client photo ${Date.now()}`;
    await attachTinyPhoto(sender);
    await sender.getByPlaceholder(/send a new message/i).fill(marker);
    await sender.getByRole("button", { name: /^send$/i }).click();

    await expectPhotoMessage(sender, marker, "baipas");
    await expectPhotoMessage(receiver, marker, "baipas");
  } finally {
    await senderContext.close();
    await receiverContext.close();
  }
});
