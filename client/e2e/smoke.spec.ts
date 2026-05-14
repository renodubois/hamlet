import { test, expect, type Page } from "@playwright/test";

// The server seeds a dev user (baipas / password) and a 'general' channel on
// every start. These E2E tests rely on that seed data. Because the server's
// database is in-memory it resets on each `cargo run`, so tests that mutate
// state need to tolerate other tests having already run in the same process.

test("logs in as the dev user and lands in a channel", async ({ page }) => {
  await page.goto("/");

  await page.getByPlaceholder("Username").fill("baipas");
  await page.getByPlaceholder("Password").fill("password");
  await page.getByRole("button", { name: /sign in/i }).click();

  // The app auto-navigates to the first text channel on login. Which one
  // that is depends on prior test ordering (the reorder spec promotes a
  // created channel), so just confirm login succeeded and the `general`
  // channel is reachable in the sidebar.
  await expect(page.getByText("baipas")).toBeVisible();
  await expect(
    page.getByRole("navigation", { name: /channels/i }).getByText("# general"),
  ).toBeVisible();
});

async function loginAndOpenGeneral(page: Page) {
  await page.goto("/");
  await page.getByPlaceholder("Username").fill("baipas");
  await page.getByPlaceholder("Password").fill("password");
  await page.getByRole("button", { name: /sign in/i }).click();

  // Navigate to general explicitly so this test doesn't depend on whichever
  // channel the app auto-picks after reorder-spec side effects. Reading the
  // href from the sidebar keeps the test coupled to the real UI, while
  // `page.goto()` avoids flaky click-vs-drag behavior from the draggable
  // sidebar rows.
  const generalLink = page.getByRole("navigation", { name: /channels/i }).getByText("# general");
  const generalHref = await generalLink.getAttribute("href");
  expect(generalHref).toBeTruthy();
  if (!generalHref) throw new Error("general link is missing an href");
  await page.goto(generalHref);
  await expect(page).toHaveURL(new RegExp(`${generalHref.replace("/", "\\/")}$`));
  await expect(page.getByRole("heading", { name: /^#\s*general$/i })).toBeVisible();
}

test("sends a message and sees it render in the channel", async ({ page }) => {
  await loginAndOpenGeneral(page);

  const marker = `hello from playwright ${Date.now()}`;
  const input = page.getByPlaceholder(/send a new message/i);
  await input.fill(marker);
  await input.press("Enter");

  await expect(page.getByText(marker)).toBeVisible({ timeout: 10_000 });
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

  await search.press("ArrowRight");
  const yellowHeartCell = picker.getByRole("gridcell", { name: /^Emoji :yellow_heart:$/ });
  await expect(yellowHeartCell).toHaveAttribute("aria-selected", "true");
  await expect(footer).toContainText(":yellow_heart:");

  await search.press("Enter");
  await expect(picker).toBeHidden();
  await expect(input).toHaveValue(expected);
  await input.press("Enter");

  await expect(page.getByText(expected)).toBeVisible({ timeout: 10_000 });
});
