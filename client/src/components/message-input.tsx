import {
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  createUniqueId,
  onCleanup,
  untrack,
  type JSX,
} from "solid-js";
import { getServerUrl, type CustomEmoji } from "../api";
import { useOptionalCustomEmojis } from "../contexts/custom-emojis";
import { parseCustomEmojiMarkers, customEmojisToEntries } from "../emoji/custom-emojis";
import { CONSERVATIVE_EMOJIS } from "../emoji/emoji-data";
import { searchEmojiResults, type EmojiSearchResult } from "../emoji/emoji-search";
import {
  createEmojiShortcodeLookup,
  hasValidShortcodeBoundary,
  replaceCompletedEmojiShortcodeBeforeCaret,
} from "../emoji/emoji-shortcodes";
import EmojiPicker from "./emoji-picker";
import { EmojiIcon } from "./icons";

interface SelectionRange {
  start: number;
  end: number;
}

interface EmojiAutocompleteSession extends SelectionRange {
  query: string;
}

type EmojiAutocompleteToken = EmojiAutocompleteSession;

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
  describedBy?: string;
  class?: string;
  inputClass?: string;
  emojiButtonClass?: string;
  emojiButtonLabel?: string;
  inputRef?: (element: MessageEditorElement) => void;
  onKeyDown?: JSX.EventHandler<MessageEditorElement, KeyboardEvent>;
}

const DEFAULT_ROOT_CLASS = "flex min-w-0 flex-1 items-center gap-2";
const MULTILINE_INPUT_BEHAVIOR_CLASS =
  "relative min-h-[2.75rem] max-h-40 overflow-x-hidden overflow-y-auto whitespace-pre-wrap break-words [overflow-wrap:anywhere]";
const DEFAULT_INPUT_CLASS = `${MULTILINE_INPUT_BEHAVIOR_CLASS} w-full rounded-md bg-gray-100 p-4 focus:outline-none focus:ring-2 focus:ring-blue-400 empty:before:content-[attr(data-placeholder)] empty:before:text-gray-500`;
const DEFAULT_EMOJI_BUTTON_CLASS =
  "cursor-pointer rounded-md bg-gray-100 p-4 text-gray-700 hover:bg-gray-200 focus:outline-none focus:ring-2 focus:ring-blue-400";
const EMOJI_AUTOCOMPLETE_SESSION_QUERY_PATTERN = /^[A-Za-z0-9_+-]{0,32}$/;
const EMOJI_AUTOCOMPLETE_QUERY_PATTERN = /^[A-Za-z0-9_+-]{2,32}$/;
const MAX_EMOJI_AUTOCOMPLETE_RESULTS = 8;
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

function hasCompletedShortcodeAhead(value: string, tokenStart: number, tokenEnd: number): boolean {
  const closingColon = value.indexOf(":", tokenEnd);
  if (closingColon < 0) return false;

  const shortcodeBody = value.slice(tokenStart + 1, closingColon);
  return EMOJI_AUTOCOMPLETE_QUERY_PATTERN.test(shortcodeBody);
}

function findEmojiAutocompleteSession(
  value: string,
  currentSelection: SelectionRange,
): EmojiAutocompleteSession | null {
  const normalizedSelection = normalizeSelection(currentSelection, value);
  if (normalizedSelection.start !== normalizedSelection.end) return null;

  const tokenEnd = normalizedSelection.start;
  const tokenStart = value.lastIndexOf(":", tokenEnd - 1);
  if (tokenStart < 0) return null;

  const query = value.slice(tokenStart + 1, tokenEnd);
  if (!EMOJI_AUTOCOMPLETE_SESSION_QUERY_PATTERN.test(query)) return null;
  if (!hasValidShortcodeBoundary(value, tokenStart)) return null;
  if (hasCompletedShortcodeAhead(value, tokenStart, tokenEnd)) return null;

  return { start: tokenStart, end: tokenEnd, query };
}

function findEmojiAutocompleteToken(
  value: string,
  currentSelection: SelectionRange,
): EmojiAutocompleteToken | null {
  const session = findEmojiAutocompleteSession(value, currentSelection);
  if (!session || !EMOJI_AUTOCOMPLETE_QUERY_PATTERN.test(session.query)) return null;

  return session;
}

function emojiAutocompleteTokenKey(token: EmojiAutocompleteToken | null): string | null {
  return token ? `${token.start}:${token.end}:${token.query}` : null;
}

function emojiAutocompleteOptionLabel(result: EmojiSearchResult): string {
  return result.matchedAlias
    ? `Emoji ${result.canonicalShortcode}, also matches ${result.matchedAlias}`
    : `Emoji ${result.canonicalShortcode}`;
}

