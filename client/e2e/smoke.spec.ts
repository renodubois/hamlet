import { test, expect } from "@playwright/test";

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

test("sends a message and sees it render in the channel", async ({ page }) => {
  await page.goto("/");
  await page.getByPlaceholder("Username").fill("baipas");
  await page.getByPlaceholder("Password").fill("password");
  await page.getByRole("button", { name: /sign in/i }).click();

  // Navigate to general explicitly so this test doesn't depend on whichever
  // channel the app auto-picks after reorder-spec side effects. Wait for
  // the URL to settle before touching the message input — clicking the link
  // while the auto-nav is still resolving can leave us on the wrong
  // channel and blow up the initial /messages/:id fetch.
  const generalLink = page
    .getByRole("navigation", { name: /channels/i })
    .getByText("# general");
  await generalLink.click();
  await page.waitForURL(/\/channel\/\d+$/);
  await expect(page.getByRole("heading", { name: /^#\s*general$/i })).toBeVisible();

  const marker = `hello from playwright ${Date.now()}`;
  const input = page.getByPlaceholder(/send a new message/i);
  await input.fill(marker);
  await input.press("Enter");

  await expect(page.getByText(marker)).toBeVisible({ timeout: 10_000 });
});
