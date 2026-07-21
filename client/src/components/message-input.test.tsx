import { useState, type FormEvent, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { renderNative as render } from "../test/render";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
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

  const result = render(<TestHarness />);
  function TestHarness() {
    const [value, setValue] = useState(initialValue);
    const [mentionUsers, setMentionUsers] = useState<readonly PublicUser[]>(
      options.initialMentionUsers ?? [],
    );

    return (
      <MessageInput
        value={value}
        onChange={(nextValue) => {
          changes.push(nextValue);
          setValue(nextValue);
        }}
        ariaLabel="Compose message"
        placeholder="Send a new message..."
        mentionUsers={mentionUsers}
        onMentionUsers={(users) => {
          discoveredUsers.push([...users]);
          setMentionUsers((current) => mergeUsers(current, users));
        }}
        searchMentionUsers={searchMentionUsers}
        onKeyDown={options.onKeyDown}
      />
    );
  }

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
  const result = render(<TestHarness />);
  function TestHarness() {
    const [value, setValue] = useState(initialValue);

    return (
      <MessageInput
        value={value}
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
  }

  return { ...result, changes };
}

function renderChannelFormHarness(initialValue = "") {
  const changes: string[] = [];
  const onSubmit = vi.fn((event: FormEvent<HTMLFormElement>) => event.preventDefault());

  const result = render(<TestHarness />);
  function TestHarness() {
    const [value, setValue] = useState(initialValue);

    return (
      <form onSubmit={onSubmit}>
        <MessageInput
          value={value}
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
  }

  return { ...result, changes, onSubmit };
}

function renderMentionFormHarness(initialValue = "", searchMentionUsers?: MentionSearch) {
  const changes: string[] = [];
  const onSubmit = vi.fn((event: FormEvent<HTMLFormElement>) => event.preventDefault());
  const search = searchMentionUsers ?? vi.fn(async () => MENTION_USER_FIXTURES);

  const result = render(<TestHarness />);
  function TestHarness() {
    const [value, setValue] = useState(initialValue);
    const [mentionUsers, setMentionUsers] = useState<readonly PublicUser[]>([]);

    return (
      <form onSubmit={onSubmit}>
        <MessageInput
          value={value}
          onChange={(nextValue) => {
            changes.push(nextValue);
            setValue(nextValue);
          }}
          ariaLabel="Compose message"
          placeholder="Send a new message..."
          mentionUsers={mentionUsers}
          onMentionUsers={(users) => setMentionUsers((current) => mergeUsers(current, users))}
          searchMentionUsers={search}
        />
        <button type="submit">Send</button>
      </form>
    );
  }

  return { ...result, changes, onSubmit, searchMentionUsers: search };
}

function renderHarness(initialValue = "") {
  const changes: string[] = [];
  let setExternalValue: (value: string) => void = () => {};

  const result = render(<TestHarness />);
  function TestHarness() {
    const [value, setValue] = useState(initialValue);
    setExternalValue = (nextValue: string) => act(() => setValue(nextValue));

    return (
      <MessageInput
        value={value}
        onChange={(nextValue) => {
          changes.push(nextValue);
          setValue(nextValue);
        }}
        ariaLabel="Compose message"
        placeholder="Send a new message..."
      />
    );
  }

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

  const result = render(<TestHarness />);
  function TestHarness() {
    const [value, setValue] = useState(initialValue);

    return (
      <AuthProvider>
        <CustomEmojisProvider>
          <MessageInput
            value={value}
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
  }

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

const CARET_SENTINEL = "\u200B";

function serializedNode(node: Node): string {
  if (node instanceof HTMLElement) {
    const marker =
      node.dataset.emojiMarker ?? node.dataset.mentionMarker ?? node.dataset.channelMarker;
    if (marker) return marker;
    if (node.dataset.editorCaretBoundary === "true") {
      return `\n${Array.from(node.childNodes, serializedNode).join("")}`;
    }
    if (node.dataset.editorCaretPlaceholder === "true") return "";
    if (node instanceof HTMLBRElement) return "\n";
  }
  if (node.nodeType === Node.TEXT_NODE) {
    return (node.textContent ?? "").split(CARET_SENTINEL).join("");
  }
  return Array.from(node.childNodes, serializedNode).join("");
}

function editorValue(input: HTMLDivElement): string {
  return serializedNode(input);
}

function domPositionForIndex(root: Node, index: number): { node: Node; offset: number } {
  let remaining = Math.max(0, index);
  for (const child of Array.from(root.childNodes)) {
    const value = serializedNode(child);
    const childOffset = Array.prototype.indexOf.call(root.childNodes, child) as number;
    if (remaining === 0) return { node: root, offset: childOffset };
    if (remaining < value.length) {
      if (child.nodeType === Node.TEXT_NODE) {
        let serializedOffset = 0;
        const text = child.textContent ?? "";
        for (let offset = 0; offset < text.length; offset += 1) {
          if (serializedOffset === remaining) return { node: child, offset };
          if (text[offset] !== CARET_SENTINEL) serializedOffset += 1;
        }
        return { node: child, offset: text.length };
      }
      if (child instanceof HTMLElement && child.dataset.emojiMarker) {
        return { node: root, offset: childOffset + (remaining > value.length / 2 ? 1 : 0) };
      }
      return domPositionForIndex(child, remaining);
    }
    if (remaining === value.length) return { node: root, offset: childOffset + 1 };
    remaining -= value.length;
  }
  return { node: root, offset: root.childNodes.length };
}

function serializedOffset(root: Node, container: Node | null, offset: number): number {
  if (!container || !root.contains(container)) return editorValue(root as HTMLDivElement).length;
  let current = container;
  let result =
    current.nodeType === Node.TEXT_NODE
      ? (current.textContent ?? "").slice(0, offset).split(CARET_SENTINEL).join("").length
      : Array.from(current.childNodes).slice(0, offset).map(serializedNode).join("").length;
  if (current instanceof HTMLElement && current.dataset.editorCaretBoundary === "true") result += 1;
  while (current !== root && current.parentNode) {
    const parent = current.parentNode;
    const childOffset = Array.prototype.indexOf.call(parent.childNodes, current) as number;
    result += Array.from(parent.childNodes)
      .slice(0, childOffset)
      .map(serializedNode)
      .join("").length;
    if (parent instanceof HTMLElement && parent.dataset.editorCaretBoundary === "true") result += 1;
    current = parent;
  }
  return result;
}

function editorSelection(input: HTMLDivElement) {
  const selection = window.getSelection();
  return {
    start: serializedOffset(input, selection?.anchorNode ?? null, selection?.anchorOffset ?? 0),
    end: serializedOffset(input, selection?.focusNode ?? null, selection?.focusOffset ?? 0),
  };
}

function setInputSelection(input: HTMLDivElement, start: number, end = start) {
  act(() => {
    input.focus();
    const range = document.createRange();
    const startPosition = domPositionForIndex(input, start);
    const endPosition = domPositionForIndex(input, end);
    range.setStart(startPosition.node, startPosition.offset);
    range.setEnd(endPosition.node, endPosition.offset);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
    fireEvent.mouseUp(input);
  });
}

function inputFromUser(
  input: HTMLDivElement,
  value: string,
  caretIndex = value.length,
  eventInit?: InputEventInit,
) {
  act(() => {
    input.textContent = value;
    setDomSelection(input.firstChild ?? input, input.firstChild ? caretIndex : 0);
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
  const onSubmit = vi.fn((event: FormEvent<HTMLFormElement>) => event.preventDefault());

  const result = render(<TestHarness />);
  function TestHarness() {
    const [value, setValue] = useState(initialValue);

    return (
      <form onSubmit={onSubmit}>
        <MessageInput
          value={value}
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
  }

  return { ...result, changes, onSubmit };
}

function keyDown(input: HTMLElement, init: KeyboardEventInit): KeyboardEvent {
  const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
  fireEvent(input, event);
  return event;
}

function composingEnter(input: HTMLElement): KeyboardEvent {
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

  test("exposes only the native div ref and reports detachment", () => {
    const inputRef = vi.fn<(element: HTMLDivElement | null) => void>();
    const { unmount } = render(
      <MessageInput
        value="draft"
        onChange={vi.fn()}
        ariaLabel="Compose message"
        inputRef={inputRef}
      />,
    );

    const input = screen.getByRole("textbox", { name: /compose message/i });
    expect(inputRef).toHaveBeenLastCalledWith(input);
    expect(input).toBeInstanceOf(HTMLDivElement);
    expect(Object.hasOwn(input, "value")).toBe(false);
    expect(Object.hasOwn(input, "selectionStart")).toBe(false);
    expect(Object.hasOwn(input, "selectionEnd")).toBe(false);
    expect(Object.hasOwn(input, "setSelectionRange")).toBe(false);

    unmount();
    expect(inputRef).toHaveBeenLastCalledWith(null);
  });

  test("keeps the multiline composer and emoji trigger keyboard reachable with visible focus styles", async () => {
    const user = userEvent.setup();
    renderHarness();

    const input = screen.getByRole("textbox", { name: /compose message/i });
    const emojiButton = screen.getByRole("button", { name: /open emoji picker/i });

    expect(input).toHaveClass(
      "focus:outline-none",
      "focus-visible:ring-2",
      "focus-visible:ring-ring",
    );
    expect(emojiButton).toHaveClass(
      "focus:outline-none",
      "focus-visible:ring-2",
      "focus-visible:ring-ring",
    );

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
      const input = screen.getByRole("textbox", { name: /compose message/i }) as HTMLDivElement;
      setInputSelection(input, caret);

      const event = keyDown(input, { key: "Enter", shiftKey: true });

      expect(event.defaultPrevented).toBe(true);
      await waitFor(() => {
        expect(editorValue(input)).toBe(nextValue);
        expect(editorSelection(input).start).toBe(nextCaret);
        expect(editorSelection(input).end).toBe(nextCaret);
      });
      expect(changes.at(-1)).toBe(nextValue);
    },
  );

  test("Shift+Enter replaces selected text with one newline and restores the caret after it", async () => {
    const { changes } = renderHarness("hello world");
    const input = screen.getByRole("textbox", { name: /compose message/i }) as HTMLDivElement;
    setInputSelection(input, "hello".length, "hello world".length);

    const event = keyDown(input, { key: "Enter", shiftKey: true });

    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => {
      expect(editorValue(input)).toBe("hello\n");
      expect(editorSelection(input).start).toBe("hello\n".length);
      expect(editorSelection(input).end).toBe("hello\n".length);
    });
    expect(changes.at(-1)).toBe("hello\n");
  });

  test("Shift+Enter replaces a serialized custom emoji chip selection with one newline", async () => {
    const { changes } = renderHarnessWithCustomEmojis("a <:party:123> b");
    const input = screen.getByRole("textbox", { name: /compose message/i }) as HTMLDivElement;
    await screen.findByRole("img", { name: /custom emoji :party:/i });
    setInputSelection(input, "a ".length, "a <:party:123>".length);

    const event = keyDown(input, { key: "Enter", shiftKey: true });

    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => {
      expect(editorValue(input)).toBe("a \n b");
      expect(editorSelection(input).start).toBe("a \n".length);
      expect(editorSelection(input).end).toBe("a \n".length);
    });
    expect(changes.at(-1)).toBe("a \n b");
  });

  test("plain Enter submits the owning form without changing the editor text", async () => {
    const { changes, onSubmit } = renderFormHarness("ready to send");
    const input = screen.getByRole("textbox", { name: /compose message/i }) as HTMLDivElement;
    setInputSelection(input, "ready".length);

    const event = keyDown(input, { key: "Enter" });

    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    expect(editorValue(input)).toBe("ready to send");
    expect(input.querySelector("br, div")).toBeNull();
    expect(changes).toEqual([]);
  });

  test("Enter during IME composition does not submit or block composition", () => {
    const { onSubmit } = renderFormHarness("composing");
    const input = screen.getByRole("textbox", { name: /compose message/i }) as HTMLDivElement;

    const event = composingEnter(input);

    expect(event.defaultPrevented).toBe(false);
    expect(onSubmit).not.toHaveBeenCalled();
    expect(editorValue(input)).toBe("composing");
  });

  test("Enter cannot submit while the composition ref is active", () => {
    const { onSubmit } = renderFormHarness("composing");
    const input = screen.getByRole("textbox", { name: /compose message/i }) as HTMLDivElement;

    fireEvent.compositionStart(input);
    const event = keyDown(input, { key: "Enter" });

    expect(event.defaultPrevented).toBe(false);
    expect(onSubmit).not.toHaveBeenCalled();
    fireEvent.compositionEnd(input);
  });

  test("defers controlled DOM and selection reconstruction until IME composition ends", async () => {
    renderHarness("a");
    const input = screen.getByRole("textbox", { name: /compose message/i }) as HTMLDivElement;

    fireEvent.compositionStart(input);
    input.textContent = "あ";
    const activeCompositionNode = input.firstChild;
    setDomSelection(activeCompositionNode ?? input, 1);
    const domSelection = window.getSelection();
    if (!domSelection) throw new Error("Expected a document selection");
    const removeAllRanges = vi.spyOn(domSelection, "removeAllRanges");
    const addRange = vi.spyOn(domSelection, "addRange");

    fireEvent.input(input, { data: "あ", inputType: "insertCompositionText", isComposing: true });

    expect(editorValue(input)).toBe("あ");
    expect(input.firstChild).toBe(activeCompositionNode);
    expect(removeAllRanges).not.toHaveBeenCalled();
    expect(addRange).not.toHaveBeenCalled();

    fireEvent.compositionEnd(input, { data: "あ" });

    expect(editorValue(input)).toBe("あ");
    expect(input.firstChild).toBe(activeCompositionNode);
    expect(removeAllRanges).not.toHaveBeenCalled();
    expect(addRange).not.toHaveBeenCalled();
    await waitFor(() => {
      expect(removeAllRanges).toHaveBeenCalled();
      expect(addRange).toHaveBeenCalled();
      expect(removeAllRanges).toHaveBeenCalledTimes(addRange.mock.calls.length);
      expect(editorSelection(input)).toEqual({ start: 1, end: 1 });
    });
  });

  test.each([
    ["rejects", (nextValue: string) => (nextValue === "changed" ? "fixed" : nextValue)],
    ["normalizes", (nextValue: string) => nextValue.toUpperCase()],
  ])(
    "restores the authoritative controlled value when the owner %s an edit",
    async (_, ownEdit) => {
      const changes: string[] = [];
      render(<ControlledHarness />);

      function ControlledHarness() {
        const [value, setValue] = useState("fixed");
        return (
          <MessageInput
            value={value}
            ariaLabel="Compose message"
            onChange={(nextValue) => {
              changes.push(nextValue);
              setValue(ownEdit(nextValue));
            }}
          />
        );
      }

      const input = screen.getByRole("textbox", { name: /compose message/i }) as HTMLDivElement;
      inputFromUser(input, "changed");

      await waitFor(() => {
        expect(editorValue(input)).toBe(ownEdit("changed"));
      });
      expect(changes).toEqual(["changed"]);
    },
  );

  test.each([
    ["Ctrl+Enter", { ctrlKey: true }],
    ["Meta+Enter", { metaKey: true }],
    ["Alt+Enter", { altKey: true }],
    ["Ctrl+Shift+Enter", { ctrlKey: true, shiftKey: true }],
  ])("%s is ignored without submitting or editing", async (_, modifiers) => {
    const { changes, onSubmit } = renderFormHarness("unchanged");
    const input = screen.getByRole("textbox", { name: /compose message/i }) as HTMLDivElement;
    setInputSelection(input, 4);

    const event = keyDown(input, { key: "Enter", ...modifiers });

    expect(event.defaultPrevented).toBe(true);
    await Promise.resolve();
    expect(onSubmit).not.toHaveBeenCalled();
    expect(editorValue(input)).toBe("unchanged");
    expect(editorSelection(input).start).toBe(4);
    expect(changes).toEqual([]);
  });

  test("ignored non-Enter keys fall through to normal editing", () => {
    renderHarness("abc");
    const input = screen.getByRole("textbox", { name: /compose message/i }) as HTMLDivElement;
    setInputSelection(input, 1);

    const event = keyDown(input, { key: "a" });

    expect(event.defaultPrevented).toBe(false);
    expect(editorValue(input)).toBe("abc");
  });

  test("owner keyboard handlers can cancel editor handling", () => {
    const onCancel = vi.fn();
    render(
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
      />,
    );
    const input = screen.getByRole("textbox", { name: /compose message/i }) as HTMLDivElement;

    const event = keyDown(input, { key: "Escape" });

    expect(event.defaultPrevented).toBe(true);
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(editorValue(input)).toBe("draft");
  });

  test("replays an initial mention search safely under Strict Mode", async () => {
    const searchMentionUsers = vi.fn<MentionSearch>(async () => MENTION_USER_FIXTURES);
    renderMentionHarness("@bo", { searchMentionUsers });

    expect(await screen.findByRole("option", { name: /mention bobby @bob/i })).toBeInTheDocument();
    expect(searchMentionUsers).toHaveBeenCalledWith({ query: "bo", limit: 8 });
  });

  test("reruns mention search when its callback is replaced for an unchanged token", async () => {
    const staleSearch = deferred<PublicUser[]>();
    const firstSearch = vi.fn<MentionSearch>(() => staleSearch.promise);
    const replacementSearch = vi.fn<MentionSearch>(async () => [MENTION_USER_FIXTURES[2]]);
    const result = render(
      <MessageInput
        value="@ca"
        onChange={vi.fn()}
        ariaLabel="Compose message"
        searchMentionUsers={firstSearch}
      />,
    );

    await waitFor(() => expect(firstSearch).toHaveBeenCalled());
    result.rerender(
      <MessageInput
        value="@ca"
        onChange={vi.fn()}
        ariaLabel="Compose message"
        searchMentionUsers={replacementSearch}
      />,
    );

    expect(
      await screen.findByRole("option", { name: /mention carol @carol/i }),
    ).toBeInTheDocument();
    staleSearch.resolve([MENTION_USER_FIXTURES[1]]);
    await Promise.resolve();
    expect(screen.queryByRole("option", { name: /mention @alice/i })).toBeNull();
  });

  test("shows mention autocomplete for a boundary-valid empty @ token", async () => {
    const { container, searchMentionUsers, discoveredUsers } = renderMentionHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

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
    expect(within(bobOption).getByText("@bob")).toHaveClass("text-muted-foreground");
    expect(input).toHaveAttribute("aria-autocomplete", "list");
    expect(input).toHaveAttribute("aria-controls", listbox.id);
    expect(input).toHaveAttribute("aria-activedescendant", options[0].id);
    await expectNoA11yViolations(container, "message input mention autocomplete");
  });

  test("hides mention autocomplete while text is selected", async () => {
    renderMentionHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    inputFromUser(input, "@bo");
    await screen.findByRole("listbox", { name: /mention suggestions/i });

    setInputSelection(input, 0, "@bo".length);
    await waitFor(() => {
      expect(screen.queryByRole("listbox", { name: /mention suggestions/i })).toBeNull();
    });
  });

  test("ignores failed mention searches quietly", async () => {
    const searchMentionUsers = vi.fn<MentionSearch>(async () => {
      throw new Error("network down");
    });
    renderMentionHarness("", { searchMentionUsers });
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    inputFromUser(input, "@zz");

    await waitFor(() => expect(searchMentionUsers).toHaveBeenCalledWith({ query: "zz", limit: 8 }));
    await waitFor(() => {
      expect(screen.queryByRole("listbox", { name: /mention suggestions/i })).toBeNull();
    });
  });

  test("ignores stale mention search responses", async () => {
    const staleSearch = deferred<PublicUser[]>();
    const searchMentionUsers = vi.fn<MentionSearch>((options) => {
      if (options.query === "a") return staleSearch.promise;
      return Promise.resolve([MENTION_USER_FIXTURES[2]]);
    });
    renderMentionHarness("", { searchMentionUsers });
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

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

  test.each(["Enter", "Tab"])(
    "%s does not replace an inline mention token when the caret has moved away",
    async (key) => {
      const { changes, onSubmit } = renderMentionFormHarness();
      const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;
      const value = "before @bo after";

      inputFromUser(input, value, "before @bo".length);
      await screen.findByRole("listbox", { name: /mention suggestions/i });
      setInputSelection(input, 0);
      await waitFor(() => {
        expect(screen.queryByRole("listbox", { name: /mention suggestions/i })).toBeNull();
      });

      const event = keyDown(input, { key });

      expect(event.defaultPrevented).toBe(key === "Enter");
      await Promise.resolve();
      expect(editorValue(input)).toBe(value);
      expect(changes).toEqual([value]);
      expect(onSubmit).toHaveBeenCalledTimes(key === "Enter" ? 1 : 0);
    },
  );

  test("Enter commits the selected mention as a durable chip before form submit", async () => {
    const { changes, onSubmit } = renderMentionFormHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    inputFromUser(input, "hello @bo world", "hello @bo".length);
    await screen.findByRole("listbox", { name: /mention suggestions/i });

    const commitEvent = keyDown(input, { key: "Enter" });

    expect(commitEvent.defaultPrevented).toBe(true);
    await waitFor(() => {
      expect(editorValue(input)).toBe("hello <@2> world");
      expect(editorSelection(input).start).toBe("hello <@2> ".length);
      expect(screen.getByText("@Bobby")).toHaveClass("bg-primary/10", "text-primary");
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
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

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
    await waitFor(() => expect(editorValue(input)).toBe("<@3> "));
    expect(changes.at(-1)).toBe("<@3> ");

    inputFromUser(input, "@bo");
    const bob = await screen.findByRole("option", { name: /mention bobby @bob/i });
    const mouseDown = new MouseEvent("mousedown", { bubbles: true, cancelable: true });
    fireEvent(bob, mouseDown);
    expect(mouseDown.defaultPrevented).toBe(true);
    fireEvent.click(bob);

    await waitFor(() => {
      expect(editorValue(input)).toBe("<@2> ");
      expect(document.activeElement).toBe(input);
    });
  });

  test("Escape dismisses mention autocomplete before owner keyboard handlers", async () => {
    const ownerKeyDown = vi.fn((event: ReactKeyboardEvent<HTMLDivElement>) => {
      if (event.key === "Escape") event.preventDefault();
    });
    renderMentionHarness("", { onKeyDown: ownerKeyDown });
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

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
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

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
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    expect(editorValue(input)).toBe("hello <@2> missing <@999>");
    await waitFor(() => {
      expect(within(input).getByText("@Bobby")).toHaveClass("bg-primary/10", "text-primary");
    });
    expect(input.textContent).toContain("<@999>");
    expect(screen.queryByText("<@2>")).toBeNull();

    input.append(document.createTextNode("!"));
    setDomSelection(input, input.childNodes.length);
    fireEvent.input(input);

    await waitFor(() => {
      expect(editorValue(input)).toBe("hello <@2> missing <@999>!");
    });
    expect(changes.at(-1)).toBe("hello <@2> missing <@999>!");
  });

  test("shows channel autocomplete for a boundary-valid # token and suggests text channels only", async () => {
    const { container } = renderChannelHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

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
      const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

      inputFromUser(input, value, selectionEnd);
      setInputSelection(input, selectionStart, selectionEnd);

      await waitFor(() => {
        expect(screen.queryByRole("listbox", { name: /channel suggestions/i })).toBeNull();
      });
    },
  );

  test.each(["Enter", "Tab"])(
    "%s does not replace an inline channel token when the caret has moved away",
    async (key) => {
      const { changes, onSubmit } = renderChannelFormHarness();
      const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;
      const value = "before #gen after";

      inputFromUser(input, value, "before #gen".length);
      await screen.findByRole("listbox", { name: /channel suggestions/i });
      setInputSelection(input, 0);
      await waitFor(() => {
        expect(screen.queryByRole("listbox", { name: /channel suggestions/i })).toBeNull();
      });

      const event = keyDown(input, { key });

      expect(event.defaultPrevented).toBe(key === "Enter");
      await Promise.resolve();
      expect(editorValue(input)).toBe(value);
      expect(changes).toEqual([value]);
      expect(onSubmit).toHaveBeenCalledTimes(key === "Enter" ? 1 : 0);
    },
  );

  test("Enter commits a selected channel before form submit and renders it as a chip", async () => {
    const { changes, onSubmit } = renderChannelFormHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    inputFromUser(input, "hello #gen world", "hello #gen".length);
    await screen.findByRole("listbox", { name: /channel suggestions/i });

    const event = keyDown(input, { key: "Enter" });

    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => {
      expect(editorValue(input)).toBe("hello <#100> world");
      expect(editorSelection(input).start).toBe("hello <#100> ".length);
      expect(within(input).getByText("#general")).toHaveClass("bg-muted", "text-foreground");
    });
    expect(screen.queryByText("<#100>")).toBeNull();
    expect(onSubmit).not.toHaveBeenCalled();
    expect(changes.at(-1)).toBe("hello <#100> world");
  });

  test("Tab, mouse, and arrow keys work for channel autocomplete", async () => {
    const { changes } = renderChannelHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

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
    await waitFor(() => expect(editorValue(input)).toBe("<#100> "));
    expect(changes.at(-1)).toBe("<#100> ");

    inputFromUser(input, "#ran");
    const random = await screen.findByRole("option", { name: /channel #random/i });
    const mouseDown = new MouseEvent("mousedown", { bubbles: true, cancelable: true });
    fireEvent(random, mouseDown);
    expect(mouseDown.defaultPrevented).toBe(true);
    fireEvent.click(random);

    await waitFor(() => {
      expect(editorValue(input)).toBe("<#200> ");
      expect(document.activeElement).toBe(input);
    });
  });

  test("Escape dismisses channel autocomplete before owner keyboard handlers", async () => {
    const ownerKeyDown = vi.fn((event: ReactKeyboardEvent<HTMLDivElement>) => {
      if (event.key === "Escape") event.preventDefault();
    });
    renderChannelHarness("", { onKeyDown: ownerKeyDown });
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

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
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    expect(editorValue(input)).toBe("hello <#100> missing <#999>");
    await waitFor(() => {
      expect(within(input).getByText("#general")).toHaveClass("bg-muted", "text-foreground");
    });
    expect(input.textContent).toContain("<#999>");
    expect(screen.queryByText("<#100>")).toBeNull();

    input.append(document.createTextNode("!"));
    setDomSelection(input, input.childNodes.length);
    fireEvent.input(input);

    await waitFor(() => {
      expect(editorValue(input)).toBe("hello <#100> missing <#999>!");
    });
    expect(changes.at(-1)).toBe("hello <#100> missing <#999>!");
  });

  test("navigates and deletes channel chips as whole markers", async () => {
    const marker = "<#100>";
    const { changes } = renderChannelHarness(`one\n${marker}\ntwo`);
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;
    await within(input).findByText("#general");

    setInputSelection(input, "one\n".length);
    fireEvent.keyDown(input, { key: "ArrowRight" });
    await waitFor(() => {
      expect(editorSelection(input).start).toBe(`one\n${marker}`.length);
    });

    fireEvent.keyDown(input, { key: "ArrowLeft" });
    await waitFor(() => {
      expect(editorSelection(input).start).toBe("one\n".length);
    });

    fireEvent.keyDown(input, { key: "Delete" });
    await waitFor(() => {
      expect(editorValue(input)).toBe("one\n\ntwo");
    });
    expect(changes).toContain("one\n\ntwo");
  });

  test("shows native emoji autocomplete suggestions for a boundary-valid prefix", async () => {
    const { container } = renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

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
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    inputFromUser(input, ":satisfied");

    const option = await screen.findByRole("option", {
      name: /emoji :laughing:, also matches :satisfied:/i,
    });
    expect(within(option).getByText(":laughing:")).toHaveClass("font-semibold");
    expect(within(option).getByText(/also matches :satisfied:/i)).toHaveClass(
      "text-muted-foreground",
    );
    expect(option.querySelector("[aria-hidden='true']")).toHaveClass("h-9", "w-9", "bg-muted");
  });

  test("shows active custom emoji autocomplete suggestions with image previews and excludes deleted emojis", async () => {
    const { container } = renderHarnessWithCustomEmojis();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

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
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    inputFromUser(input, "hello :pa world", "hello :pa".length);
    const partyOption = await screen.findByRole("option", { name: /emoji :party:/i });
    expect(partyOption).toHaveAttribute("aria-selected", "true");

    const event = keyDown(input, { key: "Enter" });

    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => {
      expect(editorValue(input)).toBe("hello <:party:123> world");
      expect(editorSelection(input).start).toBe("hello <:party:123>".length);
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
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

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
    render(<TestHarness />);
    function TestHarness() {
      const [value, setValue] = useState("");

      return (
        <AuthProvider>
          <CustomEmojisProvider>
            <MessageInput
              value={value}
              onChange={setValue}
              ariaLabel="Compose message"
              placeholder="Send a new message..."
            />
          </CustomEmojisProvider>
        </AuthProvider>
      );
    }
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    inputFromUser(input, ":sm");
    expect(await screen.findByRole("option", { name: /emoji :smiley:/i })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /emoji :party:/i })).toBeNull();

    resolveEmojis();
    inputFromUser(input, ":pa");

    expect(await screen.findByRole("option", { name: /emoji :party:/i })).toBeInTheDocument();
  });

  test("hides emoji autocomplete when a boundary-valid query has zero matches", async () => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    inputFromUser(input, ":qq");
    await Promise.resolve();

    expect(screen.queryByRole("listbox", { name: /emoji suggestions/i })).toBeNull();
    expect(screen.queryByRole("option")).toBeNull();
    expect(screen.queryByText(/no results|loading|error/i)).toBeNull();
  });

  test("opening the emoji picker closes the active emoji autocomplete menu", async () => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    inputFromUser(input, ":sm");
    await expectEmojiAutocompleteOpen();

    fireEvent.click(screen.getByRole("button", { name: /open emoji picker/i }));

    await screen.findByRole("dialog", { name: pickerName });
    await expectEmojiAutocompleteClosed();
  });

  test("opening emoji autocomplete closes the emoji picker", async () => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

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
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    inputFromUser(input, value, caretIndex);

    const options = await expectEmojiAutocompleteOpen();
    expect(options[0]).toHaveAccessibleName(/:smiley:/i);
  });

  test("opens emoji autocomplete after a custom emoji chip", async () => {
    renderHarnessWithCustomEmojis("<:party:123>:sm");
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;
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
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    inputFromUser(input, value, selectionEnd);
    setInputSelection(input, selectionStart, selectionEnd);

    await expectEmojiAutocompleteClosed();
  });

  test("Escape dismisses only the current emoji autocomplete token session", async () => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

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

  test.each(["Enter", "Tab"])(
    "%s does not replace an inline emoji token when the caret has moved away",
    async (key) => {
      const { changes, onSubmit } = renderFormHarness();
      const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;
      const value = "before :sm after";

      inputFromUser(input, value, "before :sm".length);
      await screen.findByRole("listbox", { name: /emoji suggestions/i });
      setInputSelection(input, 0);
      await expectEmojiAutocompleteClosed();

      const event = keyDown(input, { key });

      expect(event.defaultPrevented).toBe(key === "Enter");
      await Promise.resolve();
      expect(editorValue(input)).toBe(value);
      expect(changes).toEqual([value]);
      expect(onSubmit).toHaveBeenCalledTimes(key === "Enter" ? 1 : 0);
    },
  );

  test("Enter commits the selected native emoji autocomplete suggestion without submitting", async () => {
    const { changes, onSubmit } = renderFormHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    inputFromUser(input, "hello :sm world", "hello :sm".length);
    await screen.findByRole("listbox", { name: /emoji suggestions/i });

    const event = keyDown(input, { key: "Enter" });

    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => {
      expect(editorValue(input)).toBe("hello 😃 world");
      expect(editorSelection(input).start).toBe("hello 😃".length);
      expect(editorSelection(input).end).toBe("hello 😃".length);
    });
    expect(onSubmit).not.toHaveBeenCalled();
    expect(changes.at(-1)).toBe("hello 😃 world");
    expect(screen.queryByRole("listbox", { name: /emoji suggestions/i })).toBeNull();
  });

  test("Tab commits the selected native emoji autocomplete suggestion without moving focus", async () => {
    const { changes, onSubmit } = renderFormHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    inputFromUser(input, "hello :sm world", "hello :sm".length);
    await screen.findByRole("listbox", { name: /emoji suggestions/i });

    const event = keyDown(input, { key: "Tab" });

    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => {
      expect(editorValue(input)).toBe("hello 😃 world");
      expect(editorSelection(input).start).toBe("hello 😃".length);
      expect(document.activeElement).toBe(input);
    });
    expect(onSubmit).not.toHaveBeenCalled();
    expect(changes.at(-1)).toBe("hello 😃 world");
    expect(screen.queryByRole("listbox", { name: /emoji suggestions/i })).toBeNull();
  });

  test("Arrow keys wrap autocomplete selection and query changes reset it", async () => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

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
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

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
    expect(editorValue(input)).toBe("hello :sm world");
    expect(changes).toEqual(["hello :sm world"]);
  });

  test("Shift+Enter does not commit autocomplete suggestions", async () => {
    const { changes, onSubmit } = renderFormHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    inputFromUser(input, "hello :sm world", "hello :sm".length);
    await screen.findByRole("listbox", { name: /emoji suggestions/i });

    const event = keyDown(input, { key: "Enter", shiftKey: true });

    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => {
      expect(editorValue(input)).toBe("hello :sm\n world");
      expect(editorSelection(input).start).toBe("hello :sm\n".length);
    });
    expect(onSubmit).not.toHaveBeenCalled();
    expect(changes.at(-1)).toBe("hello :sm\n world");
  });

  test("autocomplete shortcuts take priority over owner keyboard handlers while open", async () => {
    const ownerKeyDown = vi.fn((event: ReactKeyboardEvent<HTMLDivElement>) => {
      if (event.key === "Enter" || event.key === "Escape") event.preventDefault();
    });

    const changes: string[] = [];
    render(<TestHarness />);
    function TestHarness() {
      const [value, setValue] = useState(":sm");

      return (
        <MessageInput
          value={value}
          onChange={(nextValue) => {
            changes.push(nextValue);
            setValue(nextValue);
          }}
          ariaLabel="Compose message"
          onKeyDown={ownerKeyDown}
        />
      );
    }
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;
    setInputSelection(input, ":sm".length);
    await screen.findByRole("listbox", { name: /emoji suggestions/i });

    const event = keyDown(input, { key: "Enter" });

    expect(event.defaultPrevented).toBe(true);
    await waitFor(() => expect(editorValue(input)).toBe("😃"));
    expect(changes.at(-1)).toBe("😃");
    expect(ownerKeyDown).not.toHaveBeenCalled();
  });

  test("clicking a native emoji autocomplete suggestion replaces only the active token", async () => {
    const { changes } = renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    inputFromUser(input, "before :sm after", "before :sm".length);
    const option = await screen.findByRole("option", { name: /:smile:/i });

    const mouseDown = new MouseEvent("mousedown", { bubbles: true, cancelable: true });
    fireEvent(option, mouseDown);
    expect(mouseDown.defaultPrevented).toBe(true);
    fireEvent.click(option);

    await waitFor(() => {
      expect(editorValue(input)).toBe("before 😄 after");
      expect(editorSelection(input).start).toBe("before 😄".length);
      expect(editorSelection(input).end).toBe("before 😄".length);
      expect(document.activeElement).toBe(input);
    });
    expect(changes.at(-1)).toBe("before 😄 after");
  });

  test("converts completed emoji shortcodes before the caret", async () => {
    const { changes } = renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    inputFromUser(input, ":grinning:");

    await waitFor(() => {
      expect(editorValue(input)).toBe("😀");
      expect(editorSelection(input).start).toBe("😀".length);
    });
    expect(changes).toContain("😀");
  });

  test("converts pasted shortcode chains at a mid-draft caret", async () => {
    renderHarness("hello world");
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;
    const rawValue = "hello :grinning::heart:world";
    const rawCaretIndex = "hello :grinning::heart:".length;

    inputFromUser(input, rawValue, rawCaretIndex, {
      data: ":grinning::heart:",
      inputType: "insertFromPaste",
    });

    await waitFor(() => {
      expect(editorValue(input)).toBe("hello 😀❤️world");
      expect(editorSelection(input).start).toBe("hello 😀❤️".length);
      expect(editorSelection(input).end).toBe("hello 😀❤️".length);
    });
  });

  test.each([
    ["after a newline", "first\n:grinning:", "first\n:grinning:".length, "first\n😀"],
    ["before a newline", "first :heart:\nsecond", "first :heart:".length, "first ❤️\nsecond"],
  ])("converts completed emoji shortcodes %s", async (_, rawValue, caretIndex, nextValue) => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;
    const expectedCaretIndex = nextValue.includes("\nsecond")
      ? nextValue.indexOf("\n")
      : nextValue.length;

    inputFromUser(input, rawValue, caretIndex, {
      data: rawValue,
      inputType: "insertFromPaste",
    });

    await waitFor(() => {
      expect(editorValue(input)).toBe(nextValue);
      expect(editorSelection(input).start).toBe(expectedCaretIndex);
    });
  });

  test("serializes browser-pasted line break elements as newline characters before shortcode replacement", async () => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;
    const firstLine = document.createTextNode("first :heart:");
    const lineBreak = document.createElement("br");
    const secondLine = document.createTextNode("second");
    input.replaceChildren(firstLine, lineBreak, secondLine);
    setDomSelection(firstLine, firstLine.textContent?.length ?? 0);

    fireEvent.input(input, { inputType: "insertFromPaste" });

    await waitFor(() => {
      expect(editorValue(input)).toBe("first ❤️\nsecond");
      expect(editorSelection(input).start).toBe("first ❤️".length);
    });
  });

  test("serializes browser-pasted line break elements as newline characters after shortcode boundaries", async () => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;
    const firstLine = document.createTextNode("first");
    const lineBreak = document.createElement("br");
    const secondLine = document.createTextNode(":grinning:");
    input.replaceChildren(firstLine, lineBreak, secondLine);
    setDomSelection(secondLine, secondLine.textContent?.length ?? 0);

    fireEvent.input(input, { inputType: "insertFromPaste" });

    await waitFor(() => {
      expect(editorValue(input)).toBe("first\n😀");
      expect(editorSelection(input).start).toBe("first\n😀".length);
    });
  });

  test("converts shortcodes after opening punctuation and inserted emoji boundaries", async () => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    inputFromUser(input, "(:heart:");

    await waitFor(() => {
      expect(editorValue(input)).toBe("(❤️");
      expect(editorSelection(input).start).toBe("(❤️".length);
    });

    inputFromUser(input, "(❤️:grinning:");

    await waitFor(() => {
      expect(editorValue(input)).toBe("(❤️😀");
      expect(editorSelection(input).start).toBe("(❤️😀".length);
    });
  });

  test("leaves word-attached shortcode-looking text literal", async () => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    inputFromUser(input, "abc:grinning:");

    await waitFor(() => {
      expect(editorValue(input)).toBe("abc:grinning:");
      expect(editorSelection(input).start).toBe("abc:grinning:".length);
    });
  });

  test("inserts a selected emoji at the current caret", async () => {
    renderHarness("hello world");
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;
    setInputSelection(input, "hello ".length);

    await selectEmoji(":smile:", /emoji :smile:/i);

    await waitFor(() => {
      expect(editorValue(input)).toBe("hello 😄world");
      expect(editorSelection(input).start).toBe("hello 😄".length);
      expect(document.activeElement).toBe(input);
    });
    expect(screen.queryByRole("dialog", { name: pickerName })).toBeNull();
  });

  test("inserts a selected emoji at a caret after a newline", async () => {
    renderHarness("hello\nworld");
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;
    setInputSelection(input, "hello\n".length);

    await selectEmoji("heart", /emoji :heart:/i);

    await waitFor(() => {
      expect(editorValue(input)).toBe("hello\n❤️world");
      expect(editorSelection(input).start).toBe("hello\n❤️".length);
      expect(document.activeElement).toBe(input);
    });
  });

  test("replaces selected text spanning a newline with a selected emoji", async () => {
    renderHarness("hello\nworld");
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;
    setInputSelection(input, "hello".length, "hello\nwor".length);

    await selectEmoji(":smile:", /emoji :smile:/i);

    await waitFor(() => {
      expect(editorValue(input)).toBe("hello😄ld");
      expect(editorSelection(input).start).toBe("hello😄".length);
      expect(document.activeElement).toBe(input);
    });
  });

  test("renders controlled custom emoji markers as accessible image-only chips", async () => {
    renderHarnessWithCustomEmojis("hello <:party:123>");

    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;
    expect(editorValue(input)).toBe("hello <:party:123>");
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

    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;
    expect(editorValue(input)).toBe("hello\n<:party:123>\n<a:dance:456>");
    expect(editorValue(input)).not.toContain("\u200B");
    await screen.findByRole("img", { name: /custom emoji :party:/i });
    await screen.findByRole("img", { name: /custom emoji :dance:/i });
    expect(screen.queryByText("<:party:123>")).toBeNull();
    expect(screen.queryByText("<a:dance:456>")).toBeNull();
  });

  test("keeps a custom-emoji-only draft editable from the caret boundaries", async () => {
    const { changes } = renderHarnessWithCustomEmojis("<:party:123>");

    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;
    await screen.findByRole("img", { name: /custom emoji :party:/i });

    expect(editorValue(input)).toBe("<:party:123>");
    expect(input.firstChild).toBe(screen.getByRole("img", { name: /custom emoji :party:/i }));
    expect(input.lastChild?.textContent).toBe("\u200B");

    setInputSelection(input, 0);
    fireEvent.keyDown(input, { key: "ArrowRight" });
    await waitFor(() => {
      expect(editorSelection(input).start).toBe(editorValue(input).length);
    });

    fireEvent.keyDown(input, { key: "ArrowLeft" });
    await waitFor(() => {
      expect(editorSelection(input).start).toBe(0);
    });

    setInputSelection(input, editorValue(input).length);
    fireEvent.keyDown(input, { key: "Backspace" });

    await waitFor(() => {
      expect(editorValue(input)).toBe("");
    });
    expect(changes).toContain("");
  });

  test("navigates across custom emoji markers adjacent to newlines as whole markers", async () => {
    const marker = "<:party:123>";
    renderHarnessWithCustomEmojis(`one\n${marker}\ntwo`);
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;
    await screen.findByRole("img", { name: /custom emoji :party:/i });

    setInputSelection(input, "one\n".length);
    fireEvent.keyDown(input, { key: "ArrowRight" });
    await waitFor(() => {
      expect(editorSelection(input).start).toBe(`one\n${marker}`.length);
    });

    fireEvent.keyDown(input, { key: "ArrowLeft" });
    await waitFor(() => {
      expect(editorSelection(input).start).toBe("one\n".length);
    });
  });

  test("deletes custom emoji markers adjacent to newlines as whole markers", async () => {
    const marker = "<:party:123>";
    const { changes } = renderHarnessWithCustomEmojis(`one\n${marker}\ntwo`);
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;
    await screen.findByRole("img", { name: /custom emoji :party:/i });

    setInputSelection(input, `one\n${marker}`.length);
    fireEvent.keyDown(input, { key: "Backspace" });
    await waitFor(() => {
      expect(editorValue(input)).toBe("one\n\ntwo");
    });
    expect(changes).toContain("one\n\ntwo");
  });

  test("forward-deletes custom emoji markers adjacent to newlines as whole markers", async () => {
    const marker = "<:party:123>";
    const { changes } = renderHarnessWithCustomEmojis(`one\n${marker}\ntwo`);
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;
    await screen.findByRole("img", { name: /custom emoji :party:/i });

    setInputSelection(input, "one\n".length);
    fireEvent.keyDown(input, { key: "Delete" });
    await waitFor(() => {
      expect(editorValue(input)).toBe("one\n\ntwo");
    });
    expect(changes).toContain("one\n\ntwo");
  });

  test("keeps plain typing order and honors a moved caret", async () => {
    const user = userEvent.setup();
    const { changes } = renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    await user.click(input);
    await user.keyboard("abc");

    await waitFor(() => {
      expect(editorValue(input)).toBe("abc");
      expect(editorSelection(input).start).toBe("abc".length);
    });

    setInputSelection(input, "a".length);
    inputFromUser(input, "aXbc", "aX".length);

    await waitFor(() => {
      expect(editorValue(input)).toBe("aXbc");
      expect(editorSelection(input).start).toBe("aX".length);
    });
    expect(changes.at(-1)).toBe("aXbc");
  });

  test("preserves browser-owned contenteditable DOM and caret after a native input event", async () => {
    const { changes } = renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    act(() => {
      input.textContent = "browser draft";
      setDomSelection(input.firstChild ?? input, "browser draft".length);
      fireEvent.input(input, { inputType: "insertText", data: "t" });
    });
    const browserTextNode = input.firstChild;

    await waitFor(() => {
      expect(editorValue(input)).toBe("browser draft");
      expect(editorSelection(input)).toEqual({
        start: "browser draft".length,
        end: "browser draft".length,
      });
    });
    expect(input.firstChild).toBe(browserTextNode);
    expect(changes.at(-1)).toBe("browser draft");
  });

  test("uses the live native caret for a picker selection after browser typing", async () => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    inputFromUser(input, "hello world", "hello ".length, {
      inputType: "insertText",
      data: "d",
    });
    await selectEmoji(":smile:", /emoji :smile:/i);

    await waitFor(() => {
      expect(editorValue(input)).toBe("hello 😄world");
      expect(editorSelection(input).start).toBe("hello 😄".length);
    });
  });

  test("keeps a text-node caret at offset zero after deleting at the start", async () => {
    const { changes } = renderHarness("abc");
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    inputFromUser(input, "bc", 0, { inputType: "deleteContentForward" });

    await waitFor(() => {
      expect(editorValue(input)).toBe("bc");
      expect(editorSelection(input)).toEqual({ start: 0, end: 0 });
    });
    await selectEmoji(":smile:", /emoji :smile:/i);

    await waitFor(() => {
      expect(editorValue(input)).toBe("😄bc");
      expect(editorSelection(input).start).toBe("😄".length);
    });
    expect(changes).toContain("bc");
    expect(changes.at(-1)).toBe("😄bc");
  });

  test("treats only an editor-root offset-zero selection after scripted fill as caret-at-end", async () => {
    renderHarness();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    act(() => {
      input.textContent = "filled";
      setDomSelection(input, 0);
      fireEvent.input(input, { inputType: "insertText", data: "filled" });
    });

    await waitFor(() => {
      expect(editorValue(input)).toBe("filled");
      expect(editorSelection(input)).toEqual({ start: "filled".length, end: "filled".length });
    });

    await selectEmoji(":smile:", /emoji :smile:/i);
    await waitFor(() => expect(editorValue(input)).toBe("filled😄"));
  });

  test("keeps the composer stable while typing a custom emoji shortcode prefix", async () => {
    const user = userEvent.setup();
    const { changes } = renderHarnessWithCustomEmojis();
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

    await user.click(input);
    await user.keyboard(":taco");

    await waitFor(() => {
      expect(editorValue(input)).toBe(":taco");
      expect(editorSelection(input).start).toBe(":taco".length);
    });
    expect(changes).toContain(":t");
    expect(changes.at(-1)).toBe(":taco");
  });

  test("inserts active custom emoji markers from autocomplete and picker", async () => {
    const { changes } = renderHarnessWithCustomEmojis("hello world");
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;

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
      expect(editorValue(input)).toBe("hello <:party:123>");
      expect(screen.getByRole("img", { name: /custom emoji :party:/i })).toBeInTheDocument();
    });
    expect(screen.queryByText("<:party:123>")).toBeNull();
    expect(changes).toContain("hello <:party:123>");

    inputFromUser(input, "hello :retired:");
    await waitFor(() => {
      expect(editorValue(input)).toBe("hello :retired:");
    });

    inputFromUser(input, "hello :dance:");
    await waitFor(() => {
      expect(editorValue(input)).toBe("hello <a:dance:456>");
    });
    expect(changes).toContain("hello <a:dance:456>");

    setInputSelection(input, "hello ".length);
    await selectEmoji("party", /emoji :party:/i);

    await waitFor(() => {
      expect(editorValue(input)).toBe("hello <:party:123><a:dance:456>");
    });
  });

  test("replaces the selected text with a selected emoji", async () => {
    renderHarness("hello world");
    const input = screen.getByLabelText(/compose message/i) as HTMLDivElement;
    setInputSelection(input, "hello ".length, "hello world".length);

    await selectEmoji("heart", /emoji :heart:/i);

    await waitFor(() => {
      expect(editorValue(input)).toBe("hello ❤️");
      expect(editorSelection(input).start).toBe("hello ❤️".length);
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
