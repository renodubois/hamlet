import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { describe, expect, test } from "vitest";
import { expectNoA11yViolations } from "../test/a11y";
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

describe("<MessageInput>", () => {
  test("renders an accessible text field, placeholder, and emoji picker button", async () => {
    const { container } = renderHarness();

    expect(screen.getByLabelText(/compose message/i)).toBeInTheDocument();
    expect(screen.getByPlaceholderText(/send a new message/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /open emoji picker/i })).toHaveAttribute(
      "aria-haspopup",
      "dialog",
    );
    await expectNoA11yViolations(container, "message input");
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
