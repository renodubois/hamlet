import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
import { createSignal } from "solid-js";
import { describe, expect, test, vi } from "vitest";
import { AuthProvider } from "../contexts/auth";
import { CustomEmojisProvider } from "../contexts/custom-emojis";
import { expectNoA11yViolations } from "../test/a11y";
import { DEV_USER } from "../test/msw/handlers";
import { resetMswState } from "../test/msw/server";
import MessageInput from "./message-input";

const searchName = /search and select emoji/i;
const pickerName = /emoji picker/i;

function renderHarness(initialValue = "") {
  const changes: string[] = [];
  let setExternalValue: (value: string) => void = () => {};

  const result = render(() => {
    const [value, setValue] = createSignal(initialValue);
    setExternalValue = (nextValue: string) => setValue(nextValue);

    return (
      <MessageInput
        value={value()}
        onChange={(nextValue) => {
          changes.push(nextValue);
          setValue(nextValue);
        }}
        ariaLabel="Compose message"
        placeholder="Send a new message..."
      />
    );
  });

  return { ...result, changes, setExternalValue };
}

function renderHarnessWithCustomEmojis(initialValue = "") {
  const changes: string[] = [];
  resetMswState({
    me: DEV_USER,
    customEmojis: [
      {
        id: 123,
        name: "party",
        image_url: "/uploads/emojis/123.webp?v=1",
        animated: false,
        created_by_user_id: 1,
        created_at: 1,
        updated_at: 1,
        deleted_at: null,
      },
      {
        id: 456,
        name: "dance",
        image_url: "/uploads/emojis/456.gif?v=1",
        animated: true,
        created_by_user_id: 1,
        created_at: 1,
        updated_at: 1,
        deleted_at: null,
      },
      {
        id: 789,
        name: "retired",
        image_url: "/uploads/emojis/789.webp?v=1",
        animated: false,
        created_by_user_id: 1,
        created_at: 1,
        updated_at: 1,
        deleted_at: 2,
      },
    ],
  });

  const result = render(() => {
    const [value, setValue] = createSignal(initialValue);

    return (
      <AuthProvider>
        <CustomEmojisProvider>
          <MessageInput
            value={value()}
            onChange={(nextValue) => {
              changes.push(nextValue);
              setValue(nextValue);
            }}
            ariaLabel="Compose message"
            placeholder="Send a new message..."
          />
        </CustomEmojisProvider>
      </AuthProvider>
    );
  });

  return { ...result, changes };
}

async function openPicker() {
  fireEvent.click(screen.getByRole("button", { name: /open emoji picker/i }));
  return screen.findByRole("dialog", { name: pickerName });
}

async function selectEmoji(searchQuery: string, emojiName: RegExp) {
  const dialog = await openPicker();
  fireEvent.input(within(dialog).getByRole("combobox", { name: searchName }), {
    target: { value: searchQuery },
  });
  const cell = within(dialog).getByRole("gridcell", { name: emojiName });
  fireEvent.click(within(cell).getByRole("button", { name: emojiName }));
}

function setInputSelection(input: HTMLInputElement, start: number, end = start) {
  input.focus();
  input.setSelectionRange(start, end);
  fireEvent.select(input);
}

function inputFromUser(
  input: HTMLInputElement,
  value: string,
  caretIndex = value.length,
  eventInit?: InputEventInit,
) {
  input.value = value;
  input.setSelectionRange(caretIndex, caretIndex);
  fireEvent.input(input, eventInit);
}

function setDomSelection(container: Node, offset: number) {
  const range = document.createRange();
  range.setStart(container, offset);
  range.collapse(true);
  window.getSelection()?.removeAllRanges();
  window.getSelection()?.addRange(range);
}

