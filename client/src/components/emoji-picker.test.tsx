import { useRef, useState } from "react";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import type { EmojiEntry } from "../emoji/emoji-data";
import { expectNoA11yViolations } from "../test/a11y";
import { renderNative } from "../test/render";
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

function renderHarness(
  initialOpen = true,
  onSelect = vi.fn(),
  onClose = vi.fn(),
  emojis: readonly EmojiEntry[] = EMOJIS,
) {
  function Harness() {
    const [open, setOpen] = useState(initialOpen);
    const anchorRef = useRef<HTMLButtonElement | null>(null);
    const close = () => {
      onClose();
      setOpen(false);
    };

    return (
      <>
        <button ref={anchorRef} type="button">
          anchor
        </button>
        <EmojiPicker
          open={open}
          anchor={() => anchorRef.current}
          emojis={emojis}
          onSelect={onSelect}
          onClose={close}
        />
      </>
    );
  }

  return renderNative(<Harness />);
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

async function inputSearch(search: Element, value: string) {
  await act(async () => {
    fireEvent.input(search, { target: { value } });
  });
}

function keyDown(target: Element, init: KeyboardEventInit & { key: string }) {
  const event = new KeyboardEvent("keydown", {
    bubbles: true,
    cancelable: true,
    ...init,
  });
  fireEvent(target, event);
  return event;
}

function composingKeyDown(target: Element, key: string) {
  const event = new KeyboardEvent("keydown", {
    bubbles: true,
    cancelable: true,
    key,
  });
  Object.defineProperty(event, "isComposing", { value: true });
  fireEvent(target, event);
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

    await inputSearch(search, "heart");

    expectActiveEmoji(":heart:");
    expect(getFooter()).toHaveTextContent(":heart:");
  });

  test("filters by shortcode with colon and bare queries", async () => {
    renderHarness();

    const search = await screen.findByRole("combobox", { name: searchName });
    await inputSearch(search, ":thumbsup:");
    expect(getEmojiGridcell(/emoji :thumbsup:, :\+1:/i)).toBeInTheDocument();

    await inputSearch(search, "smile");
    expect(getEmojiGridcell(":smile:")).toBeInTheDocument();
    expect(screen.queryByRole("gridcell", { name: /emoji :thumbsup:/i })).toBeNull();
  });

  test("normalizes shortcode separators and case while ignoring removed keywords", async () => {
    renderHarness();

    const search = await screen.findByRole("combobox", { name: searchName });
    await inputSearch(search, "WHITE check-mark");
    expect(getEmojiGridcell(":white_check_mark:")).toBeInTheDocument();

    await inputSearch(search, "favorite");
    expect(screen.getByText(/no emojis found/i)).toBeInTheDocument();
    expect(screen.queryByRole("gridcell", { name: /emoji :heart:/i })).toBeNull();
  });

  test("footer renders all active emoji shortcodes", async () => {
    renderHarness();

    const search = await screen.findByRole("combobox", { name: searchName });
    await inputSearch(search, "thumb");

    const footer = getFooter();
    expect(footer).toHaveTextContent(":thumbsup:");
    expect(footer).toHaveTextContent(":+1:");
  });

  test("shows an empty state and hides the footer when no emojis match", async () => {
    renderHarness();

    const search = await screen.findByRole("combobox", { name: searchName });
    await inputSearch(search, "not-an-emoji");

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
    await inputSearch(search, "party");
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
    await inputSearch(search, "dance");
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

  test("filter reordering keeps native and custom keyboard identity distinct", async () => {
    const onSelect = vi.fn();
    const collidingEmojis: readonly EmojiEntry[] = [
      {
        emoji: "🥳",
        shortcodes: [":party:"],
        category: "Smileys & Emotion",
      },
      EMOJIS[9],
    ];
    renderHarness(true, onSelect, vi.fn(), collidingEmojis);

    const search = await screen.findByRole("combobox", { name: searchName });
    await inputSearch(search, "party");

    const cells = screen.getAllByRole("gridcell", { name: "Emoji :party:" });
    expect(cells).toHaveLength(2);
    expect(cells[0].id).not.toBe(cells[1].id);
    expect(cells[0]).toHaveAttribute("aria-selected", "true");
    expect(cells[0].querySelector("img")).toBeInTheDocument();

    keyDown(search, { key: "ArrowRight" });
    expect(cells[1]).toHaveAttribute("aria-selected", "true");
    keyDown(search, { key: "Enter" });

    expect(onSelect).toHaveBeenCalledWith("🥳");
  });

  test.each(["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"] as const)(
    "no-results %s is not prevented",
    async (key) => {
      const onClose = vi.fn();
      renderHarness(true, vi.fn(), onClose);

      const search = await screen.findByRole("combobox", { name: searchName });
      await inputSearch(search, "not-an-emoji");

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
    await inputSearch(search, "not-an-emoji");
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

  test("an unrelated parent rerender does not reinstall open-picker listeners", async () => {
    const addWindow = vi.spyOn(window, "addEventListener");
    const removeWindow = vi.spyOn(window, "removeEventListener");
    const addDocument = vi.spyOn(document, "addEventListener");
    const removeDocument = vi.spyOn(document, "removeEventListener");

    function Harness() {
      const [, rerender] = useState(0);
      const anchorRef = useRef<HTMLButtonElement | null>(null);
      return (
        <>
          <button ref={anchorRef} type="button">
            anchor
          </button>
          <button type="button" onClick={() => rerender((count) => count + 1)}>
            unrelated rerender
          </button>
          <EmojiPicker
            open
            anchor={() => anchorRef.current}
            emojis={EMOJIS}
            onSelect={vi.fn()}
            onClose={vi.fn()}
          />
        </>
      );
    }

    const view = renderNative(<Harness />);
    await screen.findByRole("dialog", { name: pickerName });
    const countCalls = () => ({
      keydownAdds: addWindow.mock.calls.filter(([type]) => type === "keydown").length,
      keydownRemoves: removeWindow.mock.calls.filter(([type]) => type === "keydown").length,
      mouseDownAdds: addDocument.mock.calls.filter(([type]) => type === "mousedown").length,
      mouseDownRemoves: removeDocument.mock.calls.filter(([type]) => type === "mousedown").length,
    });
    const beforeRerender = countCalls();

    fireEvent.click(screen.getByRole("button", { name: "unrelated rerender" }));

    expect(countCalls()).toEqual(beforeRerender);
    view.unmount();
    const afterUnmount = countCalls();
    expect(afterUnmount.keydownRemoves).toBe(afterUnmount.keydownAdds);
    expect(afterUnmount.mouseDownRemoves).toBe(afterUnmount.mouseDownAdds);

    addWindow.mockRestore();
    removeWindow.mockRestore();
    addDocument.mockRestore();
    removeDocument.mockRestore();
  });

  test("immediate close cancels queued open focus and positioning", async () => {
    function Harness() {
      const [open, setOpen] = useState(false);
      const anchorRef = useRef<HTMLButtonElement | null>(null);
      return (
        <>
          <button ref={anchorRef} type="button" onClick={() => setOpen(true)}>
            anchor
          </button>
          <EmojiPicker
            open={open}
            anchor={() => anchorRef.current}
            emojis={EMOJIS}
            onSelect={vi.fn()}
            onClose={() => setOpen(false)}
          />
        </>
      );
    }

    renderNative(<Harness />);
    const anchor = screen.getByRole("button", { name: "anchor" });
    anchor.focus();
    fireEvent.click(anchor);
    fireEvent.mouseDown(document.body);
    await Promise.resolve();

    expect(screen.queryByRole("dialog", { name: pickerName })).toBeNull();
    expect(document.activeElement).toBe(anchor);
  });

  test("restores focus after an open-state rerender and Escape close", async () => {
    function Harness() {
      const [open, setOpen] = useState(false);
      const [label, setLabel] = useState("anchor");
      const anchorRef = useRef<HTMLButtonElement | null>(null);
      return (
        <>
          <button ref={anchorRef} type="button" onClick={() => setOpen(true)}>
            {label}
          </button>
          <button type="button" onClick={() => setLabel("updated anchor")}>
            rerender
          </button>
          <EmojiPicker
            open={open}
            anchor={() => anchorRef.current}
            emojis={EMOJIS}
            onSelect={vi.fn()}
            onClose={() => setOpen(false)}
          />
        </>
      );
    }
    renderNative(<Harness />);
    const anchor = screen.getByRole("button", { name: "anchor" });
    anchor.focus();
    fireEvent.click(anchor);
    const search = await screen.findByRole("combobox", { name: searchName });
    await waitFor(() => expect(document.activeElement).toBe(search));
    fireEvent.click(screen.getByRole("button", { name: "rerender" }));
    search.focus();
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(document.activeElement).toBe(anchor));
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