function resolveImageUrl(url: string): string {
  if (url.startsWith("http://") || url.startsWith("https://")) return url;
  return `${getServerUrl()}${url}`;
}

function markerForElement(node: Node): string | null {
  return node instanceof HTMLElement ? (node.dataset.emojiMarker ?? null) : null;
}

const BLOCK_BOUNDARY_ELEMENT_NAMES = new Set(["DIV", "P", "LI"]);

function isElementNamed(node: Node, names: ReadonlySet<string>): boolean {
  return node instanceof HTMLElement && names.has(node.tagName);
}

function isLineBreakElement(node: Node): boolean {
  return node instanceof HTMLBRElement;
}

function isBlockBoundaryElement(node: Node): boolean {
  return isElementNamed(node, BLOCK_BOUNDARY_ELEMENT_NAMES);
}

function shouldInsertBlockSeparator(
  previous: Node | null,
  next: Node | undefined,
  serializedPrefix: string,
): boolean {
  if (!previous || !next || serializedPrefix.endsWith("\n")) return false;
  return isBlockBoundaryElement(previous) || isBlockBoundaryElement(next);
}

function serializeChildren(parent: Node, limit = parent.childNodes.length): string {
  const children = Array.from(parent.childNodes).slice(0, limit);
  let value = "";
  let previous: Node | null = null;

  for (const child of children) {
    if (shouldInsertBlockSeparator(previous, child, value)) value += "\n";
    value += serializeNode(child);
    previous = child;
  }

  return value;
}

function serializeChildrenBeforeOffset(parent: Node, offset: number): string {
  const children = Array.from(parent.childNodes);
  const boundedOffset = Math.min(Math.max(offset, 0), children.length);
  let value = "";
  let previous: Node | null = null;

  for (let index = 0; index < boundedOffset; index += 1) {
    const child = children[index];
    if (shouldInsertBlockSeparator(previous, child, value)) value += "\n";
    value += serializeNode(child);
    previous = child;
  }

  const next = children[boundedOffset];
  if (shouldInsertBlockSeparator(previous, next, value)) value += "\n";

  return value;
}

function serializedLength(node: Node): number {
  return serializeNode(node).length;
}

function serializeEditor(root: HTMLElement): string {
  return serializeChildren(root);
}

function serializeNode(node: Node): string {
  const marker = markerForElement(node);
  if (marker) return marker;
  if (isLineBreakElement(node)) return "\n";
  if (node.nodeType === Node.TEXT_NODE) {
    return stripEditorCaretSentinels(node.textContent ?? "");
  }

  return serializeChildren(node);
}

function domOffsetForSerializedTextIndex(text: string, index: number): number {
  const boundedIndex = clampIndex(index, stripEditorCaretSentinels(text));
  let serializedOffset = 0;

  for (let domOffset = 0; domOffset < text.length; domOffset += 1) {
    if (serializedOffset >= boundedIndex) return domOffset;
    if (text[domOffset] !== EDITOR_CARET_SENTINEL) serializedOffset += 1;
  }

  return text.length;
}

function childOffset(parent: Node, child: Node): number {
  return Array.prototype.indexOf.call(parent.childNodes, child) as number;
}

