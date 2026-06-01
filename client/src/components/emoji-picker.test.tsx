import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { describe, expect, test, vi } from "vitest";
import type { EmojiEntry } from "../emoji/emoji-data";
import { expectNoA11yViolations } from "../test/a11y";
import EmojiPicker from "./emoji-picker";

const EMOJIS: readonly EmojiEntry[] = [
  {
    emoji: "😄",
    shortcodes: [":smile:"],
    category: "Smileys & Emotion",
  },
  {
    emoji: "❤️",
    shortcodes: [":heart:"],
    category: "Smileys & Emotion",
  },
  {
    emoji: "💛",
    shortcodes: [":yellow_heart:"],
    category: "Smileys & Emotion",
  },
  {
    emoji: "👍",
    shortcodes: [":thumbsup:", ":+1:"],
    category: "People & Body",
  },
  {
    emoji: "👎",
    shortcodes: [":thumbsdown:", ":-1:"],
    category: "People & Body",
  },
  {
    emoji: "✅",
    shortcodes: [":white_check_mark:"],
    category: "Symbols",
  },
  {
    emoji: "🔥",
    shortcodes: [":fire:"],
    category: "Travel & Places",
  },
  {
    emoji: "🎉",
    shortcodes: [":tada:"],
    category: "Activities",
  },
  {
    emoji: "🚀",
    shortcodes: [":rocket:"],
    category: "Travel & Places",
  },
  {
    kind: "custom",
    emoji: "<:party:123>",
    shortcodes: [":party:"],
    category: "Custom",
    id: 123,
    name: "party",
    marker: "<:party:123>",
    imageUrl: "/uploads/emojis/123.webp?v=1",
    animated: false,
    deletedAt: null,
  },
  {
    kind: "custom",
    emoji: "<a:dance:456>",
    shortcodes: [":dance:"],
    category: "Custom",
    id: 456,
    name: "dance",
    marker: "<a:dance:456>",
    imageUrl: "/uploads/emojis/456.gif?v=1",
    animated: true,
    deletedAt: null,
  },
];

const searchName = /search and select emoji/i;
const pickerName = /emoji picker/i;
const footerName = /emoji shortcodes/i;

function renderHarness(initialOpen = true, onSelect = vi.fn(), onClose = vi.fn()) {
  return render(() => {
    const [open, setOpen] = createSignal(initialOpen);
    let anchor: HTMLButtonElement | undefined;
    const close = () => {
      onClose();
      setOpen(false);
    };

    return (
      <>
        <button
          ref={(el) => {
            anchor = el;
          }}
          type="button"
        >
          anchor
        </button>
        <EmojiPicker
          open={open()}
          anchor={() => anchor}
          emojis={EMOJIS}
          onSelect={onSelect}
          onClose={close}
        />
      </>
    );
  });
}

function getSearch(root: HTMLElement | Document = document): HTMLInputElement {
  return within(root as HTMLElement).getByRole("combobox", {
    name: searchName,
  }) as HTMLInputElement;
}

function getEmojiGridcell(shortcodes: string | RegExp, root: HTMLElement | Document = document) {
  const name = typeof shortcodes === "string" ? `Emoji ${shortcodes}` : shortcodes;
  return within(root as HTMLElement).getByRole("gridcell", { name });
}

function getFooter(root: HTMLElement | Document = document) {
  return within(root as HTMLElement).getByRole("group", { name: footerName });
}

function expectActiveEmoji(shortcodes: string | RegExp, root: HTMLElement | Document = document) {
  const search = getSearch(root);
  const gridcell = getEmojiGridcell(shortcodes, root);

  expect(gridcell).toHaveAttribute("aria-selected", "true");
  expect(search).toHaveAttribute("aria-activedescendant", gridcell.id);

  return gridcell;
}

function keyDown(target: Element, init: KeyboardEventInit & { key: string }) {
  const event = new KeyboardEvent("keydown", {
    bubbles: true,
    cancelable: true,
    ...init,
  });
  target.dispatchEvent(event);
  return event;
}

function composingKeyDown(target: Element, key: string) {
  const event = new KeyboardEvent("keydown", {
    bubbles: true,
    cancelable: true,
    key,
  });
  Object.defineProperty(event, "isComposing", { value: true });
  target.dispatchEvent(event);
  return event;
}

