import { readFileSync } from "node:fs";
import { test, expect, type Locator, type Page } from "@playwright/test";

// Small checked-in PNG. The server normalizes static uploads to 256x256 WebP.
const STATIC_PNG = readFileSync(new URL("../packaging/icons/32x32.png", import.meta.url));

// Two-frame 1x1 GIF fixture. E2E only asserts upload/render plumbing, not animation playback.
const ANIMATED_GIF = Buffer.from([
  0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xff, 0xff, 0xff,
  0x00, 0x00, 0x00, 0x21, 0xf9, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00,
  0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01, 0x00, 0x21, 0xf9, 0x04, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01,
  0x00, 0x3b,
]);

async function loginAndOpenGeneral(page: Page) {
  await page.goto("/");
  await page.getByPlaceholder("Server URL").fill("http://127.0.0.1:3030");
  await page.getByPlaceholder("Username").fill("baipas");
  await page.getByPlaceholder("Password").fill("password");
  await page.getByRole("button", { name: /sign in/i }).click();

  const generalLink = page.getByRole("navigation", { name: /channels/i }).getByText("# general");
  const generalHref = await generalLink.getAttribute("href");
  expect(generalHref).toBeTruthy();
  if (!generalHref) throw new Error("general link is missing an href");
  await page.goto(generalHref);
  await expect(page.getByRole("heading", { name: /^#\s*general$/i })).toBeVisible();
}

async function openCustomEmojiSettings(page: Page): Promise<Locator> {
  await page.getByRole("button", { name: /^settings$/i }).click();
  const settings = page.getByRole("dialog", { name: /^settings$/i });
  await settings.getByRole("tab", { name: /custom emojis/i }).click();
  await expect(settings.getByRole("tabpanel")).toContainText("Custom Emojis");
  return settings;
}

async function closeSettings(page: Page, settings: Locator) {
  await page.keyboard.press("Escape");
  await expect(settings).toBeHidden();
}

async function uploadEmoji(
  settings: Locator,
  name: string,
  file: { name: string; mimeType: string; buffer: Buffer },
) {
  await settings.getByLabel(/emoji name/i).fill(name);
  await settings.getByLabel(/image file/i).setInputFiles(file);
  await settings.getByRole("button", { name: /upload emoji/i }).click();
  const row = settings.getByRole("group", { name: `Custom emoji :${name}: active` });
  await expect(row).toBeVisible({ timeout: 10_000 });
  return row;
}

async function expectEditorValue(input: Locator, expected: string | RegExp) {
  const value = () =>
    input.evaluate((element) =>
      "value" in element && typeof element.value === "string" ? element.value : element.textContent,
    );

  if (typeof expected === "string") {
    await expect.poll(value).toBe(expected);
  } else {
    await expect.poll(value).toMatch(expected);
  }
}

test("custom emoji upload, use, rename, delete, restore, and animated smoke", async ({ page }) => {
  test.setTimeout(90_000);

  await loginAndOpenGeneral(page);

  const unique = Date.now().toString(36);
  const emojiName = `pw_${unique}`;
  const renamed = `ren_${unique}`;
  const animatedName = `gif_${unique}`;

  let settings = await openCustomEmojiSettings(page);
  await uploadEmoji(settings, emojiName, {
    name: `${emojiName}.png`,
    mimeType: "image/png",
    buffer: STATIC_PNG,
  });
  await closeSettings(page, settings);

  const input = page.getByPlaceholder(/send a new message/i);
  await input.fill(`picker ${emojiName} `);
  await page.getByRole("button", { name: /open emoji picker/i }).click();
  const picker = page.getByRole("dialog", { name: /emoji picker/i });
  await picker.getByRole("combobox", { name: /search and select emoji/i }).fill(emojiName);
  await picker
    .getByRole("gridcell", { name: `Emoji :${emojiName}:` })
    .getByRole("button")
    .click();
  await expectEditorValue(input, new RegExp(`picker ${emojiName} <:${emojiName}:\\d+>`));
  await input.press("Enter");

  const renderedCustomImages = page.locator("main").getByRole("img", { name: `:${emojiName}:` });
  await expect(renderedCustomImages).toHaveCount(1, { timeout: 10_000 });

  settings = await openCustomEmojiSettings(page);
  await settings.getByLabel(`Rename :${emojiName}:`).fill(renamed);
  await settings.getByLabel(`Rename :${emojiName}:`).press("Enter");
  await expect(
    settings.getByRole("group", { name: `Custom emoji :${renamed}: active` }),
  ).toBeVisible();
  await closeSettings(page, settings);

  await expect(page.locator("main").getByRole("img", { name: `:${renamed}:` })).toHaveCount(1);

  await input.fill("autocomplete ");
  await input.pressSequentially(`:${renamed}:`);
  await expectEditorValue(input, new RegExp(`autocomplete <:${renamed}:\\d+>`));
  await input.press("Enter");
  await expect(page.locator("main").getByRole("img", { name: `:${renamed}:` })).toHaveCount(2, {
    timeout: 10_000,
  });

  settings = await openCustomEmojiSettings(page);
  const activeRow = settings.getByRole("group", { name: `Custom emoji :${renamed}: active` });
  page.once("dialog", (dialog) => void dialog.accept());
  await activeRow.getByRole("button", { name: /^delete$/i }).click();
  await expect(
    settings.getByRole("group", { name: `Custom emoji :${renamed}: deleted` }),
  ).toBeVisible();
  await closeSettings(page, settings);

  await expect(page.locator("main").getByRole("img", { name: `:${renamed}:` })).toHaveCount(2);

  await input.fill(`deleted :${renamed}:`);
  await expectEditorValue(input, `deleted :${renamed}:`);
  await page.getByRole("button", { name: /open emoji picker/i }).click();
  await picker.getByRole("combobox", { name: /search and select emoji/i }).fill(renamed);
  await expect(picker.getByText(/no emojis found/i)).toBeVisible();
  await page.keyboard.press("Escape");

  settings = await openCustomEmojiSettings(page);
  const deletedRow = settings.getByRole("group", { name: `Custom emoji :${renamed}: deleted` });
  await deletedRow.getByRole("button", { name: /^restore$/i }).click();
  await expect(
    settings.getByRole("group", { name: `Custom emoji :${renamed}: active` }),
  ).toBeVisible();
  await closeSettings(page, settings);

  await input.fill("after restore ");
  await input.pressSequentially(`:${renamed}:`);
  await expectEditorValue(input, new RegExp(`after restore <:${renamed}:\\d+>`));
  await input.press("Enter");
  await expect(page.locator("main").getByRole("img", { name: `:${renamed}:` })).toHaveCount(3, {
    timeout: 10_000,
  });

  settings = await openCustomEmojiSettings(page);
  const animatedRow = await uploadEmoji(settings, animatedName, {
    name: `${animatedName}.gif`,
    mimeType: "image/gif",
    buffer: ANIMATED_GIF,
  });
  await expect(animatedRow.getByText("animated")).toBeVisible();
});