function offsetWithinNode(container: Node, offset: number): number {
  const marker = markerForElement(container);
  if (marker) return offset <= 0 ? 0 : marker.length;
  if (isLineBreakElement(container)) return offset <= 0 ? 0 : 1;

  if (container.nodeType === Node.TEXT_NODE) {
    return stripEditorCaretSentinels((container.textContent ?? "").slice(0, offset)).length;
  }

  return serializeChildrenBeforeOffset(container, offset).length;
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
  let serializedPrefix = "";
  let previous: Node | null = null;

  for (const child of Array.from(parent.childNodes)) {
    const offset = childOffset(parent, child);

    if (shouldInsertBlockSeparator(previous, child, serializedPrefix)) {
      if (remaining === 0) return { node: parent, offset };
      if (remaining === 1) return { node: parent, offset };
      remaining -= 1;
      serializedPrefix += "\n";
    } else if (remaining === 0) {
      return { node: parent, offset };
    }

    const length = serializedLength(child);

    if (remaining < length) {
      const marker = markerForElement(child);
      if (marker) {
        return remaining > marker.length / 2
          ? { node: parent, offset: offset + 1 }
          : { node: parent, offset };
      }

      if (isLineBreakElement(child)) return { node: parent, offset };

      if (child.nodeType === Node.TEXT_NODE) {
        return {
          node: child,
          offset: domOffsetForSerializedTextIndex(child.textContent ?? "", remaining),
        };
      }

      const nested = findPositionInChildren(child, remaining);
      if (nested) return nested;
    }

    if (remaining === length) return { node: parent, offset: offset + 1 };

    remaining -= length;
    serializedPrefix += serializeNode(child);
    previous = child;
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

function appendTextWithLineBreaks(fragment: DocumentFragment, value: string) {
  const lines = value.split("\n");
  lines.forEach((line, index) => {
    if (line.length > 0) fragment.append(document.createTextNode(line));
    if (index < lines.length - 1) {
      fragment.append(document.createElement("br"));
      fragment.append(createEditorCaretSentinel());
    }
  });
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
      appendTextWithLineBreaks(fragment, token.value);
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
  const autocompleteListboxId = createUniqueId();
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
  const [selectedAutocompleteIndex, setSelectedAutocompleteIndex] = createSignal(0);
  const [dismissedAutocompleteSessionStart, setDismissedAutocompleteSessionStart] = createSignal<
    number | null
  >(null);
  const [selection, setSelection] = createSignal<SelectionRange>({
    start: props.value.length,
    end: props.value.length,
  });
  const autocompleteSession = createMemo(() =>
    findEmojiAutocompleteSession(props.value, selection()),
  );
  const autocompleteToken = createMemo(() => {
    const session = findEmojiAutocompleteToken(props.value, selection());
    const dismissedSessionStart = dismissedAutocompleteSessionStart();
    if (session && dismissedSessionStart !== null && session.start === dismissedSessionStart) {
      return null;
    }

    return session;
  });
  const autocompleteTokenKey = createMemo(() => emojiAutocompleteTokenKey(autocompleteToken()));
  const autocompleteSuggestions = createMemo(() => {
    const token = autocompleteToken();
    if (!token) return [];

    return searchEmojiResults(token.query, emojiEntries()).slice(0, MAX_EMOJI_AUTOCOMPLETE_RESULTS);
  });
  const autocompleteOpen = createMemo(() => autocompleteSuggestions().length > 0);
  const selectedAutocompleteSuggestion = () =>
    autocompleteSuggestions()[selectedAutocompleteIndex()] ?? autocompleteSuggestions()[0];
  const selectedAutocompleteOptionId = () =>
    autocompleteOpen()
      ? `${autocompleteListboxId}-option-${selectedAutocompleteIndex()}`
      : undefined;
  let inputRef: MessageEditorElement | undefined;
  let emojiButtonRef: HTMLButtonElement | undefined;
  let previousValue = props.value;
  let previousAutocompleteTokenKey: string | null = null;
  let stagedEditorValue: string | null = null;
  let disposed = false;

  onCleanup(() => {
    disposed = true;
  });

  createEffect(() => {
    const tokenKey = autocompleteTokenKey();

    if (tokenKey !== previousAutocompleteTokenKey) {
      previousAutocompleteTokenKey = tokenKey;
      setSelectedAutocompleteIndex(0);
    }

    const dismissedSessionStart = untrack(dismissedAutocompleteSessionStart);
    if (dismissedSessionStart === null) return;

    const session = autocompleteSession();
    if (!session || session.start !== dismissedSessionStart) {
      setDismissedAutocompleteSessionStart(null);
    }
  });

  createEffect(() => {
    const suggestionCount = autocompleteSuggestions().length;
    const selectedIndex = untrack(selectedAutocompleteIndex);

    if (suggestionCount === 0 || selectedIndex >= suggestionCount) {
      setSelectedAutocompleteIndex(0);
    }
  });

  createEffect(() => {
    if (autocompleteOpen()) setEmojiPickerOpen(false);
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

  const commitAutocompleteSuggestion = (suggestion = selectedAutocompleteSuggestion()): boolean => {
    const token = autocompleteToken();
    if (!token || !suggestion) return false;

    const emoji = suggestion.emoji.emoji;
    const nextValue = `${props.value.slice(0, token.start)}${emoji}${props.value.slice(token.end)}`;
    const caretIndex = token.start + emoji.length;

    setSelectedAutocompleteIndex(0);
    setDismissedAutocompleteSessionStart(null);
    updateValue(nextValue, { start: caretIndex, end: caretIndex }, { focusInput: true });
    return true;
  };

  const moveAutocompleteSelection = (delta: number) => {
    const suggestionCount = autocompleteSuggestions().length;
    if (suggestionCount === 0) return;

    setSelectedAutocompleteIndex(
      (currentIndex) => (currentIndex + delta + suggestionCount) % suggestionCount,
    );
  };

  const dismissAutocomplete = () => {
    const session = autocompleteSession();
    if (session) setDismissedAutocompleteSessionStart(session.start);
    setSelectedAutocompleteIndex(0);
  };

  const handleKeyDown: JSX.EventHandler<MessageEditorElement, KeyboardEvent> = (event) => {
    if (autocompleteOpen()) {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        dismissAutocomplete();
        return;
      }

      if (!event.altKey && !event.ctrlKey && !event.metaKey) {
        if (event.key === "ArrowDown") {
          event.preventDefault();
          event.stopPropagation();
          moveAutocompleteSelection(1);
          return;
        }

        if (event.key === "ArrowUp") {
          event.preventDefault();
          event.stopPropagation();
          moveAutocompleteSelection(-1);
          return;
        }

        if (event.key === "Tab" && !event.shiftKey) {
          rememberSelection();
          if (commitAutocompleteSuggestion()) {
            event.preventDefault();
            event.stopPropagation();
            return;
          }
        }

        if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
          rememberSelection();
          if (commitAutocompleteSuggestion()) {
            event.preventDefault();
            event.stopPropagation();
            return;
          }
        }
      }
    }

    props.onKeyDown?.(event);
    if (event.defaultPrevented) return;

    if (event.key === "Enter") {
      if (event.isComposing) return;

      event.preventDefault();

      if (event.shiftKey && !event.altKey && !event.ctrlKey && !event.metaKey) {
        const currentSelection = readInputSelection();
        const nextValue = `${props.value.slice(0, currentSelection.start)}\n${props.value.slice(
          currentSelection.end,
        )}`;
        const caretIndex = currentSelection.start + 1;
        updateValue(nextValue, { start: caretIndex, end: caretIndex }, { restoreSelection: true });
        return;
      }

      if (!event.shiftKey && !event.altKey && !event.ctrlKey && !event.metaKey) {
        event.currentTarget.closest("form")?.requestSubmit();
      }
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
      <div class="relative min-w-0 flex-1">
        <Show when={autocompleteOpen()}>
          <ul
            id={autocompleteListboxId}
            role="listbox"
            aria-label="Emoji suggestions"
            class="absolute bottom-full left-0 z-40 mb-2 max-h-64 w-full max-w-sm overflow-y-auto rounded-md border border-gray-200 bg-white py-1 text-sm shadow-lg"
          >
            <For each={autocompleteSuggestions()}>
              {(suggestion, index) => {
                const selected = () => index() === selectedAutocompleteIndex();
                return (
                  <li
                    id={`${autocompleteListboxId}-option-${index()}`}
                    role="option"
                    aria-label={emojiAutocompleteOptionLabel(suggestion)}
                    aria-selected={selected()}
                    class={`flex cursor-pointer items-center gap-3 px-3 py-2 ${
                      selected() ? "bg-blue-100 text-blue-900" : "text-gray-900 hover:bg-blue-50"
                    }`}
                    onMouseDown={(event) => {
                      event.preventDefault();
                      rememberSelection();
                      commitAutocompleteSuggestion(suggestion);
                    }}
                    onClick={() => commitAutocompleteSuggestion(suggestion)}
                  >
                    <span
                      aria-hidden="true"
                      class="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-gray-200 bg-gray-50 text-xl leading-none shadow-sm"
                    >
                      <Show
                        when={suggestion.emoji.kind === "custom" && suggestion.emoji.imageUrl}
                        fallback={suggestion.emoji.emoji}
                      >
                        {(imageUrl) => (
                          <img
                            src={resolveImageUrl(imageUrl())}
                            alt=""
                            class="h-7 w-7 object-contain"
                            draggable={false}
                          />
                        )}
                      </Show>
                    </span>
                    <span class="min-w-0 flex flex-col">
                      <span class="truncate font-semibold text-gray-950">
                        {suggestion.canonicalShortcode}
                      </span>
                      <Show when={suggestion.matchedAlias}>
                        {(matchedAlias) => (
                          <span class="truncate text-xs text-gray-500">
                            Also matches {matchedAlias()}
                          </span>
                        )}
                      </Show>
                    </span>
                  </li>
                );
              }}
            </For>
          </ul>
        </Show>
        <div
          ref={(el) => {
            inputRef = el as MessageEditorElement;
            if (props.placeholder) inputRef.setAttribute("placeholder", props.placeholder);
            attachCompatibilityProperties(inputRef);
            props.inputRef?.(inputRef);
          }}
          role="textbox"
          aria-multiline="true"
          aria-autocomplete="list"
          aria-controls={autocompleteOpen() ? autocompleteListboxId : undefined}
          aria-activedescendant={selectedAutocompleteOptionId()}
          class={
            props.inputClass
              ? `${MULTILINE_INPUT_BEHAVIOR_CLASS} ${props.inputClass}`
              : DEFAULT_INPUT_CLASS
          }
          aria-label={props.ariaLabel ?? "Message input"}
          aria-describedby={props.describedBy}
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
      </div>
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
          setEmojiPickerOpen((open) => {
            const nextOpen = !open;
            if (nextOpen) dismissAutocomplete();
            return nextOpen;
          });
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
