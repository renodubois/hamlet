import { readFileSync } from "node:fs";
import { test, expect, type Locator, type Page } from "@playwright/test";
import { serverUrl } from "./test-config";

const STATIC_PNG = readFileSync(new URL("../packaging/icons/32x32.png", import.meta.url));

async function loginAndOpenGeneral(page: Page, username = "baipas") {
  await page.goto("/");
  await page.getByPlaceholder("Server URL").fill(serverUrl);
  await page.getByPlaceholder("Username").fill(username);
  await page.getByPlaceholder("Password").fill("password");
  await page.getByRole("button", { name: /sign in/i }).click();

  const generalLink = page
    .getByRole("navigation", { name: /channels/i })
    .getByRole("link", { name: "general" });
  const generalHref = await generalLink.getAttribute("href");
  expect(generalHref).toBeTruthy();
  if (!generalHref) throw new Error("general link is missing an href");
  await page.goto(generalHref);
  await expect(page.getByRole("heading", { name: /^#\s*general$/i })).toBeVisible();
}

async function sendMessage(page: Page, text: string) {
  const input = page.getByPlaceholder(/send a new message/i);
  await input.fill(text);
  await input.press("Enter");
  return expectMessageRow(page, text);
}

async function expectMessageRow(page: Page, text: string): Promise<Locator> {
  const message = page
    .locator(".whitespace-pre-wrap:not([role='textbox'])")
    .filter({ hasText: text })
    .last();
  await expect(message).toBeVisible({ timeout: 10_000 });
  return message.locator("xpath=ancestor::div[contains(@class, 'group')][1]");
}

async function openReactionPicker(row: Locator) {
  await row.getByRole("button", { name: /add reaction to message by/i }).click();
  const picker = row.page().getByRole("dialog", { name: /emoji picker/i });
  await expect(picker).toBeVisible();
  return picker;
}

async function chooseNativeReaction(picker: Locator) {
  const search = picker.getByRole("combobox", { name: /search and select emoji/i });
  await search.fill("thumb");
  await search.press("Enter");
  await expect(picker).toBeHidden();
}

async function chooseCustomReaction(picker: Locator, name: string) {
  const search = picker.getByRole("combobox", { name: /search and select emoji/i });
  await search.fill(name);
  await search.press("Enter");
  await expect(picker).toBeHidden();
}

async function uploadCustomEmoji(page: Page, name: string) {
  await page.getByRole("button", { name: /^settings$/i }).click();
  const settings = page.getByRole("dialog", { name: /^settings$/i });
  await settings.getByRole("tab", { name: /custom emojis/i }).click();
  await expect(settings.getByRole("tabpanel")).toContainText("Custom Emojis");
  await settings.getByLabel(/emoji name/i).fill(name);
  await settings.getByLabel(/image file/i).setInputFiles({
    name: `${name}.png`,
    mimeType: "image/png",
    buffer: STATIC_PNG,
  });
  await settings.getByRole("button", { name: /upload emoji/i }).click();
  await expect(settings.getByRole("group", { name: `Custom emoji :${name}: active` })).toBeVisible({
    timeout: 10_000,
  });
  await page.keyboard.press("Escape");
  await expect(settings).toBeHidden();
}

async function openThreadFromRow(row: Locator) {
  await row.getByRole("button", { name: /reply in thread to message by/i }).click();
  const panel = row.page().getByRole("complementary", { name: /thread panel/i });
  await expect(panel).toBeVisible();
  return panel;
}

async function sendThreadReply(panel: Locator, text: string) {
  const input = panel.getByRole("textbox", { name: /thread reply/i });
  await input.fill(text);
  await panel.getByRole("button", { name: "Send response to thread" }).click();
  const reply = panel.locator("article", { hasText: text }).last();
  await expect(reply).toBeVisible({ timeout: 10_000 });
  return reply;
}

test("channel and thread reaction flows cover native, custom, keyboard focus, and previews", async ({
  page,
}) => {
  test.setTimeout(90_000);

  await loginAndOpenGeneral(page);
  const unique = Date.now().toString(36);
  const customName = `rx_${unique}`;
  await uploadCustomEmoji(page, customName);

  const rootText = `reaction root ${unique}`;
  const rootRow = await sendMessage(page, rootText);

  const nativePicker = await openReactionPicker(rootRow);
  await chooseNativeReaction(nativePicker);
  const nativePill = rootRow.getByRole("button", {
    name: /👍 1 reaction\. remove your reaction/i,
  });
  await expect(nativePill).toHaveAttribute("aria-pressed", "true");
  await nativePill.focus();
  await expect(page.getByRole("tooltip")).toContainText("You");
  await nativePill.click();
  await expect(nativePill).toBeHidden();

  const customPicker = await openReactionPicker(rootRow);
  await chooseCustomReaction(customPicker, customName);
  const channelCustomPill = rootRow.getByRole("button", {
    name: new RegExp(`:${customName}: 1 reaction\\. remove your reaction`, "i"),
  });
  await expect(channelCustomPill).toHaveAttribute("aria-pressed", "true");
  await channelCustomPill.click();
  await expect(channelCustomPill).toBeHidden();

  const panel = await openThreadFromRow(rootRow);
  const replyText = `reaction reply ${unique}`;
  const replyRow = await sendThreadReply(panel, replyText);

  const threadNativePicker = await openReactionPicker(replyRow);
  await chooseNativeReaction(threadNativePicker);
  const threadNativePill = replyRow.getByRole("button", {
    name: /👍 1 reaction\. remove your reaction/i,
  });
  await expect(threadNativePill).toHaveAttribute("aria-pressed", "true");
  await threadNativePill.click();
  await expect(threadNativePill).toBeHidden();

  const threadCustomPicker = await openReactionPicker(replyRow);
  await chooseCustomReaction(threadCustomPicker, customName);
  const threadCustomPill = replyRow.getByRole("button", {
    name: new RegExp(`:${customName}: 1 reaction\\. remove your reaction`, "i"),
  });
  await expect(threadCustomPill).toHaveAttribute("aria-pressed", "true");
  await threadCustomPill.click();
  await expect(threadCustomPill).toBeHidden();
});

test("reaction changes propagate live across open browser clients", async ({
  browser,
  baseURL,
}) => {
  test.setTimeout(90_000);

  const aliceContext = await browser.newContext({ baseURL });
  const bobContext = await browser.newContext({ baseURL });
  const alice = await aliceContext.newPage();
  const bob = await bobContext.newPage();

  try {
    await loginAndOpenGeneral(alice, "baipas");
    await loginAndOpenGeneral(bob, "teo");

    const marker = `multiwindow reaction ${Date.now()}`;
    const aliceRow = await sendMessage(alice, marker);
    const bobRow = await expectMessageRow(bob, marker);

    const picker = await openReactionPicker(bobRow);
    await chooseNativeReaction(picker);
    await expect(
      bobRow.getByRole("button", { name: /👍 1 reaction\. remove your reaction/i }),
    ).toHaveAttribute("aria-pressed", "true");

    await expect(
      aliceRow.getByRole("button", { name: /👍 1 reaction\. add your reaction/i }),
    ).toHaveAttribute("aria-pressed", "false", { timeout: 10_000 });
  } finally {
    await bobContext.close();
    await aliceContext.close();
  }
});
