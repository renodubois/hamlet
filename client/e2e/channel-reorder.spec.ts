import { test, expect, type Locator } from "@playwright/test";

// The seeded database only gives us `general`. Playwright tests share a
// server process, so these tests create the extra channels they need and
// avoid relying on any ordering set by prior tests.

// Playwright's `locator.dragTo` drives HTML5 DnD via synthesized mouse
// events, which Chromium sometimes refuses to convert into the
// `dragstart`/`dragover`/`drop` sequence our sidebar listens for. Drive
// the sequence directly in a single in-page call so Solid's reactive
// updates happen back-to-back without any round-trips to the test runner.
async function dragByEvents(source: Locator, target: Locator): Promise<void> {
  const sourceId = await source.getAttribute("data-channel-id");
  const targetId = await target.getAttribute("data-channel-id");
  await source.page().evaluate(
    ({ sourceId, targetId }) => {
      const src = document.querySelector<HTMLElement>(`[data-channel-id="${sourceId}"]`);
      const tgt = document.querySelector<HTMLElement>(`[data-channel-id="${targetId}"]`);
      if (!src || !tgt) throw new Error(`drag: missing row src=${sourceId} tgt=${targetId}`);
      const dt = new DataTransfer();
      src.dispatchEvent(new DragEvent("dragstart", { bubbles: true, dataTransfer: dt }));
      tgt.dispatchEvent(new DragEvent("dragenter", { bubbles: true, dataTransfer: dt }));
      tgt.dispatchEvent(new DragEvent("dragover", { bubbles: true, dataTransfer: dt }));
      tgt.dispatchEvent(new DragEvent("drop", { bubbles: true, dataTransfer: dt }));
      src.dispatchEvent(new DragEvent("dragend", { bubbles: true, dataTransfer: dt }));
    },
    { sourceId, targetId },
  );
}

test("rearranges channels by drag-and-drop and persists the new order on reload", async ({
  page,
}) => {
  // Two channel-create round-trips + SSE refetches + a reload can bump up
  // against Playwright's default 30s budget on a loaded dev server.
  test.setTimeout(60_000);
  await page.goto("/");
  await page.getByPlaceholder("Username").fill("baipas");
  await page.getByPlaceholder("Password").fill("password");
  await page.getByRole("button", { name: /sign in/i }).click();
  await expect(page.getByText("baipas")).toBeVisible();
  await expect(page.getByRole("navigation", { name: /channels/i })).toBeVisible();

  // Seed two extra channels under unique names so concurrent runs don't
  // collide on the "random"/"dev" list.
  const suffix = Date.now().toString();
  const names = [`alpha-${suffix}`, `bravo-${suffix}`];
  for (const name of names) {
    await page.getByRole("button", { name: /add channel/i }).click();
    // Wait for the Add-Channel dialog to be ready before typing; otherwise
    // the "Create" submit below can race against the modal's initial render.
    const dialog = page.getByRole("dialog", { name: /add channel/i });
    await expect(dialog).toBeVisible();
    await dialog.getByPlaceholder(/channel name/i).fill(name);
    await dialog.getByRole("button", { name: /^create$/i }).click();
    // The modal closes when the POST resolves; wait for that so the next
    // iteration doesn't fight the old dialog for focus.
    await expect(dialog).toBeHidden();
    // The new channel appears via SSE → refetch. Locate by link text, not
    // by role.name regex (which is sensitive to whitespace normalization).
    await expect(
      page.getByRole("navigation", { name: /channels/i }).getByText(`# ${name}`),
    ).toBeVisible({ timeout: 10_000 });
  }

  const nav = page.getByRole("navigation", { name: /channels/i });
  const rowSelector = (name: string) => nav.locator(`[data-channel-id]`).filter({ hasText: name });

  // Ignore voice channels — they also carry `data-channel-id` but aren't
  // part of what this test drags around, and the seeded `voice` channel
  // would otherwise show up between `general` and the channels we create.
  async function orderedTextNames(): Promise<string[]> {
    const rows = nav.locator('[data-channel-id][data-channel-type="text"]');
    const count = await rows.count();
    const out: string[] = [];
    for (let i = 0; i < count; i++) {
      const text = (await rows.nth(i).innerText()).trim().replace(/^#\s*/, "");
      out.push(text);
    }
    return out;
  }

  // Baseline: the freshly-created channels should preserve insertion order,
  // even if older text channels from prior runs already exist above them.
  const before = await orderedTextNames();
  expect(before.indexOf(names[0])).toBeGreaterThanOrEqual(0);
  expect(before.indexOf(names[1])).toBeGreaterThan(before.indexOf(names[0]));

  // Drag the second new channel above `general`.
  await dragByEvents(rowSelector(names[1]), rowSelector("general"));

  await expect
    .poll(async () => {
      const current = await orderedTextNames();
      const dragged = current.indexOf(names[1]);
      const general = current.indexOf("general");
      const otherNew = current.indexOf(names[0]);
      return dragged >= 0 && general === dragged + 1 && otherNew > general;
    })
    .toBe(true);

  // Persistence: hard-reload and confirm the server returned the new order.
  await page.reload();
  await expect(page.getByRole("navigation", { name: /channels/i })).toBeVisible();
  await expect
    .poll(async () => {
      const current = await orderedTextNames();
      const dragged = current.indexOf(names[1]);
      const general = current.indexOf("general");
      const otherNew = current.indexOf(names[0]);
      return dragged >= 0 && general === dragged + 1 && otherNew > general;
    })
    .toBe(true);
});
