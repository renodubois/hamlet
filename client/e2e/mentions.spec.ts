import { test, expect, type Locator, type Page } from "@playwright/test";
import { rendererUrl, serverUrl } from "./test-config";

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

async function loginAndOpenGeneral(page: Page, username = "baipas") {
  await page.goto("/");
  await page.getByPlaceholder("Server URL").fill(serverUrl);
  await page.getByPlaceholder("Username").fill(username);
  await page.getByPlaceholder("Password").fill("password");
  await page.getByRole("button", { name: /sign in/i }).click();

  const generalLink = page.getByRole("navigation", { name: /channels/i }).getByText("# general");
  await expect(generalLink).toBeVisible();
  const generalHref = await generalLink.getAttribute("href");
  expect(generalHref).toBeTruthy();
  if (!generalHref) throw new Error("general link is missing an href");

  await page.goto(new URL(generalHref, rendererUrl).toString());
  await expect(page.getByRole("heading", { name: /^#\s*general$/i })).toBeVisible();
}

function channelMessageRow(page: Page, text: string): Locator {
  return page
    .getByRole("region", { name: /messages/i })
    .locator("[data-message-id]")
    .filter({ hasText: text })
    .last();
}

function threadMessageRow(panel: Locator, text: string): Locator {
  return panel.locator("article[data-message-id]").filter({ hasText: text }).last();
}

async function expectEditorValue(input: Locator, expected: RegExp) {
  await expect
    .poll(() =>
      input.evaluate((element) =>
        "value" in element && typeof element.value === "string"
          ? element.value
          : (element.textContent ?? ""),
      ),
    )
    .toMatch(expected);
}

async function moveEditorCaretToEnd(input: Locator) {
  await input.evaluate((element) => {
    const editor = element as HTMLElement & {
      value?: string;
      setSelectionRange?: (start: number, end?: number) => void;
    };
    const value = typeof editor.value === "string" ? editor.value : (editor.textContent ?? "");
    editor.focus();
    editor.setSelectionRange?.(value.length, value.length);
  });
}

async function commitMentionWithActiveOption(
  page: Page,
  input: Locator,
  username: string,
  displayName = username,
) {
  await moveEditorCaretToEnd(input);

  const listbox = page.getByRole("listbox", { name: /mention suggestions/i });
  await expect(listbox).toBeVisible();

  const optionLabel =
    displayName === username ? `Mention @${username}` : `Mention ${displayName} @${username}`;
  const option = listbox.getByRole("option", {
    name: new RegExp(`^${escapeRegExp(optionLabel)}$`, "i"),
  });
  await expect(option).toBeVisible();
  await expect(option).toHaveAttribute("aria-selected", "true");

  const listboxId = await listbox.getAttribute("id");
  const optionId = await option.getAttribute("id");
  expect(listboxId).toBeTruthy();
  expect(optionId).toBeTruthy();
  if (!listboxId) throw new Error("mention listbox is missing an id");
  if (!optionId) throw new Error("mention option is missing an id");
  await expect(input).toHaveAttribute("aria-controls", listboxId);
  await expect(input).toHaveAttribute("aria-activedescendant", optionId);

  await input.press("Enter");
  await expect(listbox).toBeHidden();
  await expectEditorValue(input, /<@\d+> $/);
  await expect(input.locator("span", { hasText: `@${displayName}` })).toBeVisible();
}

test("sends and edits a channel mention through autocomplete with accessible preview", async ({
  page,
}) => {
  await loginAndOpenGeneral(page);

  const unique = Date.now();
  const linkUrl = `https://example.com/hamlet-mention-${unique}`;
  const initialMarker = `mention e2e channel ${unique}`;
  const input = page.getByRole("textbox", { name: /new message/i });

  await input.fill(`${initialMarker} ${linkUrl} @teo`);
  await commitMentionWithActiveOption(page, input, "teo");
  await input.press("Enter");

  const initialRow = channelMessageRow(page, initialMarker);
  await expect(initialRow).toBeVisible({ timeout: 10_000 });
  const initialMessageId = await initialRow.getAttribute("data-message-id");
  expect(initialMessageId).toBeTruthy();
  if (!initialMessageId) throw new Error("created mention message is missing a message id");
  const editingRow = page
    .getByRole("region", { name: /messages/i })
    .locator(`[data-message-id="${initialMessageId}"]`);
  await expect(initialRow.getByRole("link", { name: linkUrl })).toHaveAttribute("href", linkUrl);

  const teoMention = initialRow.getByRole("button", { name: "Mention teo (@teo)" });
  await expect(teoMention).toBeVisible();
  await expect(teoMention).toHaveAttribute("title", "@teo");
  await teoMention.click();
  await expect(page.getByRole("dialog", { name: "Profile preview for teo (@teo)" })).toBeVisible();
  await page.keyboard.press("Escape");

  await editingRow.hover();
  await editingRow.getByRole("button", { name: /^Edit$/ }).click();

  const editedMarker = `mention e2e edited ${unique}`;
  const editInput = editingRow.getByRole("textbox", { name: /edit message/i });
  await editInput.fill(`${editedMarker} @teo`);
  await commitMentionWithActiveOption(page, editInput, "teo");
  await editingRow.getByRole("button", { name: /^Save$/ }).click();

  const editedRow = channelMessageRow(page, editedMarker);
  await expect(editedRow).toBeVisible({ timeout: 10_000 });
  await expect(editedRow.getByRole("button", { name: "Mention teo (@teo)" })).toBeVisible();
  await expect(
    page
      .getByRole("region", { name: /messages/i })
      .locator("[data-message-id]")
      .filter({ hasText: initialMarker }),
  ).toHaveCount(0);
});

test("sends a thread reply mention through autocomplete and updates the thread summary", async ({
  page,
}) => {
  await loginAndOpenGeneral(page);

  const unique = Date.now();
  const rootMarker = `mention e2e thread root ${unique}`;
  const input = page.getByRole("textbox", { name: /new message/i });
  await input.fill(rootMarker);
  await input.press("Enter");

  const rootRow = channelMessageRow(page, rootMarker);
  await expect(rootRow).toBeVisible({ timeout: 10_000 });
  await rootRow.hover();
  await rootRow.getByRole("button", { name: /reply in thread to message by baipas/i }).click();

  const panel = page.getByRole("complementary", { name: /thread panel/i });
  await expect(panel).toBeVisible();

  const replyMarker = `mention e2e thread reply ${unique}`;
  const threadInput = panel.getByRole("textbox", { name: /thread reply/i });
  await threadInput.fill(`${replyMarker} @teo`);
  await commitMentionWithActiveOption(page, threadInput, "teo");
  await panel.getByRole("button", { name: /^Send$/ }).click();

  const replyRow = threadMessageRow(panel, replyMarker);
  await expect(replyRow).toBeVisible({ timeout: 10_000 });
  await expect(replyRow.getByRole("button", { name: "Mention teo (@teo)" })).toBeVisible();
  await expect(rootRow.getByRole("button", { name: /open thread with 1 reply/i })).toBeVisible({
    timeout: 10_000,
  });
});

test("delivers mentioned messages over SSE with current-user emphasis", async ({ browser }) => {
  const receiverContext = await browser.newContext();
  const senderContext = await browser.newContext();
  const receiver = await receiverContext.newPage();
  const sender = await senderContext.newPage();

  try {
    await loginAndOpenGeneral(receiver, "teo");
    await loginAndOpenGeneral(sender, "baipas");

    const unique = Date.now();
    const marker = `mention e2e live update ${unique}`;
    const senderInput = sender.getByRole("textbox", { name: /new message/i });
    await senderInput.fill(`${marker} @teo`);
    await commitMentionWithActiveOption(sender, senderInput, "teo");
    await senderInput.press("Enter");

    const receiverRow = channelMessageRow(receiver, marker);
    await expect(receiverRow).toBeVisible({ timeout: 10_000 });
    await expect(receiverRow.getByRole("button", { name: "Mention teo (@teo)" })).toBeVisible();
    await expect(receiverRow).toHaveAttribute("data-mentioned-current-user", "true");
    await expect(receiverRow).toHaveClass(/bg-yellow-50/);
  } finally {
    await senderContext.close();
    await receiverContext.close();
  }
});
