import { test, expect } from "@playwright/test";
import { serverUrl } from "./test-config";

test("rejects incorrect credentials and stays on the login screen", async ({ page }) => {
  // Argon2 verification is deliberately slow (~500ms on a quiet machine,
  // noticeably more under CI load / cold-start). Give the whole test —
  // not just the assertion — generous headroom so we don't trip the
  // default 30s test budget before the error banner paints.
  test.setTimeout(60_000);

  await page.goto("/");

  await page.getByPlaceholder("Server URL").fill(serverUrl);
  await page.getByPlaceholder("Username").fill("baipas");
  await page.getByPlaceholder("Password").fill("definitely-wrong");
  await page.getByRole("button", { name: /sign in/i }).click();

  await expect(page.getByText(/invalid username or password/i)).toBeVisible({
    timeout: 30_000,
  });
  await expect(page.getByRole("heading", { name: /sign in/i })).toBeVisible();
});