function renderFormHarness(initialValue = "") {
  const changes: string[] = [];
  const onSubmit = vi.fn((event: SubmitEvent) => event.preventDefault());

  const result = render(() => {
    const [value, setValue] = createSignal(initialValue);

    return (
      <form onSubmit={onSubmit}>
        <MessageInput
          value={value()}
          onChange={(nextValue) => {
            changes.push(nextValue);
            setValue(nextValue);
          }}
          ariaLabel="Compose message"
          placeholder="Send a new message..."
        />
        <button type="submit">Send</button>
      </form>
    );
  });

  return { ...result, changes, onSubmit };
}

function keyDown(input: HTMLElement, init: KeyboardEventInit): KeyboardEvent {
  const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
  input.dispatchEvent(event);
  return event;
}

function composingEnter(input: HTMLElement): KeyboardEvent {
  const event = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
  Object.defineProperty(event, "isComposing", { value: true });
  input.dispatchEvent(event);
  return event;
}

describe("<MessageInput>", () => {
  test("renders an accessible multiline text field, placeholder, and emoji picker button", async () => {
    const { container } = renderHarness();

    const input = screen.getByRole("textbox", { name: /compose message/i });
    expect(input).toBeInTheDocument();
    expect(input).toHaveAttribute("aria-multiline", "true");
    expect(input).toHaveClass(
      "min-h-[2.75rem]",
      "max-h-40",
      "overflow-x-hidden",
      "overflow-y-auto",
      "whitespace-pre-wrap",
      "break-words",
    );
    expect(input.className).toContain("[overflow-wrap:anywhere]");
    expect(screen.getByPlaceholderText(/send a new message/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /open emoji picker/i })).toHaveAttribute(
      "aria-haspopup",
      "dialog",
    );
    await expectNoA11yViolations(container, "message input");
  });

  test("keeps the multiline composer and emoji trigger keyboard reachable with visible focus styles", async () => {
    const user = userEvent.setup();
    renderHarness();

    const input = screen.getByRole("textbox", { name: /compose message/i });
    const emojiButton = screen.getByRole("button", { name: /open emoji picker/i });

    expect(input).toHaveClass("focus:outline-none", "focus:ring-2", "focus:ring-blue-400");
    expect(emojiButton).toHaveClass("focus:outline-none", "focus:ring-2", "focus:ring-blue-400");

    await user.tab();
    expect(document.activeElement).toBe(input);

    await user.tab();
    expect(document.activeElement).toBe(emojiButton);

    await user.keyboard("{Enter}");
    const dialog = await screen.findByRole("dialog", { name: pickerName });
    expect(within(dialog).getByRole("combobox", { name: searchName })).toHaveFocus();

    fireEvent.keyDown(window, { key: "Escape" });

    await waitFor(() => expect(screen.queryByRole("dialog", { name: pickerName })).toBeNull());
  });

  test.each([
    ["start", "hello", 0, "\nhello", 1],
    ["middle", "hello", 2, "he\nllo", 3],
    ["end", "hello", 5, "hello\n", 6],
  ])(
    "Shift+Enter inserts exactly one newline at the %s caret",
    async (_, value, caret, nextValue, nextCaret) => {
      const { changes } = renderHarness(value);
      const input = screen.getByRole("textbox", { name: /compose message/i }) as HTMLInputElement;
      setInputSelection(input, caret);

      const event = keyDown(input, { key: "Enter", shiftKey: true });

      expect(event.defaultPrevented).toBe(true);
      await waitFor(() => {
        expect(input.value).toBe(nextValue);
        expect(input.selectionStart).toBe(nextCaret);
        expect(input.selectionEnd).toBe(nextCaret);
      });
      expect(changes.at(-1)).toBe(nextValue);
    },
  );

  test("Shift+Enter replaces selected text with one newline and restores the caret after it", async () => {
    const { changes } = renderHarness("hello world");
    const input = screen.getByRole("textbox", { name: /compose message/i }) as HTMLInputElement;
    setInputSelection(input, "hello".length, "hello world".length);

    const event = keyDown(input, { key: "Enter", shiftKey: true });

    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => {
      expect(input.value).toBe("hello\n");
      expect(input.selectionStart).toBe("hello\n".length);
      expect(input.selectionEnd).toBe("hello\n".length);
    });
    expect(changes.at(-1)).toBe("hello\n");
  });

  test("Shift+Enter replaces a serialized custom emoji chip selection with one newline", async () => {
    const { changes } = renderHarnessWithCustomEmojis("a <:party:123> b");
    const input = screen.getByRole("textbox", { name: /compose message/i }) as HTMLInputElement;
    await screen.findByRole("img", { name: /custom emoji :party:/i });
    setInputSelection(input, "a ".length, "a <:party:123>".length);

    const event = keyDown(input, { key: "Enter", shiftKey: true });

    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => {
      expect(input.value).toBe("a \n b");
      expect(input.selectionStart).toBe("a \n".length);
      expect(input.selectionEnd).toBe("a \n".length);
    });
    expect(changes.at(-1)).toBe("a \n b");
  });

  test("plain Enter submits the owning form without changing the editor text", async () => {
    const { changes, onSubmit } = renderFormHarness("ready to send");
    const input = screen.getByRole("textbox", { name: /compose message/i }) as HTMLInputElement;
    setInputSelection(input, "ready".length);

    const event = keyDown(input, { key: "Enter" });

    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    expect(input.value).toBe("ready to send");
    expect(input.querySelector("br, div")).toBeNull();
    expect(changes).toEqual([]);
  });

  test("Enter during IME composition does not submit or block composition", () => {
    const { onSubmit } = renderFormHarness("composing");
    const input = screen.getByRole("textbox", { name: /compose message/i }) as HTMLInputElement;

    const event = composingEnter(input);

    expect(event.defaultPrevented).toBe(false);
    expect(onSubmit).not.toHaveBeenCalled();
    expect(input.value).toBe("composing");
  });

  test.each([
    ["Ctrl+Enter", { ctrlKey: true }],
    ["Meta+Enter", { metaKey: true }],
    ["Alt+Enter", { altKey: true }],
    ["Ctrl+Shift+Enter", { ctrlKey: true, shiftKey: true }],
  ])("%s is ignored without submitting or editing", async (_, modifiers) => {
    const { changes, onSubmit } = renderFormHarness("unchanged");
    const input = screen.getByRole("textbox", { name: /compose message/i }) as HTMLInputElement;
    setInputSelection(input, 4);

    const event = keyDown(input, { key: "Enter", ...modifiers });

    expect(event.defaultPrevented).toBe(true);
    await Promise.resolve();
    expect(onSubmit).not.toHaveBeenCalled();
    expect(input.value).toBe("unchanged");
    expect(input.selectionStart).toBe(4);
    expect(changes).toEqual([]);
  });

  test("ignored non-Enter keys fall through to normal editing", () => {
    renderHarness("abc");
    const input = screen.getByRole("textbox", { name: /compose message/i }) as HTMLInputElement;
    setInputSelection(input, 1);

    const event = keyDown(input, { key: "a" });

    expect(event.defaultPrevented).toBe(false);
    expect(input.value).toBe("abc");
  });

  test("owner keyboard handlers can cancel editor handling", () => {
    const onCancel = vi.fn();
    render(() => (
      <MessageInput
        value="draft"
        onChange={vi.fn()}
        ariaLabel="Compose message"
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            onCancel();
          }
        }}
      />
    ));
    const input = screen.getByRole("textbox", { name: /compose message/i }) as HTMLInputElement;

    const event = keyDown(input, { key: "Escape" });

    expect(event.defaultPrevented).toBe(true);
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(input.value).toBe("draft");
  });

  test("converts completed emoji shortcodes before the caret", async () => {
    const { changes } = renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, ":grinning:");

    await waitFor(() => {
      expect(input.value).toBe("😀");
      expect(input.selectionStart).toBe("😀".length);
    });
    expect(changes).toContain("😀");
  });

  test("converts pasted shortcode chains at a mid-draft caret", async () => {
    renderHarness("hello world");
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;
    const rawValue = "hello :grinning::heart:world";
    const rawCaretIndex = "hello :grinning::heart:".length;

    inputFromUser(input, rawValue, rawCaretIndex, {
      data: ":grinning::heart:",
      inputType: "insertFromPaste",
    });

    await waitFor(() => {
      expect(input.value).toBe("hello 😀❤️world");
      expect(input.selectionStart).toBe("hello 😀❤️".length);
      expect(input.selectionEnd).toBe("hello 😀❤️".length);
    });
  });

  test.each([
    ["after a newline", "first\n:grinning:", "first\n:grinning:".length, "first\n😀"],
    ["before a newline", "first :heart:\nsecond", "first :heart:".length, "first ❤️\nsecond"],
  ])("converts completed emoji shortcodes %s", async (_, rawValue, caretIndex, nextValue) => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;
    const expectedCaretIndex = nextValue.includes("\nsecond")
      ? nextValue.indexOf("\n")
      : nextValue.length;

    inputFromUser(input, rawValue, caretIndex, {
      data: rawValue,
      inputType: "insertFromPaste",
    });

    await waitFor(() => {
      expect(input.value).toBe(nextValue);
      expect(input.selectionStart).toBe(expectedCaretIndex);
    });
  });

  test("serializes browser-pasted line break elements as newline characters before shortcode replacement", async () => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;
    const firstLine = document.createTextNode("first :heart:");
    const lineBreak = document.createElement("br");
    const secondLine = document.createTextNode("second");
    input.replaceChildren(firstLine, lineBreak, secondLine);
    setDomSelection(firstLine, firstLine.textContent?.length ?? 0);

    fireEvent.input(input, { inputType: "insertFromPaste" });

    await waitFor(() => {
      expect(input.value).toBe("first ❤️\nsecond");
      expect(input.selectionStart).toBe("first ❤️".length);
    });
  });

  test("serializes browser-pasted line break elements as newline characters after shortcode boundaries", async () => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;
    const firstLine = document.createTextNode("first");
    const lineBreak = document.createElement("br");
    const secondLine = document.createTextNode(":grinning:");
    input.replaceChildren(firstLine, lineBreak, secondLine);
    setDomSelection(secondLine, secondLine.textContent?.length ?? 0);

    fireEvent.input(input, { inputType: "insertFromPaste" });

    await waitFor(() => {
      expect(input.value).toBe("first\n😀");
      expect(input.selectionStart).toBe("first\n😀".length);
    });
  });

  test("converts shortcodes after opening punctuation and inserted emoji boundaries", async () => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, "(:heart:");

    await waitFor(() => {
      expect(input.value).toBe("(❤️");
      expect(input.selectionStart).toBe("(❤️".length);
    });

    inputFromUser(input, "(❤️:grinning:");

    await waitFor(() => {
      expect(input.value).toBe("(❤️😀");
      expect(input.selectionStart).toBe("(❤️😀".length);
    });
  });

  test("leaves word-attached shortcode-looking text literal", async () => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, "abc:grinning:");

    await waitFor(() => {
      expect(input.value).toBe("abc:grinning:");
      expect(input.selectionStart).toBe("abc:grinning:".length);
    });
  });

  test("inserts a selected emoji at the current caret", async () => {
    renderHarness("hello world");
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;
    setInputSelection(input, "hello ".length);

    await selectEmoji(":smile:", /emoji :smile:/i);

    await waitFor(() => {
      expect(input.value).toBe("hello 😄world");
      expect(input.selectionStart).toBe("hello 😄".length);
      expect(document.activeElement).toBe(input);
    });
    expect(screen.queryByRole("dialog", { name: pickerName })).toBeNull();
  });

  test("inserts a selected emoji at a caret after a newline", async () => {
    renderHarness("hello\nworld");
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;
    setInputSelection(input, "hello\n".length);

    await selectEmoji("heart", /emoji :heart:/i);

    await waitFor(() => {
      expect(input.value).toBe("hello\n❤️world");
      expect(input.selectionStart).toBe("hello\n❤️".length);
      expect(document.activeElement).toBe(input);
    });
  });

  test("replaces selected text spanning a newline with a selected emoji", async () => {
    renderHarness("hello\nworld");
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;
    setInputSelection(input, "hello".length, "hello\nwor".length);

    await selectEmoji(":smile:", /emoji :smile:/i);

    await waitFor(() => {
      expect(input.value).toBe("hello😄ld");
      expect(input.selectionStart).toBe("hello😄".length);
      expect(document.activeElement).toBe(input);
    });
  });

  test("renders controlled custom emoji markers as accessible image-only chips", async () => {
    renderHarnessWithCustomEmojis("hello <:party:123>");

    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;
    expect(input.value).toBe("hello <:party:123>");
    await screen.findByRole("img", { name: /custom emoji :party:/i });
    let chip: HTMLElement | null = null;
    await waitFor(() => {
      chip = screen.getByRole("img", { name: /custom emoji :party:/i });
      const image = chip.querySelector("img");
      expect(image).not.toBeNull();
      expect(image?.getAttribute("src")).toContain("/uploads/emojis/123.webp?v=1");
    });
    expect(chip).toHaveTextContent("");
    expect(screen.queryByText(":party:")).toBeNull();
    expect(screen.queryByText("<:party:123>")).toBeNull();
  });

  test("renders custom emoji chips in multiline drafts while serialized markers stay stable", async () => {
    renderHarnessWithCustomEmojis("hello\n<:party:123>\n<a:dance:456>");

    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;
    expect(input.value).toBe("hello\n<:party:123>\n<a:dance:456>");
    expect(input.value).not.toContain("\u200B");
    await screen.findByRole("img", { name: /custom emoji :party:/i });
    await screen.findByRole("img", { name: /custom emoji :dance:/i });
    expect(screen.queryByText("<:party:123>")).toBeNull();
    expect(screen.queryByText("<a:dance:456>")).toBeNull();
  });

  test("keeps a custom-emoji-only draft editable from the caret boundaries", async () => {
    const { changes } = renderHarnessWithCustomEmojis("<:party:123>");

    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;
    await screen.findByRole("img", { name: /custom emoji :party:/i });

    expect(input.value).toBe("<:party:123>");
    expect(input.firstChild).toBe(screen.getByRole("img", { name: /custom emoji :party:/i }));
    expect(input.lastChild?.textContent).toBe("\u200B");

    setInputSelection(input, 0);
    fireEvent.keyDown(input, { key: "ArrowRight" });
    await waitFor(() => {
      expect(input.selectionStart).toBe(input.value.length);
    });

    fireEvent.keyDown(input, { key: "ArrowLeft" });
    await waitFor(() => {
      expect(input.selectionStart).toBe(0);
    });

    setInputSelection(input, input.value.length);
    fireEvent.keyDown(input, { key: "Backspace" });

    await waitFor(() => {
      expect(input.value).toBe("");
    });
    expect(changes).toContain("");
  });

  test("navigates across custom emoji markers adjacent to newlines as whole markers", async () => {
    const marker = "<:party:123>";
    renderHarnessWithCustomEmojis(`one\n${marker}\ntwo`);
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;
    await screen.findByRole("img", { name: /custom emoji :party:/i });

    setInputSelection(input, "one\n".length);
    fireEvent.keyDown(input, { key: "ArrowRight" });
    await waitFor(() => {
      expect(input.selectionStart).toBe(`one\n${marker}`.length);
    });

    fireEvent.keyDown(input, { key: "ArrowLeft" });
    await waitFor(() => {
      expect(input.selectionStart).toBe("one\n".length);
    });
  });

  test("deletes custom emoji markers adjacent to newlines as whole markers", async () => {
    const marker = "<:party:123>";
    const { changes } = renderHarnessWithCustomEmojis(`one\n${marker}\ntwo`);
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;
    await screen.findByRole("img", { name: /custom emoji :party:/i });

    setInputSelection(input, `one\n${marker}`.length);
    fireEvent.keyDown(input, { key: "Backspace" });
    await waitFor(() => {
      expect(input.value).toBe("one\n\ntwo");
    });
    expect(changes).toContain("one\n\ntwo");
  });

  test("forward-deletes custom emoji markers adjacent to newlines as whole markers", async () => {
    const marker = "<:party:123>";
    const { changes } = renderHarnessWithCustomEmojis(`one\n${marker}\ntwo`);
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;
    await screen.findByRole("img", { name: /custom emoji :party:/i });

    setInputSelection(input, "one\n".length);
    fireEvent.keyDown(input, { key: "Delete" });
    await waitFor(() => {
      expect(input.value).toBe("one\n\ntwo");
    });
    expect(changes).toContain("one\n\ntwo");
  });

  test("keeps the composer stable while typing a custom emoji shortcode prefix", async () => {
    const user = userEvent.setup();
    const { changes } = renderHarnessWithCustomEmojis();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    await user.click(input);
    await user.keyboard(":taco");

    await waitFor(() => {
      expect(input.value).toBe(":taco");
      expect(input.selectionStart).toBe(":taco".length);
    });
    expect(changes).toContain(":t");
    expect(changes.at(-1)).toBe(":taco");
  });

  test("inserts active custom emoji markers from autocomplete and picker", async () => {
    const { changes } = renderHarnessWithCustomEmojis("hello world");
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    const dialog = await openPicker();
    fireEvent.input(within(dialog).getByRole("combobox", { name: searchName }), {
      target: { value: "party" },
    });
    await waitFor(() =>
      expect(within(dialog).getByRole("gridcell", { name: /emoji :party:/i })).toBeInTheDocument(),
    );
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: pickerName })).toBeNull());

    inputFromUser(input, "hello :party:");
    await waitFor(() => {
      expect(input.value).toBe("hello <:party:123>");
      expect(screen.getByRole("img", { name: /custom emoji :party:/i })).toBeInTheDocument();
    });
    expect(screen.queryByText("<:party:123>")).toBeNull();
    expect(changes).toContain("hello <:party:123>");

    inputFromUser(input, "hello :retired:");
    await waitFor(() => {
      expect(input.value).toBe("hello :retired:");
    });

    inputFromUser(input, "hello :dance:");
    await waitFor(() => {
      expect(input.value).toBe("hello <a:dance:456>");
    });
    expect(changes).toContain("hello <a:dance:456>");

    setInputSelection(input, "hello ".length);
    await selectEmoji("party", /emoji :party:/i);

    await waitFor(() => {
      expect(input.value).toBe("hello <:party:123><a:dance:456>");
    });
  });

  test("replaces the selected text with a selected emoji", async () => {
    renderHarness("hello world");
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;
    setInputSelection(input, "hello ".length, "hello world".length);

    await selectEmoji("heart", /emoji :heart:/i);

    await waitFor(() => {
      expect(input.value).toBe("hello ❤️");
      expect(input.selectionStart).toBe("hello ❤️".length);
      expect(document.activeElement).toBe(input);
    });
  });

  test("closes the picker when the controlled value is reset after submit", async () => {
    const { setExternalValue } = renderHarness("ready to send");
    await openPicker();

    setExternalValue("");

    await waitFor(() => {
      expect(screen.queryByRole("dialog", { name: pickerName })).toBeNull();
    });
  });
});
