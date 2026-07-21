import { test, expect, type Locator, type Page } from "@playwright/test";
import { serverUrl } from "./test-config";

// The server seeds a dev user (baipas / password) and a 'general' channel on
// every start. These E2E tests rely on that seed data. Because the server's
// database is in-memory it resets on each `cargo run`, so tests that mutate
// state need to tolerate other tests having already run in the same process.

test("logs in as the dev user and lands in a channel", async ({ page }) => {
  await page.goto("/");

  await page.getByPlaceholder("Server URL").fill(serverUrl);
  await page.getByPlaceholder("Username").fill("baipas");
  await page.getByPlaceholder("Password").fill("password");
  await page.getByRole("button", { name: /sign in/i }).click();

  // The app auto-navigates to the first text channel on login. Which one
  // that is depends on prior test ordering (the reorder spec promotes a
  // created channel), so just confirm login succeeded and the `general`
  // channel is reachable in the sidebar.
  await expect(page.locator("aside").getByText("baipas").last()).toBeVisible();
  await expect(
    page.getByRole("navigation", { name: /channels/i }).getByRole("link", { name: "general" }),
  ).toBeVisible();
});

async function expectEditorValue(input: Locator, expected: string) {
  await expect
    .poll(() =>
      input.evaluate((element) => {
        if ("value" in element && typeof element.value === "string") return element.value;
        const serialize = (node: Node): string => {
          if (node instanceof HTMLElement) {
            if (node.dataset.editorCaretBoundary === "true") {
              return `\n${Array.from(node.childNodes, serialize).join("")}`;
            }
            if (node.dataset.editorCaretPlaceholder === "true") return "";
            if (node instanceof HTMLBRElement) return "\n";
          }
          if (node.nodeType === Node.TEXT_NODE)
            return (node.textContent ?? "").replaceAll("\u200B", "");
          return Array.from(node.childNodes, serialize).join("");
        };
        return serialize(element);
      }),
    )
    .toBe(expected);
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

async function loginAndOpenGeneral(page: Page) {
  await page.goto("/");
  await page.getByPlaceholder("Server URL").fill(serverUrl);
  await page.getByPlaceholder("Username").fill("baipas");
  await page.getByPlaceholder("Password").fill("password");
  await page.getByRole("button", { name: /sign in/i }).click();

  // Navigate to general explicitly so this test doesn't depend on whichever
  // channel the app auto-picks after reorder-spec side effects. Reading the
  // href from the sidebar keeps the test coupled to the real UI, while
  // `page.goto()` avoids flaky click-vs-drag behavior from the draggable
  // sidebar rows.
  const generalLink = page
    .getByRole("navigation", { name: /channels/i })
    .getByRole("link", { name: "general" });
  const generalHref = await generalLink.getAttribute("href");
  expect(generalHref).toBeTruthy();
  if (!generalHref) throw new Error("general link is missing an href");
  await page.goto(generalHref);
  await expect(page).toHaveURL(new RegExp(`${generalHref.replace("/", "\\/")}$`));
  await expect(page.getByRole("heading", { name: /^#\s*general$/i })).toBeVisible();
}

type E2eChannel = {
  id: number;
  name: string;
  type: "text" | "voice";
  position: number;
};

async function authedApiJson<T>(
  page: Page,
  path: string,
  options: { method?: string; body?: unknown } = {},
): Promise<T> {
  return page.evaluate(
    async ({ serverUrl, path, options }) => {
      const apiBaseUrl = localStorage.getItem("hamlet.serverUrl") ?? serverUrl;
      const method = options.method ?? "GET";
      const unsafe = !["GET", "HEAD", "OPTIONS"].includes(method.toUpperCase());
      let csrf = document.cookie
        .split(";")
        .map((cookie) => cookie.trim())
        .find((cookie) => cookie.startsWith("hamlet_csrf="))
        ?.slice("hamlet_csrf=".length);

      if (unsafe && !csrf) {
        const csrfResponse = await fetch(`${apiBaseUrl}/csrf`, { credentials: "include" });
        if (!csrfResponse.ok) throw new Error(`CSRF request failed: ${csrfResponse.status}`);
        csrf = ((await csrfResponse.json()) as { token: string }).token;
      }

      const headers: Record<string, string> = {};
      if (options.body !== undefined) headers["Content-Type"] = "application/json";
      if (unsafe && csrf) headers["X-Hamlet-CSRF"] = csrf;

      const response = await fetch(`${apiBaseUrl}${path}`, {
        method,
        credentials: "include",
        headers,
        body: options.body === undefined ? undefined : JSON.stringify(options.body),
      });
      const text = await response.text();
      if (!response.ok) {
        throw new Error(`${method} ${path} failed: ${response.status} ${text}`);
      }
      return text.length > 0 ? JSON.parse(text) : null;
    },
    { serverUrl, path, options },
  ) as Promise<T>;
}

test("sends a message and sees it render in the channel", async ({ page }) => {
  await loginAndOpenGeneral(page);

  const marker = `hello from playwright ${Date.now()}`;
  const input = page.getByPlaceholder(/send a new message/i);
  await input.fill(marker);
  await input.press("Enter");

  await expect(
    page.locator(".whitespace-pre-wrap:not([role='textbox'])").filter({ hasText: marker }).last(),
  ).toBeVisible({ timeout: 10_000 });
});

test("can scroll from the newest message back to the oldest message in a long channel", async ({
  page,
}) => {
  await loginAndOpenGeneral(page);

  const unique = Date.now();
  const channel = await authedApiJson<E2eChannel>(page, "/channel", {
    method: "POST",
    body: { name: `scroll-${unique}`, type: "text" },
  });
  const messages = Array.from(
    { length: 36 },
    (_, index) => `scroll history ${unique} ${String(index + 1).padStart(2, "0")}`,
  );

  for (const text of messages) {
    await authedApiJson(page, `/message/${channel.id}`, { method: "POST", body: { text } });
  }

  await page.goto(`/channel/${channel.id}`);
  await expect(
    page.getByRole("heading", { name: new RegExp(`#\\s*${escapeRegExp(channel.name)}`) }),
  ).toBeVisible();

  const messageRegion = page.getByRole("region", { name: /messages/i });
  await expect(messageRegion).toHaveCSS("overscroll-behavior-y", "none");
  const oldestMessage = messageRegion
    .locator(".whitespace-pre-wrap:not([role='textbox'])")
    .filter({ hasText: messages[0] });
  const newestMessage = messageRegion
    .locator(".whitespace-pre-wrap:not([role='textbox'])")
    .filter({ hasText: messages.at(-1) ?? "" });

  await expect(newestMessage).toBeInViewport({ timeout: 10_000 });
  await messageRegion.evaluate((element) => {
    element.scrollTop = 0;
    element.dispatchEvent(new Event("scroll", { bubbles: true }));
  });
  await expect(oldestMessage).toBeInViewport();
});

test("sends an inline reply and renders its compact preview in the channel", async ({ page }) => {
  await loginAndOpenGeneral(page);

  const unique = Date.now();
  const targetText = `inline reply target ${unique}`;
  const replyText = `inline reply body ${unique}`;
  const input = page.getByPlaceholder(/send a new message/i);

  await input.fill(targetText);
  await input.press("Enter");

  const targetRow = page.locator(".group").filter({ hasText: targetText }).last();
  await expect(targetRow).toBeVisible({ timeout: 10_000 });
  await targetRow.hover();
  await targetRow
    .getByRole("button", {
      name: new RegExp(`reply inline to message by baipas: ${escapeRegExp(targetText)}`, "i"),
    })
    .click();

  const banner = page.getByRole("status", {
    name: new RegExp(`inline reply target: replying to baipas: ${escapeRegExp(targetText)}`, "i"),
  });
  await expect(banner).toBeVisible();
  const bannerId = await banner.getAttribute("id");
  expect(bannerId).toBeTruthy();
  if (!bannerId) throw new Error("inline reply banner is missing an id");
  await expect(input).toHaveAttribute("aria-describedby", bannerId);

  await input.fill(replyText);
  await input.press("Enter");

  await expect(banner).toBeHidden({ timeout: 10_000 });
  const replyRow = page.locator(".group").filter({ hasText: replyText }).last();
  await expect(replyRow).toBeVisible({ timeout: 10_000 });
  await expect(
    replyRow.getByLabel(new RegExp(`replying to baipas: ${escapeRegExp(targetText)}`, "i")),
  ).toBeVisible();
});

test("sends a multiline message with Shift+Enter and renders both lines", async ({ page }) => {
  await loginAndOpenGeneral(page);

  const marker = `multiline from playwright ${Date.now()}`;
  const firstLine = `${marker} first line`;
  const secondLine = `${marker} second line`;
  const expected = `${firstLine}\n${secondLine}`;
  const input = page.getByPlaceholder(/send a new message/i);

  await input.fill(firstLine);
  await input.press("Shift+Enter");
  await expectEditorValue(input, `${firstLine}\n`);
  await input.pressSequentially(secondLine);
  await expectEditorValue(input, expected);
  await input.press("Enter");

  const renderedMessage = page
    .locator(".whitespace-pre-wrap:not([role='textbox'])")
    .filter({ hasText: firstLine })
    .filter({ hasText: secondLine })
    .last();

  await expect(renderedMessage).toBeVisible({ timeout: 10_000 });
  await expect
    .poll(() => renderedMessage.evaluate((element) => element.textContent))
    .toBe(expected);
});

test("keeps the caret at the start after forward deletion", async ({ page }) => {
  await loginAndOpenGeneral(page);

  const input = page.getByPlaceholder(/send a new message/i);
  await input.fill("abc");
  await expectEditorValue(input, "abc");

  await input.evaluate((element) => {
    const text = element.firstChild;
    if (!text || text.nodeType !== Node.TEXT_NODE) throw new Error("composer text node is missing");
    const range = document.createRange();
    range.setStart(text, 0);
    range.collapse(true);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
  });
  await input.press("Delete");
  await expectEditorValue(input, "bc");

  await input.pressSequentially("X");
  await expectEditorValue(input, "Xbc");
});

test("commits native emoji autocomplete and sends the emoji", async ({ page }) => {
  await loginAndOpenGeneral(page);

  const marker = `autocomplete emoji from playwright ${Date.now()} `;
  const expected = `${marker}😃`;
  const input = page.getByPlaceholder(/send a new message/i);
  await input.fill(`${marker}:sm`);

  await expect(page.getByRole("option", { name: /emoji :smiley:/i })).toBeVisible();
  await input.press("Enter");
  await expectEditorValue(input, expected);
  await input.press("Enter");

  await expect(
    page.locator(".whitespace-pre-wrap:not([role='textbox'])").filter({ hasText: expected }).last(),
  ).toBeVisible({ timeout: 10_000 });
});

test("selects an emoji from the picker and sends it", async ({ page }) => {
  await loginAndOpenGeneral(page);

  const marker = `emoji from playwright ${Date.now()} `;
  const expected = `${marker}💛`;
  const input = page.getByPlaceholder(/send a new message/i);
  await input.fill(marker);

  const emojiTrigger = page.getByRole("button", { name: /open emoji picker/i });
  await expect(emojiTrigger).toHaveCSS("cursor", "pointer");
  await emojiTrigger.click();

  const picker = page.getByRole("dialog", { name: /emoji picker/i });
  await expect(picker).toBeVisible();

  const search = picker.getByRole("combobox", { name: /search and select emoji/i });
  await search.fill("heart");

  const footer = picker.getByRole("group", { name: /emoji shortcodes/i });
  await expect(footer).toContainText(":heart:");

  const heartCell = picker.getByRole("gridcell", { name: /^Emoji :heart:$/ });
  const heartButton = heartCell.getByRole("button", { name: /^Emoji :heart:$/ });
  await expect(heartButton).toHaveCSS("cursor", "pointer");

  const yellowHeartCell = picker.getByRole("gridcell", { name: /^Emoji :yellow_heart:$/ });
  const yellowHeartButton = yellowHeartCell.getByRole("button", { name: /^Emoji :yellow_heart:$/ });
  await expect(yellowHeartButton).toHaveCSS("cursor", "pointer");
  await yellowHeartButton.click();
  await expect(picker).toBeHidden();
  await expectEditorValue(input, expected);
  await input.press("Enter");

  await expect(
    page.locator(".whitespace-pre-wrap:not([role='textbox'])").filter({ hasText: expected }).last(),
  ).toBeVisible({ timeout: 10_000 });
});