describe("<EmojiPicker>", () => {
  test("renders nothing when closed", () => {
    renderHarness(false);
    expect(screen.queryByRole("dialog", { name: pickerName })).toBeNull();
  });

  test("renders search, emoji results, and the active shortcode footer when open", async () => {
    renderHarness();

    const dialog = await screen.findByRole("dialog", { name: pickerName });
    expect(getSearch(dialog)).toBeInTheDocument();
    expect(within(dialog).getByRole("grid", { name: /emoji results/i })).toBeInTheDocument();
    expect(within(dialog).getByRole("gridcell", { name: /emoji :smile:/i })).toBeInTheDocument();
    expect(within(dialog).getByRole("gridcell", { name: /emoji :heart:/i })).toBeInTheDocument();
    expect(getFooter(dialog)).toHaveTextContent(":smile:");
  });

  test("focuses the search input when opened", async () => {
    renderHarness();

    const search = await screen.findByRole("combobox", { name: searchName });
    await waitFor(() => expect(document.activeElement).toBe(search));
  });

  test("activates the first result on open and after search", async () => {
    renderHarness();

    const search = await screen.findByRole("combobox", { name: searchName });
    expectActiveEmoji(":smile:");

    fireEvent.input(search, { target: { value: "heart" } });

    expectActiveEmoji(":heart:");
    expect(getFooter()).toHaveTextContent(":heart:");
  });

  test("filters by shortcode with colon and bare queries", async () => {
    renderHarness();

    const search = await screen.findByRole("combobox", { name: searchName });
    fireEvent.input(search, { target: { value: ":thumbsup:" } });
    expect(getEmojiGridcell(/emoji :thumbsup:, :\+1:/i)).toBeInTheDocument();

    fireEvent.input(search, { target: { value: "smile" } });
    expect(getEmojiGridcell(":smile:")).toBeInTheDocument();
    expect(screen.queryByRole("gridcell", { name: /emoji :thumbsup:/i })).toBeNull();
  });

  test("normalizes shortcode separators and case while ignoring removed keywords", async () => {
    renderHarness();

    const search = await screen.findByRole("combobox", { name: searchName });
    fireEvent.input(search, { target: { value: "WHITE check-mark" } });
    expect(getEmojiGridcell(":white_check_mark:")).toBeInTheDocument();

    fireEvent.input(search, { target: { value: "favorite" } });
    expect(screen.getByText(/no emojis found/i)).toBeInTheDocument();
    expect(screen.queryByRole("gridcell", { name: /emoji :heart:/i })).toBeNull();
  });

  test("footer renders all active emoji shortcodes", async () => {
    renderHarness();

    const search = await screen.findByRole("combobox", { name: searchName });
    fireEvent.input(search, { target: { value: "thumb" } });

    const footer = getFooter();
    expect(footer).toHaveTextContent(":thumbsup:");
    expect(footer).toHaveTextContent(":+1:");
  });

  test("shows an empty state and hides the footer when no emojis match", async () => {
    renderHarness();

    const search = await screen.findByRole("combobox", { name: searchName });
    fireEvent.input(search, { target: { value: "not-an-emoji" } });

    expect(screen.getByText(/no emojis found/i)).toBeInTheDocument();
    expect(screen.queryByLabelText(footerName)).toBeNull();
  });

  test("selecting an emoji calls onSelect and closes", async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();
    renderHarness(true, onSelect, onClose);

    const heartCell = await screen.findByRole("gridcell", { name: /emoji :heart:/i });
    fireEvent.click(within(heartCell).getByRole("button", { name: /emoji :heart:/i }));

    expect(onSelect).toHaveBeenCalledWith("❤️");
    expect(onClose).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: pickerName })).toBeNull());
  });

  test("renders custom emoji image results and selects their marker", async () => {
    const onSelect = vi.fn();
    renderHarness(true, onSelect);

    const search = await screen.findByRole("combobox", { name: searchName });
    fireEvent.input(search, { target: { value: "party" } });
    const partyCell = getEmojiGridcell(":party:");
    const image = partyCell.querySelector("img");

    expect(image?.getAttribute("src")).toContain("/uploads/emojis/123.webp?v=1");
    fireEvent.click(within(partyCell).getByRole("button", { name: /emoji :party:/i }));

    expect(onSelect).toHaveBeenCalledWith("<:party:123>");
  });

  test("renders animated custom emoji affordance and selects its animated marker", async () => {
    const onSelect = vi.fn();
    renderHarness(true, onSelect);

    const search = await screen.findByRole("combobox", { name: searchName });
    fireEvent.input(search, { target: { value: "dance" } });
    const danceCell = getEmojiGridcell(/animated emoji :dance:/i);
    const image = danceCell.querySelector("img");

    expect(image?.getAttribute("src")).toContain("/uploads/emojis/456.gif?v=1");
    expect(danceCell).toHaveTextContent("A");
    expect(getFooter()).toHaveTextContent("animated");
    fireEvent.click(within(danceCell).getByRole("button", { name: /animated emoji :dance:/i }));

    expect(onSelect).toHaveBeenCalledWith("<a:dance:456>");
  });

  test("arrows navigate the active emoji", async () => {
    renderHarness();

    const search = await screen.findByRole("combobox", { name: searchName });
    expectActiveEmoji(":smile:");

    let event = keyDown(search, { key: "ArrowRight" });
    expect(event.defaultPrevented).toBe(true);
    expectActiveEmoji(":heart:");

    event = keyDown(search, { key: "ArrowLeft" });
    expect(event.defaultPrevented).toBe(true);
    expectActiveEmoji(":smile:");

    event = keyDown(search, { key: "ArrowDown" });
    expect(event.defaultPrevented).toBe(true);
    expectActiveEmoji(":rocket:");

    event = keyDown(search, { key: "ArrowUp" });
    expect(event.defaultPrevented).toBe(true);
    expectActiveEmoji(":smile:");
  });

  test("Enter selects the active emoji", async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();
    renderHarness(true, onSelect, onClose);

    const search = await screen.findByRole("combobox", { name: searchName });
    keyDown(search, { key: "ArrowRight" });
    const event = keyDown(search, { key: "Enter" });

    expect(event.defaultPrevented).toBe(true);
    expect(onSelect).toHaveBeenCalledWith("❤️");
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  test.each(["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"] as const)(
    "no-results %s is not prevented",
    async (key) => {
      const onClose = vi.fn();
      renderHarness(true, vi.fn(), onClose);

      const search = await screen.findByRole("combobox", { name: searchName });
      fireEvent.input(search, { target: { value: "not-an-emoji" } });

      const event = keyDown(search, { key });

      expect(event.defaultPrevented).toBe(false);
      expect(onClose).not.toHaveBeenCalled();
    },
  );

  test("no-results Enter is prevented without selecting", async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();
    renderHarness(true, onSelect, onClose);

    const search = await screen.findByRole("combobox", { name: searchName });
    fireEvent.input(search, { target: { value: "not-an-emoji" } });
    const event = keyDown(search, { key: "Enter" });

    expect(event.defaultPrevented).toBe(true);
    expect(onSelect).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  test.each([
    ["Shift", { shiftKey: true }],
    ["Control", { ctrlKey: true }],
    ["Meta", { metaKey: true }],
    ["Alt", { altKey: true }],
  ] as const)("%s-modified arrows are not intercepted", async (_label, modifier) => {
    renderHarness();

    const search = await screen.findByRole("combobox", { name: searchName });
    expectActiveEmoji(":smile:");

    const event = keyDown(search, { key: "ArrowRight", ...modifier });

    expect(event.defaultPrevented).toBe(false);
    expectActiveEmoji(":smile:");
  });

  test("IME composition arrows are not intercepted", async () => {
    renderHarness();

    const search = await screen.findByRole("combobox", { name: searchName });
    expectActiveEmoji(":smile:");

    const event = composingKeyDown(search, "ArrowRight");

    expect(event.defaultPrevented).toBe(false);
    expectActiveEmoji(":smile:");
  });

  test("IME composition Enter is not intercepted", async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();
    renderHarness(true, onSelect, onClose);

    const search = await screen.findByRole("combobox", { name: searchName });
    const event = composingKeyDown(search, "Enter");

    expect(event.defaultPrevented).toBe(false);
    expect(onSelect).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  test("hover updates the footer", async () => {
    renderHarness();

    await screen.findByRole("combobox", { name: searchName });
    expect(getFooter()).toHaveTextContent(":smile:");

    fireEvent.mouseEnter(getEmojiGridcell(":heart:"));

    expectActiveEmoji(":heart:");
    expect(getFooter()).toHaveTextContent(":heart:");
  });

  test.each([
    ["Tab", false],
    ["Shift+Tab", true],
  ] as const)("%s closes the picker without preventing default", async (_label, shiftKey) => {
    const onClose = vi.fn();
    renderHarness(true, vi.fn(), onClose);

    const search = await screen.findByRole("combobox", { name: searchName });
    const event = keyDown(search, { key: "Tab", shiftKey });

    expect(event.defaultPrevented).toBe(false);
    expect(onClose).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: pickerName })).toBeNull());
  });

  test("Escape closes the picker", async () => {
    const onClose = vi.fn();
    renderHarness(true, vi.fn(), onClose);

    await screen.findByRole("dialog", { name: pickerName });
    fireEvent.keyDown(window, { key: "Escape" });

    expect(onClose).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: pickerName })).toBeNull());
  });

  test("outside mouse down closes the picker", async () => {
    const onClose = vi.fn();
    renderHarness(true, vi.fn(), onClose);

    await screen.findByRole("dialog", { name: pickerName });
    fireEvent.mouseDown(document.body);

    expect(onClose).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: pickerName })).toBeNull());
  });

  test("has no accessibility violations", async () => {
    renderHarness();
    await screen.findByRole("dialog", { name: pickerName });

    await expectNoA11yViolations(document.body, "emoji picker");
  });
});
