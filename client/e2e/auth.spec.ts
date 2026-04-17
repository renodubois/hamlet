import { test, expect } from "@playwright/test";

test("rejects incorrect credentials and stays on the login screen", async ({ page }) => {
  await page.goto("/");

  await page.getByPlaceholder("Username").fill("baipas");
  await page.getByPlaceholder("Password").fill("definitely-wrong");
  await page.getByRole("button", { name: /sign in/i }).click();

  await expect(page.getByText(/invalid username or password/i)).toBeVisible();
  await expect(page.getByRole("heading", { name: /sign in/i })).toBeVisible();
});
