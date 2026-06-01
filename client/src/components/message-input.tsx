import { createEffect, createMemo, createSignal, onCleanup, untrack, type JSX } from "solid-js";
import { getServerUrl, type CustomEmoji } from "../api";
import { useOptionalCustomEmojis } from "../contexts/custom-emojis";
import { parseCustomEmojiMarkers, customEmojisToEntries } from "../emoji/custom-emojis";
import { CONSERVATIVE_EMOJIS } from "../emoji/emoji-data";
import {
  createEmojiShortcodeLookup,
  replaceCompletedEmojiShortcodeBeforeCaret,
} from "../emoji/emoji-shortcodes";
import EmojiPicker from "./emoji-picker";
import { EmojiIcon } from "./icons";

interface SelectionRange {
  start: number;
  end: number;
}

type MessageEditorElement = HTMLDivElement & {
  value?: string;
  selectionStart?: number;
  selectionEnd?: number;
  setSelectionRange?: (start: number, end?: number) => void;
};

export interface MessageInputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  ariaLabel?: string;
  class?: string;
  inputClass?: string;
  emojiButtonClass?: string;
  emojiButtonLabel?: string;
  inputRef?: (element: MessageEditorElement) => void;
  onKeyDown?: JSX.EventHandler<MessageEditorElement, KeyboardEvent>;
}

const DEFAULT_ROOT_CLASS = "flex min-w-0 flex-1 items-center gap-2";
const DEFAULT_INPUT_CLASS =
  "relative min-h-[3.5rem] w-full rounded-md bg-gray-100 p-4 whitespace-pre-wrap break-words focus:outline-none focus:ring-2 focus:ring-blue-400 empty:before:content-[attr(data-placeholder)] empty:before:text-gray-500";
const DEFAULT_EMOJI_BUTTON_CLASS =
  "cursor-pointer rounded-md bg-gray-100 p-4 text-gray-700 hover:bg-gray-200 focus:outline-none focus:ring-2 focus:ring-blue-400";
// Browsers struggle to place a caret after a contenteditable=false chip when
// it has no editable text after it. Keep an invisible editable text boundary in
// the DOM only where needed, then strip it from serialized text and offsets.
const EDITOR_CARET_SENTINEL = "\u200B";

function stripEditorCaretSentinels(value: string): string {
  return value.split(EDITOR_CARET_SENTINEL).join("");
}

function clampIndex(index: number, value: string): number {
  return Math.min(Math.max(index, 0), value.length);
}

function normalizeSelection(selection: SelectionRange, value: string): SelectionRange {
  const start = clampIndex(selection.start, value);
  const end = clampIndex(selection.end, value);

  return start <= end ? { start, end } : { start: end, end: start };
}

function resolveImageUrl(url: string): string {
  if (url.startsWith("http://") || url.startsWith("https://")) return url;
  return `${getServerUrl()}${url}`;
}

function markerForElement(node: Node): string | null {
  return node instanceof HTMLElement ? (node.dataset.emojiMarker ?? null) : null;
}

function serializedLength(node: Node): number {
  const marker = markerForElement(node);
  if (marker) return marker.length;
  if (node.nodeType === Node.TEXT_NODE) {
    return stripEditorCaretSentinels(node.textContent ?? "").length;
  }

  let length = 0;
  node.childNodes.forEach((child) => {
    length += serializedLength(child);
  });
  return length;
}

function serializeEditor(root: HTMLElement): string {
  let value = "";
  root.childNodes.forEach((child) => {
    value += serializeNode(child);
  });
  return value;
}

function serializeNode(node: Node): string {
  const marker = markerForElement(node);
  if (marker) return marker;
  if (node.nodeType === Node.TEXT_NODE) {
    return stripEditorCaretSentinels(node.textContent ?? "");
  }

  let value = "";
  node.childNodes.forEach((child) => {
    value += serializeNode(child);
  });
  return value;
}

function childOffset(parent: Node, child: Node): number {
  return Array.prototype.indexOf.call(parent.childNodes, child) as number;
}

function offsetWithinNode(container: Node, offset: number): number {
  const marker = markerForElement(container);
  if (marker) return offset <= 0 ? 0 : marker.length;

  if (container.nodeType === Node.TEXT_NODE) {
    return stripEditorCaretSentinels((container.textContent ?? "").slice(0, offset)).length;
  }

  let length = 0;
  const children = Array.from(container.childNodes).slice(0, offset);
  for (const child of children) length += serializedLength(child);
  return length;
}

