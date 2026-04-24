import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { describe, expect, test, vi } from "vitest";
import type { EmojiEntry } from "../emoji/emoji_data";
import { expectNoA11yViolations } from "../test/a11y";
import EmojiPicker from "./emoji_picker";

const EMOJIS: readonly EmojiEntry[] = [
  {
    emoji: "😄",
    name: "grinning face with smiling eyes",
    shortcodes: [":smile:"],
    keywords: ["happy", "laugh"],
    category: "Smileys & Emotion",
  },
  {
    emoji: "❤️",
    name: "red heart",
    shortcodes: [":heart:"],
    keywords: ["love", "favorite"],
    category: "Smileys & Emotion",
  },
  {
    emoji: "👍",
    name: "thumbs up",
    shortcodes: [":thumbsup:", ":+1:"],
    keywords: ["approve", "yes"],
    category: "People & Body",
  },
];

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

describe("<EmojiPicker>", () => {
  test("renders nothing when closed", () => {
    renderHarness(false);
    expect(screen.queryByRole("dialog", { name: /emoji picker/i })).toBeNull();
  });

  test("renders search and emoji results when open", async () => {
    renderHarness();

    expect(await screen.findByRole("dialog", { name: /emoji picker/i })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: /search emojis/i })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /grinning face with smiling eyes emoji/i }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /red heart emoji/i })).toBeInTheDocument();
  });

  test("focuses the search input when opened", async () => {
    renderHarness();

    const search = await screen.findByRole("textbox", { name: /search emojis/i });
    await waitFor(() => expect(document.activeElement).toBe(search));
  });

  test("filters results as the user searches", async () => {
    renderHarness();

    const search = await screen.findByRole("textbox", { name: /search emojis/i });
    fireEvent.input(search, { target: { value: "heart" } });

    expect(screen.getByRole("button", { name: /red heart emoji/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /thumbs up emoji/i })).toBeNull();
  });

  test("searches shortcodes with and without colons", async () => {
    renderHarness();

    const search = await screen.findByRole("textbox", { name: /search emojis/i });
    fireEvent.input(search, { target: { value: ":thumbsup:" } });
    expect(screen.getByRole("button", { name: /thumbs up emoji/i })).toBeInTheDocument();

    fireEvent.input(search, { target: { value: "smile" } });
    expect(
      screen.getByRole("button", { name: /grinning face with smiling eyes emoji/i }),
    ).toBeInTheDocument();
  });

  test("shows an empty state when no emojis match", async () => {
    renderHarness();

    const search = await screen.findByRole("textbox", { name: /search emojis/i });
    fireEvent.input(search, { target: { value: "not-an-emoji" } });

    expect(screen.getByText(/no emojis found/i)).toBeInTheDocument();
  });

  test("selecting an emoji calls onSelect and closes", async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();
    renderHarness(true, onSelect, onClose);

    fireEvent.click(await screen.findByRole("button", { name: /red heart emoji/i }));

    expect(onSelect).toHaveBeenCalledWith("❤️");
    expect(onClose).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: /emoji picker/i })).toBeNull());
  });

  test("Escape closes the picker", async () => {
    const onClose = vi.fn();
    renderHarness(true, vi.fn(), onClose);

    await screen.findByRole("dialog", { name: /emoji picker/i });
    fireEvent.keyDown(window, { key: "Escape" });

    expect(onClose).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: /emoji picker/i })).toBeNull());
  });

  test("outside mouse down closes the picker", async () => {
    const onClose = vi.fn();
    renderHarness(true, vi.fn(), onClose);

    await screen.findByRole("dialog", { name: /emoji picker/i });
    fireEvent.mouseDown(document.body);

    expect(onClose).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: /emoji picker/i })).toBeNull());
  });

  test("has no accessibility violations", async () => {
    renderHarness();
    await screen.findByRole("dialog", { name: /emoji picker/i });

    await expectNoA11yViolations(document.body, "emoji picker");
  });
});
