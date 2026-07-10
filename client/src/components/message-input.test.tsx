import { act, fireEvent, render, screen, waitFor, within } from "../test/testing-library";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { useSignalState } from "../hooks/react-state";
import { describe, expect, test, vi } from "vitest";
import type { Channel, PublicUser, SearchUsersOptions } from "../api";
import { AuthProvider } from "../contexts/auth";
import { CustomEmojisProvider } from "../contexts/custom-emojis";
import { expectNoA11yViolations } from "../test/a11y";
import { DEV_USER, type HandlerState } from "../test/msw/handlers";
import { resetMswState, server } from "../test/msw/server";
import MessageInput, { type MessageInputProps } from "./message-input";

const searchName = /search and select emoji/i;
const pickerName = /emoji picker/i;
const TEST_SERVER = import.meta.env.VITE_HAMLET_DEFAULT_SERVER_URL ?? "http://127.0.0.1:3030";

const CUSTOM_EMOJI_FIXTURES: HandlerState["customEmojis"] = [
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
];

const MENTION_USER_FIXTURES: PublicUser[] = [
  {
    id: 2,
    username: "bob",
    display_name: "Bobby",
    avatar_url: "/uploads/avatars/bob.webp?v=1",
  },
  {
    id: 3,
    username: "alice",
    display_name: null,
    avatar_url: null,
  },
  {
    id: 4,
    username: "carol",
    display_name: "Carol",
    avatar_url: null,
  },
];

const CHANNEL_FIXTURES: Channel[] = [
  { id: 100, name: "general", position: 0, type: "text" },
  { id: 200, name: "random", position: 1, type: "text" },
  { id: 300, name: "voice", position: 2, type: "voice" },
  { id: 400, name: "project-planning", position: 3, type: "text" },
];

type MentionSearch = (options: SearchUsersOptions) => Promise<PublicUser[]>;

function mergeUsers(current: readonly PublicUser[], discovered: readonly PublicUser[]) {
  const byId = new Map(current.map((user) => [user.id, user]));
  for (const user of discovered) byId.set(user.id, user);
  return Array.from(byId.values());
}

function deferred<T = void>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function renderMentionHarness(
  initialValue = "",
  options: {
    searchMentionUsers?: MentionSearch;
    initialMentionUsers?: readonly PublicUser[];
    onKeyDown?: MessageInputProps["onKeyDown"];
  } = {},
) {
  const changes: string[] = [];
  const discoveredUsers: PublicUser[][] = [];
  const searchMentionUsers = options.searchMentionUsers ?? vi.fn(async () => MENTION_USER_FIXTURES);

  const result = render(() => {
    const [value, setValue] = useSignalState(initialValue);
    const [mentionUsers, setMentionUsers] = useSignalState<readonly PublicUser[]>(
      options.initialMentionUsers ?? [],
    );

    return (
      <MessageInput
        value={value()}
        onChange={(nextValue) => {
          changes.push(nextValue);
          setValue(nextValue);
        }}
        ariaLabel="Compose message"
        placeholder="Send a new message..."
        mentionUsers={mentionUsers()}
        onMentionUsers={(users) => {
          discoveredUsers.push([...users]);
          setMentionUsers((current) => mergeUsers(current, users));
        }}
        searchMentionUsers={searchMentionUsers}
        onKeyDown={options.onKeyDown}
      />
    );
  });

  return { ...result, changes, discoveredUsers, searchMentionUsers };
}

function renderChannelHarness(
  initialValue = "",
  options: {
    channels?: readonly Channel[];
    onKeyDown?: MessageInputProps["onKeyDown"];
  } = {},
) {
  const changes: string[] = [];
  const result = render(() => {
    const [value, setValue] = useSignalState(initialValue);

    return (
      <MessageInput
        value={value()}
        onChange={(nextValue) => {
          changes.push(nextValue);
          setValue(nextValue);
        }}
        ariaLabel="Compose message"
        placeholder="Send a new message..."
        channels={options.channels ?? CHANNEL_FIXTURES}
        onKeyDown={options.onKeyDown}
      />
    );
  });

  return { ...result, changes };
}

function renderChannelFormHarness(initialValue = "") {
  const changes: string[] = [];
  const onSubmit = vi.fn((event: any) => event.preventDefault());

  const result = render(() => {
    const [value, setValue] = useSignalState(initialValue);

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
          channels={CHANNEL_FIXTURES}
        />
        <button type="submit">Send</button>
      </form>
    );
  });

  return { ...result, changes, onSubmit };
}