function serializedOffset(root: HTMLElement, container: Node, offset: number): number {
  if (!root.contains(container)) return serializeEditor(root).length;

  let current: Node = container;
  let position = offsetWithinNode(current, offset);

  while (current !== root) {
    const parent = current.parentNode;
    if (!parent) break;

    const currentOffset = childOffset(parent, current);
    position += offsetWithinNode(parent, currentOffset);
    current = parent;
  }

  return position;
}

function readEditorSelection(root: HTMLElement, value: string): SelectionRange {
  const selection = window.getSelection();
  if (!selection || selection.rangeCount === 0) {
    return { start: value.length, end: value.length };
  }

  const anchor = selection.anchorNode;
  const focus = selection.focusNode;
  if ((anchor && !root.contains(anchor)) || (focus && !root.contains(focus))) {
    return { start: value.length, end: value.length };
  }

  const start = serializedOffset(root, anchor ?? root, selection.anchorOffset);
  const end = serializedOffset(root, focus ?? root, selection.focusOffset);
  return normalizeSelection({ start, end }, value);
}

interface DomPosition {
  node: Node;
  offset: number;
}

function positionForIndex(root: HTMLElement, index: number): DomPosition {
  const target = clampIndex(index, serializeEditor(root));
  const position = findPositionInChildren(root, target);
  return position ?? { node: root, offset: root.childNodes.length };
}

function findPositionInChildren(parent: Node, index: number): DomPosition | null {
  let remaining = index;

  for (const child of Array.from(parent.childNodes)) {
    const length = serializedLength(child);

    if (remaining === 0) return { node: parent, offset: childOffset(parent, child) };

    if (remaining < length) {
      const marker = markerForElement(child);
      if (marker) {
        return remaining > marker.length / 2
          ? { node: parent, offset: childOffset(parent, child) + 1 }
          : { node: parent, offset: childOffset(parent, child) };
      }

      if (child.nodeType === Node.TEXT_NODE) return { node: child, offset: remaining };

      const nested = findPositionInChildren(child, remaining);
      if (nested) return nested;
    }

    if (remaining === length) return { node: parent, offset: childOffset(parent, child) + 1 };

    remaining -= length;
  }

  return { node: parent, offset: parent.childNodes.length };
}

function placeEditorSelection(root: HTMLElement, selection: SelectionRange) {
  const range = document.createRange();
  const start = positionForIndex(root, selection.start);
  const end = positionForIndex(root, selection.end);

  range.setStart(start.node, start.offset);
  range.setEnd(end.node, end.offset);

  const domSelection = window.getSelection();
  domSelection?.removeAllRanges();
  domSelection?.addRange(range);
}

function findMarkerEndingAt(value: string, caret: number): { start: number; end: number } | null {
  for (const token of parseCustomEmojiMarkers(value)) {
    if (token.type === "text") continue;
    const start = value.indexOf(token.marker, Math.max(0, caret - token.marker.length));
    if (start >= 0 && start + token.marker.length === caret) {
      return { start, end: caret };
    }
  }
  return null;
}

function findMarkerStartingAt(value: string, caret: number): { start: number; end: number } | null {
  const rest = value.slice(caret);
  const token = parseCustomEmojiMarkers(rest)[0];
  if (!token || token.type === "text") return null;
  return token.marker.length > 0 ? { start: caret, end: caret + token.marker.length } : null;
}

function createEditorCaretSentinel(): Text {
  return document.createTextNode(EDITOR_CARET_SENTINEL);
}

function createEmojiChipElement(
  marker: string,
  storedName: string,
  emoji: CustomEmoji | null,
): HTMLSpanElement {
  const label = `:${emoji?.name ?? storedName}:`;
  const chip = document.createElement("span");
  chip.contentEditable = "false";
  chip.dataset.emojiMarker = marker;
  chip.setAttribute("role", "img");
  chip.setAttribute("aria-label", `Custom emoji ${label}`);
  chip.title = emoji
    ? emoji.deleted_at === null
      ? label
      : `${label} (deleted)`
    : `${label} (unavailable)`;
  chip.className = "mx-0.5 inline-flex h-6 w-6 items-center justify-center align-text-bottom";

  if (!emoji) {
    const fallback = document.createElement("span");
    fallback.setAttribute("aria-hidden", "true");
    fallback.textContent = label;
    chip.append(fallback);
    return chip;
  }

  const image = document.createElement("img");
  image.src = resolveImageUrl(emoji.image_url);
  image.alt = "";
  image.className = "h-6 w-6 object-contain";
  image.draggable = false;
  image.setAttribute("aria-hidden", "true");
  chip.append(image);
  return chip;
}

