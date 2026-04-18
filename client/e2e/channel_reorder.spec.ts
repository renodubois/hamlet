import { test, expect } from "@playwright/test";

// The seeded database only gives us `general`. Playwright tests share a
// server process, so these tests create the extra channels they need and
// avoid relying on any ordering set by prior tests.

test("rearranges channels by drag-and-drop and persists the new order on reload", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByPlaceholder("Username").fill("baipas");
  await page.getByPlaceholder("Password").fill("password");
  await page.getByRole("button", { name: /sign in/i }).click();
  await expect(page.getByRole("heading", { name: /general/i })).toBeVisible();

  // Seed two extra channels under unique names so concurrent runs don't
  // collide on the "random"/"dev" list.
  const suffix = Date.now().toString();
  const names = [`alpha-${suffix}`, `bravo-${suffix}`];
  for (const name of names) {
    await page.getByRole("button", { name: /add channel/i }).click();
    await page.getByPlaceholder(/channel name/i).fill(name);
    await page.getByRole("button", { name: /^create$/i }).click();
    await expect(page.getByRole("link", { name: new RegExp(`#\\s*${name}$`) })).toBeVisible();
  }

  const nav = page.getByRole("navigation", { name: /channels/i });
  const rowSelector = (name: string) => nav.locator(`[data-channel-id]`).filter({ hasText: name });

  async function orderedNames(): Promise<string[]> {
    const rows = nav.locator("[data-channel-id]");
    const count = await rows.count();
    const out: string[] = [];
    for (let i = 0; i < count; i++) {
      const text = (await rows.nth(i).innerText()).trim().replace(/^#\s*/, "");
      out.push(text);
    }
    return out;
  }

  // Baseline: newly-created channels should sit below `general` in
  // insertion order, since the server assigns position = max + 1.
  const before = await orderedNames();
  expect(before.slice(0, 3)).toEqual(["general", names[0], names[1]]);

  // Drag the second new channel above `general`.
  await rowSelector(names[1]).dragTo(rowSelector("general"));

  await expect
    .poll(async () => (await orderedNames()).slice(0, 3))
    .toEqual([names[1], "general", names[0]]);

  // Persistence: hard-reload and confirm the server returned the new order.
  await page.reload();
  await expect(page.getByRole("heading", { name: /general/i })).toBeVisible();
  await expect
    .poll(async () => (await orderedNames()).slice(0, 3))
    .toEqual([names[1], "general", names[0]]);
});
