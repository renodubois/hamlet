import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
import { createSignal } from "solid-js";
import { describe, expect, test } from "vitest";
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