function renderEditorValue(
  root: HTMLElement,
  value: string,
  byId: (id: number) => CustomEmoji | null,
) {
  const fragment = document.createDocumentFragment();

  const tokens = parseCustomEmojiMarkers(value);

  tokens.forEach((token, index) => {
    if (token.type === "text") {
      fragment.append(document.createTextNode(token.value));
      return;
    }

    fragment.append(createEmojiChipElement(token.marker, token.storedName, byId(token.id)));

    const nextToken = tokens[index + 1];
    if (!nextToken || nextToken.type !== "text" || nextToken.value.length === 0) {
      fragment.append(createEditorCaretSentinel());
    }
  });

  root.replaceChildren(fragment);
}

export default function MessageInput(props: MessageInputProps) {
  const customEmojis = useOptionalCustomEmojis();
  const allCustomEmojis = () => customEmojis?.allEmojis?.() ?? [];
  const activeCustomEmojis = () => customEmojis?.activeEmojis?.() ?? [];
  const emojiEntries = createMemo(() => [
    ...CONSERVATIVE_EMOJIS,
    ...customEmojisToEntries(activeCustomEmojis()),
  ]);
  const emojiShortcodeLookup = createMemo(() => createEmojiShortcodeLookup(emojiEntries()));
  const customEmojiRenderVersion = createMemo(() =>
    allCustomEmojis()
      .map(
        (emoji) => `${emoji.id}:${emoji.name}:${emoji.image_url}:${emoji.deleted_at ?? "active"}`,
      )
      .join("|"),
  );
  const customEmojiById = (id: number) => customEmojis?.byId(id) ?? null;
  const [emojiPickerOpen, setEmojiPickerOpen] = createSignal(false);
  const [selection, setSelection] = createSignal<SelectionRange>({
    start: props.value.length,
    end: props.value.length,
  });
  let inputRef: MessageEditorElement | undefined;
  let emojiButtonRef: HTMLButtonElement | undefined;
  let previousValue = props.value;
  let stagedEditorValue: string | null = null;
  let disposed = false;

  onCleanup(() => {
    disposed = true;
  });

  const readInputSelection = (): SelectionRange => {
    if (!inputRef) return normalizeSelection(selection(), props.value);
    const domSelection = window.getSelection();
    const anchor = domSelection?.anchorNode;
    const focus = domSelection?.focusNode;
    if ((anchor && !inputRef.contains(anchor)) || (focus && !inputRef.contains(focus))) {
      return normalizeSelection(selection(), props.value);
    }
    return readEditorSelection(inputRef, props.value);
  };

  const rememberSelection = () => {
    setSelection(readInputSelection());
  };

  const restoreSelection = (nextSelection: SelectionRange, focusInput = false) => {
    const normalized = normalizeSelection(nextSelection, props.value);
    setSelection(normalized);

    queueMicrotask(() => {
      if (disposed || !inputRef) return;
      if (focusInput) inputRef.focus();
      placeEditorSelection(inputRef, normalized);
      setSelection(normalized);
    });
  };

  const updateValue = (
    value: string,
    nextSelection: SelectionRange,
    options: { focusInput?: boolean; restoreSelection?: boolean } = {},
  ) => {
    props.onChange(value);
    const normalized = normalizeSelection(nextSelection, value);
    setSelection(normalized);

    if (!options.focusInput && !options.restoreSelection) return;

    queueMicrotask(() => {
      if (disposed || !inputRef) return;
      if (options.focusInput) inputRef.focus();
      placeEditorSelection(inputRef, normalized);
      setSelection(normalized);
    });
  };

  const handleInput = () => {
    if (!inputRef) return;

    const usedStagedValue = stagedEditorValue !== null;
    const rawValue = stagedEditorValue ?? serializeEditor(inputRef);
    stagedEditorValue = null;
    const currentSelection = readEditorSelection(inputRef, rawValue);
    if (usedStagedValue) inputRef.replaceChildren();
    const next = replaceCompletedEmojiShortcodeBeforeCaret(
      rawValue,
      currentSelection.start,
      emojiShortcodeLookup(),
    );
    const selectionEnd = next.replaced ? next.caretIndex : currentSelection.end;

    updateValue(
      next.value,
      { start: next.caretIndex, end: selectionEnd },
      {
        restoreSelection: !usedStagedValue || next.replaced,
      },
    );
  };

  const handleEmojiSelect = (emoji: string) => {
    const currentSelection = normalizeSelection(selection(), props.value);
    const nextValue = `${props.value.slice(0, currentSelection.start)}${emoji}${props.value.slice(
      currentSelection.end,
    )}`;
    const caretIndex = currentSelection.start + emoji.length;

    updateValue(nextValue, { start: caretIndex, end: caretIndex }, { focusInput: true });
  };

  const handleKeyDown: JSX.EventHandler<MessageEditorElement, KeyboardEvent> = (event) => {
    props.onKeyDown?.(event);
    if (event.defaultPrevented) return;

    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      event.currentTarget.closest("form")?.requestSubmit();
      return;
    }

    const currentSelection = readInputSelection();
    if (currentSelection.start !== currentSelection.end) return;

    if (!event.shiftKey && !event.altKey && !event.ctrlKey && !event.metaKey) {
      const navigationRange =
        event.key === "ArrowRight"
          ? findMarkerStartingAt(props.value, currentSelection.start)
          : event.key === "ArrowLeft"
            ? findMarkerEndingAt(props.value, currentSelection.start)
            : null;

      if (navigationRange) {
        event.preventDefault();
        const caretIndex = event.key === "ArrowRight" ? navigationRange.end : navigationRange.start;
        restoreSelection({ start: caretIndex, end: caretIndex }, true);
        return;
      }
    }

    const markerRange =
      event.key === "Backspace"
        ? findMarkerEndingAt(props.value, currentSelection.start)
        : event.key === "Delete"
          ? findMarkerStartingAt(props.value, currentSelection.start)
          : null;

    if (!markerRange) return;

    event.preventDefault();
    const nextValue = `${props.value.slice(0, markerRange.start)}${props.value.slice(
      markerRange.end,
    )}`;
    updateValue(
      nextValue,
      { start: markerRange.start, end: markerRange.start },
      { restoreSelection: true },
    );
  };

  const attachCompatibilityProperties = (el: MessageEditorElement) => {
    Object.defineProperties(el, {
      value: {
        configurable: true,
        get: () => props.value,
        set: (nextValue: string) => {
          stagedEditorValue = nextValue;
          el.textContent = nextValue;
        },
      },
      selectionStart: {
        configurable: true,
        get: () => selection().start,
        set: (start: number) => {
          restoreSelection({ start, end: selection().end }, true);
        },
      },
      selectionEnd: {
        configurable: true,
        get: () => selection().end,
        set: (end: number) => {
          restoreSelection({ start: selection().start, end }, true);
        },
      },
    });

    el.setSelectionRange = (start: number, end = start) => {
      const normalized = normalizeSelection({ start, end }, stagedEditorValue ?? props.value);
      setSelection(normalized);
      el.focus();
      placeEditorSelection(el, normalized);
    };
  };

  createEffect(() => {
    const value = props.value;
    customEmojiRenderVersion();
    if (inputRef) renderEditorValue(inputRef, value, customEmojiById);

    const currentSelection = untrack(selection);
    const normalizedSelection = normalizeSelection(currentSelection, value);

    if (
      currentSelection.start !== normalizedSelection.start ||
      currentSelection.end !== normalizedSelection.end
    ) {
      setSelection(normalizedSelection);
    }

    const valueWasReset = previousValue.length > 0 && value.length === 0;
    previousValue = value;

    if (valueWasReset) {
      setEmojiPickerOpen(false);
      restoreSelection({ start: 0, end: 0 });
    }
  });

  return (
    <div class={props.class ?? DEFAULT_ROOT_CLASS}>
      <div
        ref={(el) => {
          inputRef = el as MessageEditorElement;
          if (props.placeholder) inputRef.setAttribute("placeholder", props.placeholder);
          attachCompatibilityProperties(inputRef);
          props.inputRef?.(inputRef);
        }}
        role="textbox"
        aria-multiline="false"
        class={props.inputClass ?? DEFAULT_INPUT_CLASS}
        aria-label={props.ariaLabel ?? "Message input"}
        aria-placeholder={props.placeholder}
        autocorrect="off"
        contenteditable="true"
        data-placeholder={props.placeholder}
        onInput={handleInput}
        onKeyDown={handleKeyDown}
        onSelect={rememberSelection}
        onKeyUp={rememberSelection}
        onMouseUp={rememberSelection}
        onClick={rememberSelection}
      />
      <button
        ref={(el) => {
          emojiButtonRef = el;
        }}
        type="button"
        class={props.emojiButtonClass ?? DEFAULT_EMOJI_BUTTON_CLASS}
        aria-label={props.emojiButtonLabel ?? "Open emoji picker"}
        aria-haspopup="dialog"
        aria-expanded={emojiPickerOpen()}
        title="Emoji"
        onMouseDown={(event) => {
          event.preventDefault();
          rememberSelection();
        }}
        onClick={() => {
          rememberSelection();
          setEmojiPickerOpen((open) => !open);
        }}
      >
        <EmojiIcon size={20} aria-hidden="true" />
      </button>
      <EmojiPicker
        open={emojiPickerOpen()}
        anchor={() => emojiButtonRef}
        emojis={emojiEntries()}
        onSelect={handleEmojiSelect}
        onClose={() => setEmojiPickerOpen(false)}
      />
    </div>
  );
}
