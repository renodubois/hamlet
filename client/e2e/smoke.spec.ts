import { test, expect } from "@playwright/test";

// The server seeds a dev user (baipas / password) and a 'general' channel on
// every start. These E2E tests rely on that seed data. Because the server's
// database is in-memory it resets on each `cargo run`, so tests that mutate
// state need to tolerate other tests having already run in the same process.

test("logs in as the dev user and lands in the general channel", async ({ page }) => {
  await page.goto("/");

  await page.getByPlaceholder("Username").fill("baipas");
  await page.getByPlaceholder("Password").fill("password");
  await page.getByRole("button", { name: /sign in/i }).click();

  await expect(page.getByRole("heading", { name: /general/i })).toBeVisible();
  await expect(page.getByText("baipas")).toBeVisible();
});

test("sends a message and sees it render in the channel", async ({ page }) => {
  await page.goto("/");
  await page.getByPlaceholder("Username").fill("baipas");
  await page.getByPlaceholder("Password").fill("password");
  await page.getByRole("button", { name: /sign in/i }).click();

  await expect(page.getByRole("heading", { name: /general/i })).toBeVisible();

  const marker = `hello from playwright ${Date.now()}`;
  const input = page.getByPlaceholder(/send a new message/i);
  await input.fill(marker);
  await input.press("Enter");

  await expect(page.getByText(marker)).toBeVisible({ timeout: 5000 });
});