function renderMentionFormHarness(initialValue = "", searchMentionUsers?: MentionSearch) {
  const changes: string[] = [];
  const onSubmit = vi.fn((event: any) => event.preventDefault());
  const search = searchMentionUsers ?? vi.fn(async () => MENTION_USER_FIXTURES);

  const result = render(() => {
    const [value, setValue] = useSignalState(initialValue);
    const [mentionUsers, setMentionUsers] = useSignalState<readonly PublicUser[]>([]);

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
          mentionUsers={mentionUsers()}
          onMentionUsers={(users) => setMentionUsers((current) => mergeUsers(current, users))}
          searchMentionUsers={search}
        />
        <button type="submit">Send</button>
      </form>
    );
  });

  return { ...result, changes, onSubmit, searchMentionUsers: search };
}

function renderHarness(initialValue = "") {
  const changes: string[] = [];
  let setExternalValue: (value: string) => void = () => {};

  const result = render(() => {
    const [value, setValue] = useSignalState(initialValue);
    setExternalValue = (nextValue: string) => act(() => setValue(nextValue));

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

function renderHarnessWithCustomEmojis(
  initialValue = "",
  customEmojis: HandlerState["customEmojis"] = CUSTOM_EMOJI_FIXTURES,
) {
  const changes: string[] = [];
  resetMswState({
    me: DEV_USER,
    customEmojis,
  });

  const result = render(() => {
    const [value, setValue] = useSignalState(initialValue);

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
  act(() => {
    input.focus();
    input.setSelectionRange(start, end);
    fireEvent.select(input);
  });
}

function inputFromUser(
  input: HTMLInputElement,
  value: string,
  caretIndex = value.length,
  eventInit?: InputEventInit,
) {
  act(() => {
    input.value = value;
    input.setSelectionRange(caretIndex, caretIndex);
    fireEvent.input(input, eventInit);
  });
}

async function expectEmojiAutocompleteOpen() {
  const listbox = await screen.findByRole("listbox", { name: /emoji suggestions/i });
  return within(listbox).getAllByRole("option");
}

async function expectEmojiAutocompleteClosed() {
  await waitFor(() => {
    expect(screen.queryByRole("listbox", { name: /emoji suggestions/i })).toBeNull();
  });
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
  const onSubmit = vi.fn((event: any) => event.preventDefault());

  const result = render(() => {
    const [value, setValue] = useSignalState(initialValue);

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

function keyDown(input: HTMLElement, init: KeyboardEventInit): any {
  const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
  fireEvent(input, event);
  return event;
}

function composingEnter(input: HTMLElement): any {
  const event = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
  Object.defineProperty(event, "isComposing", { value: true });
  fireEvent(input, event);
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

  test("shows mention autocomplete for a boundary-valid empty @ token", async () => {
    const { container, searchMentionUsers, discoveredUsers } = renderMentionHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, "@");

    const listbox = await screen.findByRole("listbox", { name: /mention suggestions/i });
    const options = within(listbox).getAllByRole("option");
    expect(searchMentionUsers).toHaveBeenLastCalledWith({ query: "", limit: 8 });
    expect(discoveredUsers.at(-1)?.map((user) => user.username)).toEqual(["alice", "bob", "carol"]);
    expect(options).toHaveLength(3);
    expect(options[0]).toHaveAttribute("aria-selected", "true");
    expect(options[0]).toHaveAccessibleName(/mention @alice/i);
    const bobOption = within(listbox).getByRole("option", { name: /mention bobby @bob/i });
    expect(within(bobOption).getByRole("img", { name: /bobby's avatar/i })).toBeInTheDocument();
    expect(within(bobOption).getByText("Bobby")).toHaveClass("font-semibold");
    expect(within(bobOption).getByText("@bob")).toHaveClass("text-gray-500");
    expect(input).toHaveAttribute("aria-autocomplete", "list");
    expect(input).toHaveAttribute("aria-controls", listbox.id);
    expect(input).toHaveAttribute("aria-activedescendant", options[0].id);
    await expectNoA11yViolations(container, "message input mention autocomplete");
  });

  test("hides mention autocomplete while text is selected", async () => {
    renderMentionHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, "@bo");
    await screen.findByRole("listbox", { name: /mention suggestions/i });

    setInputSelection(input, 0, "@bo".length);
    await waitFor(() => {
      expect(screen.queryByRole("listbox", { name: /mention suggestions/i })).toBeNull();
    });
  });

  test("ignores failed mention searches quietly", async () => {
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const searchMentionUsers = vi.fn<MentionSearch>(async () => {
      throw new Error("network down");
    });
    renderMentionHarness("", { searchMentionUsers });
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, "@zz");

    await waitFor(() => expect(searchMentionUsers).toHaveBeenCalledWith({ query: "zz", limit: 8 }));
    await waitFor(() => {
      expect(screen.queryByRole("listbox", { name: /mention suggestions/i })).toBeNull();
    });
    expect(errorSpy).not.toHaveBeenCalled();
    errorSpy.mockRestore();
  });

  test("ignores stale mention search responses", async () => {
    const staleSearch = deferred<PublicUser[]>();
    const searchMentionUsers = vi.fn<MentionSearch>((options) => {
      if (options.query === "a") return staleSearch.promise;
      return Promise.resolve([MENTION_USER_FIXTURES[2]]);
    });
    renderMentionHarness("", { searchMentionUsers });
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, "@a");
    await waitFor(() => expect(searchMentionUsers).toHaveBeenCalledWith({ query: "a", limit: 8 }));

    inputFromUser(input, "@c");
    const carolOption = await screen.findByRole("option", { name: /mention carol @carol/i });
    expect(carolOption).toHaveAttribute("aria-selected", "true");

    staleSearch.resolve([MENTION_USER_FIXTURES[1]]);
    await Promise.resolve();

    expect(screen.queryByRole("option", { name: /mention @alice/i })).toBeNull();
    expect(screen.getByRole("option", { name: /mention carol @carol/i })).toBeInTheDocument();
  });

  test("Enter commits the selected mention as a durable chip before form submit", async () => {
    const { changes, onSubmit } = renderMentionFormHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, "hello @bo world", "hello @bo".length);
    await screen.findByRole("listbox", { name: /mention suggestions/i });

    const commitEvent = keyDown(input, { key: "Enter" });

    expect(commitEvent.defaultPrevented).toBe(true);
    await waitFor(() => {
      expect(input.value).toBe("hello <@2> world");
      expect(input.selectionStart).toBe("hello <@2> ".length);
      expect(screen.getByText("@Bobby")).toHaveClass("bg-blue-100", "text-blue-800");
    });
    expect(screen.queryByText("<@2>")).toBeNull();
    expect(onSubmit).not.toHaveBeenCalled();
    expect(changes.at(-1)).toBe("hello <@2> world");

    const submitEvent = keyDown(input, { key: "Enter" });

    expect(submitEvent.defaultPrevented).toBe(true);
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
  });

  test("Tab and mouse commit mention suggestions while arrow keys wrap selection", async () => {
    const { changes } = renderMentionFormHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, "@");
    const listbox = await screen.findByRole("listbox", { name: /mention suggestions/i });
    const options = within(listbox).getAllByRole("option");

    let event = keyDown(input, { key: "ArrowUp" });
    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => expect(options.at(-1)).toHaveAttribute("aria-selected", "true"));

    event = keyDown(input, { key: "ArrowDown" });
    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => expect(options[0]).toHaveAttribute("aria-selected", "true"));

    event = keyDown(input, { key: "Tab" });
    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => expect(input.value).toBe("<@3> "));
    expect(changes.at(-1)).toBe("<@3> ");

    inputFromUser(input, "@bo");
    const bob = await screen.findByRole("option", { name: /mention bobby @bob/i });
    const mouseDown = new MouseEvent("mousedown", { bubbles: true, cancelable: true });
    fireEvent(bob, mouseDown);
    expect(mouseDown.defaultPrevented).toBe(true);
    fireEvent.click(bob);

    await waitFor(() => {
      expect(input.value).toBe("<@2> ");
      expect(document.activeElement).toBe(input);
    });
  });

  test("Escape dismisses mention autocomplete before owner keyboard handlers", async () => {
    const ownerKeyDown = vi.fn((event: any) => {
      if (event.key === "Escape") event.preventDefault();
    });
    renderMentionHarness("", { onKeyDown: ownerKeyDown });
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, "@bo");
    await screen.findByRole("listbox", { name: /mention suggestions/i });

    const event = keyDown(input, { key: "Escape" });

    expect(event.defaultPrevented).toBe(true);
    expect(ownerKeyDown).not.toHaveBeenCalled();
    await waitFor(() => {
      expect(screen.queryByRole("listbox", { name: /mention suggestions/i })).toBeNull();
    });
  });

  test("mention autocomplete, emoji autocomplete, and emoji picker are mutually exclusive", async () => {
    renderMentionHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    await openPicker();
    inputFromUser(input, "@bo");

    await screen.findByRole("listbox", { name: /mention suggestions/i });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: pickerName })).toBeNull());

    inputFromUser(input, ":sm");
    await expectEmojiAutocompleteOpen();
    expect(screen.queryByRole("listbox", { name: /mention suggestions/i })).toBeNull();

    inputFromUser(input, "@bo");
    await screen.findByRole("listbox", { name: /mention suggestions/i });
    fireEvent.click(screen.getByRole("button", { name: /open emoji picker/i }));

    await screen.findByRole("dialog", { name: pickerName });
    expect(screen.queryByRole("listbox", { name: /mention suggestions/i })).toBeNull();
  });

  test("renders known mention markers as chips, leaves unknown markers readable, and preserves markers when editing", async () => {
    const { changes } = renderMentionHarness("hello <@2> missing <@999>", {
      initialMentionUsers: [MENTION_USER_FIXTURES[0]],
    });
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    expect(input.value).toBe("hello <@2> missing <@999>");
    await waitFor(() => {
      expect(within(input).getByText("@Bobby")).toHaveClass("bg-blue-100", "text-blue-800");
    });
    expect(input.textContent).toContain("<@999>");
    expect(screen.queryByText("<@2>")).toBeNull();

    input.append(document.createTextNode("!"));
    setDomSelection(input, input.childNodes.length);
    fireEvent.input(input);

    await waitFor(() => {
      expect(input.value).toBe("hello <@2> missing <@999>!");
    });
    expect(changes.at(-1)).toBe("hello <@2> missing <@999>!");
  });

  test("shows channel autocomplete for a boundary-valid # token and suggests text channels only", async () => {
    const { container } = renderChannelHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, "see #");

    const listbox = await screen.findByRole("listbox", { name: /channel suggestions/i });
    const options = within(listbox).getAllByRole("option");
    expect(options.map((option) => option.getAttribute("aria-label"))).toEqual([
      "Channel #general",
      "Channel #random",
      "Channel #project-planning",
    ]);
    expect(screen.queryByRole("option", { name: /channel #voice/i })).toBeNull();
    expect(options[0]).toHaveAttribute("aria-selected", "true");
    expect(options[0]).toHaveAccessibleName(/channel #general/i);
    expect(input).toHaveAttribute("aria-autocomplete", "list");
    expect(input).toHaveAttribute("aria-controls", listbox.id);
    expect(input).toHaveAttribute("aria-activedescendant", options[0].id);
    await expectNoA11yViolations(container, "message input channel autocomplete");
  });

  test.each([
    ["selected text", "#general", 0, "#general".length],
    ["word-attached hash", "abc#general", "abc#general".length, "abc#general".length],
    [
      "URL-like text",
      "https://example.test/#general",
      "https://example.test/#general".length,
      "https://example.test/#general".length,
    ],
  ])(
    "does not show channel autocomplete for %s",
    async (_, value, selectionStart, selectionEnd) => {
      renderChannelHarness();
      const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

      inputFromUser(input, value, selectionEnd);
      setInputSelection(input, selectionStart, selectionEnd);

      await waitFor(() => {
        expect(screen.queryByRole("listbox", { name: /channel suggestions/i })).toBeNull();
      });
    },
  );

  test("Enter commits a selected channel before form submit and renders it as a chip", async () => {
    const { changes, onSubmit } = renderChannelFormHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, "hello #gen world", "hello #gen".length);
    await screen.findByRole("listbox", { name: /channel suggestions/i });

    const event = keyDown(input, { key: "Enter" });

    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => {
      expect(input.value).toBe("hello <#100> world");
      expect(input.selectionStart).toBe("hello <#100> ".length);
      expect(within(input).getByText("#general")).toHaveClass("bg-gray-200", "text-gray-800");
    });
    expect(screen.queryByText("<#100>")).toBeNull();
    expect(onSubmit).not.toHaveBeenCalled();
    expect(changes.at(-1)).toBe("hello <#100> world");
  });

  test("Tab, mouse, and arrow keys work for channel autocomplete", async () => {
    const { changes } = renderChannelHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, "#");
    const listbox = await screen.findByRole("listbox", { name: /channel suggestions/i });
    const options = within(listbox).getAllByRole("option");

    let event = keyDown(input, { key: "ArrowUp" });
    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => expect(options.at(-1)).toHaveAttribute("aria-selected", "true"));

    event = keyDown(input, { key: "ArrowDown" });
    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => expect(options[0]).toHaveAttribute("aria-selected", "true"));

    event = keyDown(input, { key: "Tab" });
    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => expect(input.value).toBe("<#100> "));
    expect(changes.at(-1)).toBe("<#100> ");

    inputFromUser(input, "#ran");
    const random = await screen.findByRole("option", { name: /channel #random/i });
    const mouseDown = new MouseEvent("mousedown", { bubbles: true, cancelable: true });
    fireEvent(random, mouseDown);
    expect(mouseDown.defaultPrevented).toBe(true);
    fireEvent.click(random);

    await waitFor(() => {
      expect(input.value).toBe("<#200> ");
      expect(document.activeElement).toBe(input);
    });
  });

  test("Escape dismisses channel autocomplete before owner keyboard handlers", async () => {
    const ownerKeyDown = vi.fn((event: any) => {
      if (event.key === "Escape") event.preventDefault();
    });
    renderChannelHarness("", { onKeyDown: ownerKeyDown });
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, "#gen");
    await screen.findByRole("listbox", { name: /channel suggestions/i });

    const event = keyDown(input, { key: "Escape" });

    expect(event.defaultPrevented).toBe(true);
    expect(ownerKeyDown).not.toHaveBeenCalled();
    await waitFor(() => {
      expect(screen.queryByRole("listbox", { name: /channel suggestions/i })).toBeNull();
    });
  });

  test("renders known channel markers as chips, leaves unknown markers readable, and edits safely", async () => {
    const { changes } = renderChannelHarness("hello <#100> missing <#999>");
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    expect(input.value).toBe("hello <#100> missing <#999>");
    await waitFor(() => {
      expect(within(input).getByText("#general")).toHaveClass("bg-gray-200", "text-gray-800");
    });
    expect(input.textContent).toContain("<#999>");
    expect(screen.queryByText("<#100>")).toBeNull();

    input.append(document.createTextNode("!"));
    setDomSelection(input, input.childNodes.length);
    fireEvent.input(input);

    await waitFor(() => {
      expect(input.value).toBe("hello <#100> missing <#999>!");
    });
    expect(changes.at(-1)).toBe("hello <#100> missing <#999>!");
  });

  test("navigates and deletes channel chips as whole markers", async () => {
    const marker = "<#100>";
    const { changes } = renderChannelHarness(`one\n${marker}\ntwo`);
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;
    await within(input).findByText("#general");

    setInputSelection(input, "one\n".length);
    fireEvent.keyDown(input, { key: "ArrowRight" });
    await waitFor(() => {
      expect(input.selectionStart).toBe(`one\n${marker}`.length);
    });

    fireEvent.keyDown(input, { key: "ArrowLeft" });
    await waitFor(() => {
      expect(input.selectionStart).toBe("one\n".length);
    });

    fireEvent.keyDown(input, { key: "Delete" });
    await waitFor(() => {
      expect(input.value).toBe("one\n\ntwo");
    });
    expect(changes).toContain("one\n\ntwo");
  });

  test("shows native emoji autocomplete suggestions for a boundary-valid prefix", async () => {
    const { container } = renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, ":sm");

    const listbox = await screen.findByRole("listbox", { name: /emoji suggestions/i });
    const options = within(listbox).getAllByRole("option");
    expect(options.length).toBeGreaterThan(0);
    expect(options.length).toBeLessThanOrEqual(8);
    expect(options[0]).toHaveAttribute("aria-selected", "true");
    expect(options[0]).toHaveAccessibleName(/:smiley:/i);
    expect(input).toHaveAttribute("aria-autocomplete", "list");
    expect(input).toHaveAttribute("aria-controls", listbox.id);
    expect(input).toHaveAttribute("aria-activedescendant", options[0].id);
    await expectNoA11yViolations(container, "message input emoji autocomplete");
  });

  test("renders prominent shortcode rows with fixed previews and muted matched aliases", async () => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, ":satisfied");

    const option = await screen.findByRole("option", {
      name: /emoji :laughing:, also matches :satisfied:/i,
    });
    expect(within(option).getByText(":laughing:")).toHaveClass("font-semibold");
    expect(within(option).getByText(/also matches :satisfied:/i)).toHaveClass("text-gray-500");
    expect(option.querySelector("[aria-hidden='true']")).toHaveClass("h-9", "w-9", "bg-gray-50");
  });

  test("shows active custom emoji autocomplete suggestions with image previews and excludes deleted emojis", async () => {
    const { container } = renderHarnessWithCustomEmojis();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, ":pa");

    const partyOption = await screen.findByRole("option", { name: /emoji :party:/i });
    const listbox = screen.getByRole("listbox", { name: /emoji suggestions/i });
    const image = partyOption.querySelector("img");
    expect(image?.getAttribute("src")).toContain("/uploads/emojis/123.webp?v=1");
    expect(within(partyOption).getByText(":party:")).toHaveClass("font-semibold");
    expect(partyOption).toHaveAttribute("aria-selected", "true");
    expect(input).toHaveAttribute("aria-autocomplete", "list");
    expect(input).toHaveAttribute("aria-controls", listbox.id);
    expect(input).toHaveAttribute("aria-activedescendant", partyOption.id);
    await expectNoA11yViolations(container, "message input custom emoji autocomplete");

    inputFromUser(input, ":ret");

    await waitFor(() => {
      expect(screen.queryByRole("option", { name: /emoji :retired:/i })).toBeNull();
      expect(screen.queryByRole("listbox", { name: /emoji suggestions/i })).toBeNull();
    });
  });

  test("commits custom emoji autocomplete suggestions as durable marker chips", async () => {
    const { changes } = renderHarnessWithCustomEmojis();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, "hello :pa world", "hello :pa".length);
    const partyOption = await screen.findByRole("option", { name: /emoji :party:/i });
    expect(partyOption).toHaveAttribute("aria-selected", "true");

    const event = keyDown(input, { key: "Enter" });

    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => {
      expect(input.value).toBe("hello <:party:123> world");
      expect(input.selectionStart).toBe("hello <:party:123>".length);
      expect(screen.getByRole("img", { name: /custom emoji :party:/i })).toBeInTheDocument();
    });
    expect(screen.queryByText("<:party:123>")).toBeNull();
    expect(changes.at(-1)).toBe("hello <:party:123> world");
  });

  test("prioritizes custom emoji over native emoji for autocomplete exact and prefix ties", async () => {
    renderHarnessWithCustomEmojis("", [
      ...CUSTOM_EMOJI_FIXTURES,
      {
        id: 321,
        name: "panda_face",
        image_url: "/uploads/emojis/321.webp?v=1",
        animated: false,
        created_by_user_id: 1,
        created_at: 1,
        updated_at: 1,
        deleted_at: null,
      },
    ]);
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, ":panda");

    await waitFor(() => {
      const options = within(
        screen.getByRole("listbox", { name: /emoji suggestions/i }),
      ).getAllByRole("option");
      expect(options[0]).toHaveAccessibleName(/emoji :panda_face:/i);
      expect(options[0].querySelector("img")?.getAttribute("src")).toContain(
        "/uploads/emojis/321.webp?v=1",
      );
    });
  });

  test("keeps native autocomplete available while custom emojis load and shows custom emojis after update", async () => {
    resetMswState({ me: DEV_USER, customEmojis: CUSTOM_EMOJI_FIXTURES });
    let resolveEmojis: () => void = () => {};
    const emojisLoaded = new Promise<void>((resolve) => {
      resolveEmojis = resolve;
    });
    server.use(
      http.get(`${TEST_SERVER}/emojis`, async () => {
        await emojisLoaded;
        return HttpResponse.json(CUSTOM_EMOJI_FIXTURES);
      }),
    );
    render(() => {
      const [value, setValue] = useSignalState("");

      return (
        <AuthProvider>
          <CustomEmojisProvider>
            <MessageInput
              value={value()}
              onChange={setValue}
              ariaLabel="Compose message"
              placeholder="Send a new message..."
            />
          </CustomEmojisProvider>
        </AuthProvider>
      );
    });
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, ":sm");
    expect(await screen.findByRole("option", { name: /emoji :smiley:/i })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /emoji :party:/i })).toBeNull();

    resolveEmojis();
    inputFromUser(input, ":pa");

    expect(await screen.findByRole("option", { name: /emoji :party:/i })).toBeInTheDocument();
  });

  test("hides emoji autocomplete when a boundary-valid query has zero matches", async () => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, ":qq");
    await Promise.resolve();

    expect(screen.queryByRole("listbox", { name: /emoji suggestions/i })).toBeNull();
    expect(screen.queryByRole("option")).toBeNull();
    expect(screen.queryByText(/no results|loading|error/i)).toBeNull();
  });

  test("opening the emoji picker closes the active emoji autocomplete menu", async () => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, ":sm");
    await expectEmojiAutocompleteOpen();

    fireEvent.click(screen.getByRole("button", { name: /open emoji picker/i }));

    await screen.findByRole("dialog", { name: pickerName });
    await expectEmojiAutocompleteClosed();
  });

  test("opening emoji autocomplete closes the emoji picker", async () => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    await openPicker();

    inputFromUser(input, ":sm");

    await expectEmojiAutocompleteOpen();
    await waitFor(() => {
      expect(screen.queryByRole("dialog", { name: pickerName })).toBeNull();
    });
  });

  test.each([
    ["after whitespace", "hello :sm", "hello :sm".length],
    ["after opening punctuation", "(:sm", "(:sm".length],
    ["after another emoji", "😄:sm", "😄:sm".length],
  ])("opens emoji autocomplete %s", async (_, value, caretIndex) => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, value, caretIndex);

    const options = await expectEmojiAutocompleteOpen();
    expect(options[0]).toHaveAccessibleName(/:smiley:/i);
  });

  test("opens emoji autocomplete after a custom emoji chip", async () => {
    renderHarnessWithCustomEmojis("<:party:123>:sm");
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;
    await screen.findByRole("img", { name: /custom emoji :party:/i });
    setInputSelection(input, "<:party:123>:sm".length);

    const options = await expectEmojiAutocompleteOpen();
    expect(options[0]).toHaveAccessibleName(/:smiley:/i);
  });

  test.each([
    ["selected text", ":sm", 0, ":sm".length],
    ["completed shortcode", ":smile:", ":sm".length, ":sm".length],
    ["word-attached colon", "abc:sm", "abc:sm".length, "abc:sm".length],
    [
      "URL-like text",
      "https://example.com/:sm",
      "https://example.com/:sm".length,
      "https://example.com/:sm".length,
    ],
    ["time-like text", "12:30", "12:30".length, "12:30".length],
    ["one-character prefix", ":s", ":s".length, ":s".length],
  ])("does not show emoji autocomplete for %s", async (_, value, selectionStart, selectionEnd) => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, value, selectionEnd);
    setInputSelection(input, selectionStart, selectionEnd);

    await expectEmojiAutocompleteClosed();
  });

  test("Escape dismisses only the current emoji autocomplete token session", async () => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, ":sm");
    await expectEmojiAutocompleteOpen();

    const dismissEvent = keyDown(input, { key: "Escape" });

    expect(dismissEvent.defaultPrevented).toBe(true);
    await expectEmojiAutocompleteClosed();

    inputFromUser(input, ":smi");
    await expectEmojiAutocompleteClosed();

    inputFromUser(input, ":smi :he");
    await expectEmojiAutocompleteOpen();
    expect(screen.getByRole("option", { name: /:heart:/i })).toBeInTheDocument();

    inputFromUser(input, "");
    await expectEmojiAutocompleteClosed();
    inputFromUser(input, ":sm");
    await expectEmojiAutocompleteOpen();
  });

  test("Enter commits the selected native emoji autocomplete suggestion without submitting", async () => {
    const { changes, onSubmit } = renderFormHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, "hello :sm world", "hello :sm".length);
    await screen.findByRole("listbox", { name: /emoji suggestions/i });

    const event = keyDown(input, { key: "Enter" });

    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => {
      expect(input.value).toBe("hello 😃 world");
      expect(input.selectionStart).toBe("hello 😃".length);
      expect(input.selectionEnd).toBe("hello 😃".length);
    });
    expect(onSubmit).not.toHaveBeenCalled();
    expect(changes.at(-1)).toBe("hello 😃 world");
    expect(screen.queryByRole("listbox", { name: /emoji suggestions/i })).toBeNull();
  });

  test("Tab commits the selected native emoji autocomplete suggestion without moving focus", async () => {
    const { changes, onSubmit } = renderFormHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, "hello :sm world", "hello :sm".length);
    await screen.findByRole("listbox", { name: /emoji suggestions/i });

    const event = keyDown(input, { key: "Tab" });

    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => {
      expect(input.value).toBe("hello 😃 world");
      expect(input.selectionStart).toBe("hello 😃".length);
      expect(document.activeElement).toBe(input);
    });
    expect(onSubmit).not.toHaveBeenCalled();
    expect(changes.at(-1)).toBe("hello 😃 world");
    expect(screen.queryByRole("listbox", { name: /emoji suggestions/i })).toBeNull();
  });

  test("Arrow keys wrap autocomplete selection and query changes reset it", async () => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, ":sm");
    const listbox = await screen.findByRole("listbox", { name: /emoji suggestions/i });
    let options = within(listbox).getAllByRole("option");
    expect(options[0]).toHaveAttribute("aria-selected", "true");

    let event = keyDown(input, { key: "ArrowDown" });
    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => expect(options[1]).toHaveAttribute("aria-selected", "true"));
    expect(input).toHaveAttribute("aria-activedescendant", options[1].id);

    event = keyDown(input, { key: "ArrowUp" });
    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => expect(options[0]).toHaveAttribute("aria-selected", "true"));

    event = keyDown(input, { key: "ArrowUp" });
    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => expect(options.at(-1)).toHaveAttribute("aria-selected", "true"));

    inputFromUser(input, ":he");
    await waitFor(() => {
      options = within(listbox).getAllByRole("option");
      expect(options[0]).toHaveAttribute("aria-selected", "true");
      expect(input).toHaveAttribute("aria-activedescendant", options[0].id);
    });
  });

  test("Enter submits normally after Escape dismisses autocomplete", async () => {
    const { changes, onSubmit } = renderFormHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, "hello :sm world", "hello :sm".length);
    await screen.findByRole("listbox", { name: /emoji suggestions/i });

    const escapeEvent = keyDown(input, { key: "Escape" });

    expect(escapeEvent.defaultPrevented).toBe(true);
    await waitFor(() =>
      expect(screen.queryByRole("listbox", { name: /emoji suggestions/i })).toBeNull(),
    );

    const enterEvent = keyDown(input, { key: "Enter" });

    expect(enterEvent.defaultPrevented).toBe(true);
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    expect(input.value).toBe("hello :sm world");
    expect(changes).toEqual(["hello :sm world"]);
  });

  test("Shift+Enter does not commit autocomplete suggestions", async () => {
    const { changes, onSubmit } = renderFormHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, "hello :sm world", "hello :sm".length);
    await screen.findByRole("listbox", { name: /emoji suggestions/i });

    const event = keyDown(input, { key: "Enter", shiftKey: true });

    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => {
      expect(input.value).toBe("hello :sm\n world");
      expect(input.selectionStart).toBe("hello :sm\n".length);
    });
    expect(onSubmit).not.toHaveBeenCalled();
    expect(changes.at(-1)).toBe("hello :sm\n world");
  });

  test("autocomplete shortcuts take priority over owner keyboard handlers while open", async () => {
    const ownerKeyDown = vi.fn((event: any) => {
      if (event.key === "Enter" || event.key === "Escape") event.preventDefault();
    });

    const changes: string[] = [];
    render(() => {
      const [value, setValue] = useSignalState(":sm");

      return (
        <MessageInput
          value={value()}
          onChange={(nextValue) => {
            changes.push(nextValue);
            setValue(nextValue);
          }}
          ariaLabel="Compose message"
          onKeyDown={ownerKeyDown}
        />
      );
    });
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;
    setInputSelection(input, ":sm".length);
    await screen.findByRole("listbox", { name: /emoji suggestions/i });

    const event = keyDown(input, { key: "Enter" });

    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => expect(input.value).toBe("😃"));
    expect(changes.at(-1)).toBe("😃");
    expect(ownerKeyDown).not.toHaveBeenCalled();
  });

  test("clicking a native emoji autocomplete suggestion replaces only the active token", async () => {
    const { changes } = renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    inputFromUser(input, "before :sm after", "before :sm".length);
    const option = await screen.findByRole("option", { name: /:smile:/i });

    const mouseDown = new MouseEvent("mousedown", { bubbles: true, cancelable: true });
    fireEvent(option, mouseDown);
    expect(mouseDown.defaultPrevented).toBe(true);
    fireEvent.click(option);

    await waitFor(() => {
      expect(input.value).toBe("before 😄 after");
      expect(input.selectionStart).toBe("before 😄".length);
      expect(input.selectionEnd).toBe("before 😄".length);
      expect(document.activeElement).toBe(input);
    });
    expect(changes.at(-1)).toBe("before 😄 after");
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

  test("keeps plain typing order and honors a moved caret", async () => {
    const user = userEvent.setup();
    const { changes } = renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLInputElement;

    await user.click(input);
    await user.keyboard("abc");

    await waitFor(() => {
      expect(input.value).toBe("abc");
      expect(input.selectionStart).toBe("abc".length);
    });

    setInputSelection(input, "a".length);
    inputFromUser(input, "aXbc", "aX".length);

    await waitFor(() => {
      expect(input.value).toBe("aXbc");
      expect(input.selectionStart).toBe("aX".length);
    });
    expect(changes.at(-1)).toBe("aXbc");
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
