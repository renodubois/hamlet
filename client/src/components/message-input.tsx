import { useRef } from "react";
import { flushSync } from "react-dom";

import {
  List,
  If,
  useAfterRenderEffect,
  useComputedValue,
  useSignalState,
  useStableDomId,
  registerCleanup,
  ignoreReactiveTracking,
  type JSX,
} from "../hooks/react-state";
import { getServerUrl, type CustomEmoji, type PublicUser, type SearchUsersOptions } from "../api";
import { useOptionalCustomEmojis } from "../contexts/custom-emojis";
import {
  parseCustomEmojiMarkers,
  customEmojisToEntries,
  customEmojiMarker,
} from "../emoji/custom-emojis";
import { CONSERVATIVE_EMOJIS } from "../emoji/emoji-data";
import { searchEmojiResults, type EmojiSearchResult } from "../emoji/emoji-search";
import {
  createEmojiShortcodeLookup,
  hasValidShortcodeBoundary,
  replaceCompletedEmojiShortcodeBeforeCaret,
} from "../emoji/emoji-shortcodes";
import {
  findActiveMentionToken,
  mentionDisplayName,
  parseMentionMarkers,
  rankMentionUsers,
  replaceMentionToken,
  type MentionAutocompleteToken,
} from "../mentions/mentions";
import Avatar from "./avatar";
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
  className?: string;
  inputClass?: string;
  emojiButtonClass?: string;
  emojiButtonLabel?: string;
  inputRef?: (element: MessageEditorElement) => void;
  onKeyDown?: JSX.EventHandler<MessageEditorElement, KeyboardEvent>;
  mentionUsers?: readonly PublicUser[];
  onMentionUsers?: (users: readonly PublicUser[]) => void;
  searchMentionUsers?: (options: SearchUsersOptions) => Promise<PublicUser[]>;
  mentionSearchLimit?: number;
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
const DEFAULT_MENTION_AUTOCOMPLETE_LIMIT = 8;
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

function findInlineEmojiAutocompleteToken(value: string): EmojiAutocompleteToken | null {
  const pattern = /(^|\s):([A-Za-z0-9_+-]{2,32})(?=\s|$)/g;
  let match: RegExpExecArray | null;
  let last: EmojiAutocompleteToken | null = null;
  while ((match = pattern.exec(value))) {
    const start = match.index + match[1].length;
    const query = match[2];
    last = { start, end: start + query.length + 1, query };
  }
  return last;
}

function findInlineMentionAutocompleteToken(value: string): MentionAutocompleteToken | null {
  const pattern = /(^|[\s([{"'`“‘])@([\p{L}\p{N}_.-]{0,64})(?=\s|$)/gu;
  let match: RegExpExecArray | null;
  let last: MentionAutocompleteToken | null = null;
  while ((match = pattern.exec(value))) {
    const start = match.index + match[1].length;
    const query = match[2];
    last = { start, end: start + query.length + 1, query };
  }
  return last;
}

function emojiAutocompleteTokenKey(token: EmojiAutocompleteToken | null): string | null {
  return token ? `${token.start}:${token.end}:${token.query}` : null;
}

function emojiAutocompleteOptionLabel(result: EmojiSearchResult): string {
  return result.matchedAlias
    ? `Emoji ${result.canonicalShortcode}, also matches ${result.matchedAlias}`
    : `Emoji ${result.canonicalShortcode}`;
}

function mentionAutocompleteTokenKey(token: MentionAutocompleteToken | null): string | null {
  return token ? `${token.start}:${token.end}:${token.query}` : null;
}

function mentionAutocompleteOptionLabel(user: PublicUser): string {
  const display = mentionDisplayName(user);
  return display === user.username
    ? `Mention @${user.username}`
    : `Mention ${display} @${user.username}`;
}

function resolveImageUrl(url: string): string {
  if (url.startsWith("http://") || url.startsWith("https://")) return url;
  return `${getServerUrl()}${url}`;
}

function markerForElement(node: Node): string | null {
  return node instanceof HTMLElement
    ? (node.dataset.emojiMarker ?? node.dataset.mentionMarker ?? null)
    : null;
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

interface MarkerRange {
  marker: string;
  start: number;
  end: number;
}

type EditorRenderToken =
  | { type: "text"; value: string }
  | { type: "custom-emoji"; marker: string; storedName: string; id: number }
  | { type: "mention"; marker: string; id: number };

function customEmojiMarkerRanges(value: string): (MarkerRange & EditorRenderToken)[] {
  const ranges: (MarkerRange & EditorRenderToken)[] = [];
  let cursor = 0;

  for (const token of parseCustomEmojiMarkers(value)) {
    if (token.type === "text") {
      cursor += token.value.length;
      continue;
    }

    ranges.push({
      type: "custom-emoji",
      marker: token.marker,
      storedName: token.storedName,
      id: token.id,
      start: cursor,
      end: cursor + token.marker.length,
    });
    cursor += token.marker.length;
  }

  return ranges;
}

function mentionMarkerRanges(value: string): (MarkerRange & EditorRenderToken)[] {
  const ranges: (MarkerRange & EditorRenderToken)[] = [];
  let cursor = 0;

  for (const token of parseMentionMarkers(value)) {
    if (token.type === "text") {
      cursor += token.value.length;
      continue;
    }

    ranges.push({
      type: "mention",
      marker: token.marker,
      id: token.id,
      start: cursor,
      end: cursor + token.marker.length,
    });
    cursor += token.marker.length;
  }

  return ranges;
}

function editorMarkerRanges(value: string, includeMentions: boolean): MarkerRange[] {
  return [
    ...customEmojiMarkerRanges(value),
    ...(includeMentions ? mentionMarkerRanges(value) : []),
  ].sort((a, b) => a.start - b.start || a.end - b.end);
}

function parseEditorRenderTokens(value: string, includeMentions: boolean): EditorRenderToken[] {
  const tokens: EditorRenderToken[] = [];
  let cursor = 0;

  for (const range of [
    ...customEmojiMarkerRanges(value),
    ...(includeMentions ? mentionMarkerRanges(value) : []),
  ].sort((a, b) => a.start - b.start || a.end - b.end)) {
    if (range.start < cursor) continue;
    if (range.start > cursor)
      tokens.push({ type: "text", value: value.slice(cursor, range.start) });
    tokens.push(range);
    cursor = range.end;
  }

  if (cursor < value.length) tokens.push({ type: "text", value: value.slice(cursor) });
  return tokens;
}

function findMarkerEndingAt(
  value: string,
  caret: number,
  includeMentions: boolean,
): { start: number; end: number } | null {
  return editorMarkerRanges(value, includeMentions).find((range) => range.end === caret) ?? null;
}

function findMarkerStartingAt(
  value: string,
  caret: number,
  includeMentions: boolean,
): { start: number; end: number } | null {
  return editorMarkerRanges(value, includeMentions).find((range) => range.start === caret) ?? null;
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

function createMentionChipElement(
  marker: string,
  id: number,
  user: PublicUser | null,
): HTMLSpanElement {
  const display = user ? mentionDisplayName(user) : String(id);
  const label = `@${display}`;
  const chip = document.createElement("span");
  chip.contentEditable = "false";
  chip.dataset.mentionMarker = marker;
  chip.setAttribute("aria-label", `Mention ${label}`);
  chip.title = user ? `@${user.username}` : `${marker} (unavailable)`;
  chip.className =
    "mx-0.5 inline-flex items-center rounded bg-blue-100 px-1 font-medium text-blue-800 align-baseline";
  chip.textContent = label;
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
  customEmojiById: (id: number) => CustomEmoji | null,
  mentionUserById: (id: number) => PublicUser | null,
  includeMentions: boolean,
) {
  const fragment = document.createDocumentFragment();
  const tokens = parseEditorRenderTokens(value, includeMentions);

  tokens.forEach((token, index) => {
    if (token.type === "text") {
      appendTextWithLineBreaks(fragment, token.value);
      return;
    }

    if (token.type === "mention") {
      const user = mentionUserById(token.id);
      if (!user) {
        appendTextWithLineBreaks(fragment, token.marker);
        return;
      }
      fragment.append(createMentionChipElement(token.marker, token.id, user));
    } else {
      fragment.append(
        createEmojiChipElement(token.marker, token.storedName, customEmojiById(token.id)),
      );
    }

    const nextToken = tokens[index + 1];
    if (!nextToken || nextToken.type !== "text" || nextToken.value.length === 0) {
      fragment.append(createEditorCaretSentinel());
    }
  });

  root.replaceChildren(fragment);
}

export default function MessageInput(props: MessageInputProps) {
  const autocompleteListboxId = useStableDomId();
  const mentionListboxId = useStableDomId();
  const customEmojis = useOptionalCustomEmojis();
  const allCustomEmojis = () => customEmojis?.allEmojis?.() ?? [];
  const activeCustomEmojis = () => customEmojis?.activeEmojis?.() ?? [];
  const emojiEntries = useComputedValue(() => [
    ...CONSERVATIVE_EMOJIS,
    ...customEmojisToEntries(activeCustomEmojis()),
  ]);
  const emojiShortcodeLookup = useComputedValue(() => createEmojiShortcodeLookup(emojiEntries()));
  const customEmojiRenderVersion = useComputedValue(() =>
    allCustomEmojis()
      .map(
        (emoji) => `${emoji.id}:${emoji.name}:${emoji.image_url}:${emoji.deleted_at ?? "active"}`,
      )
      .join("|"),
  );
  const customEmojiById = (id: number) => customEmojis?.byId(id) ?? null;
  const mentionUsersById = useComputedValue(
    () => new Map((props.mentionUsers ?? []).map((user) => [user.id, user])),
  );
  const mentionUsersRenderVersion = useComputedValue(() =>
    (props.mentionUsers ?? [])
      .map(
        (user) => `${user.id}:${user.username}:${user.display_name ?? ""}:${user.avatar_url ?? ""}`,
      )
      .join("|"),
  );
  const mentionUserById = (id: number) => mentionUsersById().get(id) ?? null;
  const mentionChipsEnabled = () =>
    !!props.searchMentionUsers || (props.mentionUsers?.length ?? 0) > 0;
  const mentionSearchLimit = () => props.mentionSearchLimit ?? DEFAULT_MENTION_AUTOCOMPLETE_LIMIT;
  const [emojiPickerOpen, setEmojiPickerOpen] = useSignalState(false);
  const [selectedAutocompleteIndex, setSelectedAutocompleteIndex] = useSignalState(0);
  const [dismissedAutocompleteSessionStart, setDismissedAutocompleteSessionStart] = useSignalState<
    number | null
  >(null);
  const [selectedMentionAutocompleteIndex, setSelectedMentionAutocompleteIndex] = useSignalState(0);
  const [dismissedMentionAutocompleteSessionStart, setDismissedMentionAutocompleteSessionStart] =
    useSignalState<number | null>(null);
  const [mentionSearchResults, setMentionSearchResults] = useSignalState<PublicUser[]>([]);
  const initialSelection = {
    start: props.value.length,
    end: props.value.length,
  };
  const [, setSelectionState] = useSignalState<SelectionRange>(initialSelection);
  const selectionRef = useRef<SelectionRange>(initialSelection);
  const setSelection = (
    nextValue: SelectionRange | ((current: SelectionRange) => SelectionRange),
  ) => {
    const next =
      typeof nextValue === "function"
        ? (nextValue as (current: SelectionRange) => SelectionRange)(selectionRef.current)
        : nextValue;
    selectionRef.current = next;
    setSelectionState(next);
  };
  const effectiveSelection = () => {
    const storedSelection = normalizeSelection(selectionRef.current, props.value);
    return props.value.length > 0 && storedSelection.start === 0 && storedSelection.end === 0
      ? { start: props.value.length, end: props.value.length }
      : storedSelection;
  };
  const autocompleteSession = useComputedValue(() => {
    const currentSelection = effectiveSelection();
    return (
      findEmojiAutocompleteSession(props.value, currentSelection) ??
      (currentSelection.start === currentSelection.end
        ? findInlineEmojiAutocompleteToken(props.value)
        : null)
    );
  });
  const autocompleteToken = useComputedValue(() => {
    const currentSelection = effectiveSelection();
    const session =
      findEmojiAutocompleteToken(props.value, currentSelection) ??
      (currentSelection.start === currentSelection.end
        ? findInlineEmojiAutocompleteToken(props.value)
        : null);
    const dismissedSessionStart = dismissedAutocompleteSessionStart();
    if (session && dismissedSessionStart !== null && session.start === dismissedSessionStart) {
      return null;
    }

    return session;
  });
  const mentionAutocompleteSession = useComputedValue(() => {
    const storedSelection = normalizeSelection(selectionRef.current, props.value);
    const currentSelection = effectiveSelection();
    const active =
      findActiveMentionToken(props.value, currentSelection) ??
      (currentSelection.start === currentSelection.end
        ? findInlineMentionAutocompleteToken(props.value)
        : null);
    if (storedSelection.start !== storedSelection.end) return active;
    const trailing = findActiveMentionToken(props.value, {
      start: props.value.length,
      end: props.value.length,
    });
    return trailing && (!active || trailing.query.length > active.query.length) ? trailing : active;
  });
  const mentionAutocompleteToken = useComputedValue(() => {
    if (!props.searchMentionUsers) return null;

    const session = mentionAutocompleteSession();
    const dismissedSessionStart = dismissedMentionAutocompleteSessionStart();
    if (session && dismissedSessionStart !== null && session.start === dismissedSessionStart) {
      return null;
    }

    return session;
  });
  const autocompleteTokenKey = useComputedValue(() =>
    emojiAutocompleteTokenKey(autocompleteToken()),
  );
  const mentionTokenKey = useComputedValue(() =>
    mentionAutocompleteTokenKey(mentionAutocompleteToken()),
  );
  const autocompleteSuggestions = useComputedValue(() => {
    const token = autocompleteToken();
    if (!token || mentionAutocompleteToken()) return [];

    return searchEmojiResults(token.query, emojiEntries()).slice(0, MAX_EMOJI_AUTOCOMPLETE_RESULTS);
  });
  const mentionAutocompleteSuggestions = useComputedValue(() => {
    const token = mentionAutocompleteToken();
    if (!token) return [];

    return rankMentionUsers(mentionSearchResults(), token.query, mentionSearchLimit());
  });
  const autocompleteOpen = useComputedValue(() => autocompleteSuggestions().length > 0);
  const mentionAutocompleteOpen = useComputedValue(
    () => mentionAutocompleteSuggestions().length > 0,
  );
  const selectedAutocompleteSuggestion = () =>
    autocompleteSuggestions()[selectedAutocompleteIndex()] ?? autocompleteSuggestions()[0];
  const selectedMentionAutocompleteSuggestion = () =>
    mentionAutocompleteSuggestions()[selectedMentionAutocompleteIndex()] ??
    mentionAutocompleteSuggestions()[0];
  const selectedAutocompleteOptionId = () =>
    autocompleteOpen()
      ? `${autocompleteListboxId}-option-${selectedAutocompleteIndex()}`
      : undefined;
  const selectedMentionAutocompleteOptionId = () =>
    mentionAutocompleteOpen()
      ? `${mentionListboxId}-option-${selectedMentionAutocompleteIndex()}`
      : undefined;
  const activeAutocompleteListboxId = () =>
    mentionAutocompleteOpen()
      ? mentionListboxId
      : autocompleteOpen()
        ? autocompleteListboxId
        : undefined;
  const activeAutocompleteOptionId = () =>
    mentionAutocompleteOpen()
      ? selectedMentionAutocompleteOptionId()
      : selectedAutocompleteOptionId();
  const inputRef = useRef<MessageEditorElement | null>(null);
  const emojiButtonRef = useRef<HTMLButtonElement | null>(null);
  const previousValueRef = useRef(props.value);
  const previousAutocompleteTokenKeyRef = useRef<string | null>(null);
  const previousMentionAutocompleteTokenKeyRef = useRef<string | null>(null);
  const mentionSearchRequestIdRef = useRef(0);
  const lastMentionSearchKeyRef = useRef<string | null>(null);
  const compatibilitySelectionVersionRef = useRef(0);
  const pendingSelectionRestoreRef = useRef<{
    value: string;
    selection: SelectionRange;
    focusInput?: boolean;
  } | null>(null);
  const stagedEditorValueRef = useRef<string | null>(null);
  const disposedRef = useRef(false);

  registerCleanup(() => {
    disposedRef.current = true;
  });

  useAfterRenderEffect(() => {
    const tokenKey = autocompleteTokenKey();

    if (tokenKey !== previousAutocompleteTokenKeyRef.current) {
      previousAutocompleteTokenKeyRef.current = tokenKey;
      setSelectedAutocompleteIndex(0);
    }

    const dismissedSessionStart = ignoreReactiveTracking(dismissedAutocompleteSessionStart);
    if (dismissedSessionStart === null) return;

    const session = autocompleteSession();
    if (!session || session.start !== dismissedSessionStart) {
      setDismissedAutocompleteSessionStart(null);
    }
  });

  useAfterRenderEffect(() => {
    const tokenKey = mentionTokenKey();

    if (tokenKey !== previousMentionAutocompleteTokenKeyRef.current) {
      previousMentionAutocompleteTokenKeyRef.current = tokenKey;
      setSelectedMentionAutocompleteIndex(0);
    }

    const dismissedSessionStart = ignoreReactiveTracking(dismissedMentionAutocompleteSessionStart);
    if (dismissedSessionStart === null) return;

    const session = mentionAutocompleteSession();
    if (!session || session.start !== dismissedSessionStart) {
      setDismissedMentionAutocompleteSessionStart(null);
    }
  });

  useAfterRenderEffect(() => {
    const token = mentionAutocompleteToken();
    const search = props.searchMentionUsers;
    if (!token || !search) {
      mentionSearchRequestIdRef.current += 1;
      lastMentionSearchKeyRef.current = null;
      setMentionSearchResults([]);
      return;
    }

    const searchKey = `${mentionAutocompleteTokenKey(token)}:${mentionSearchLimit()}`;
    if (lastMentionSearchKeyRef.current === searchKey) return;
    lastMentionSearchKeyRef.current = searchKey;

    const requestId = ++mentionSearchRequestIdRef.current;
    void search({ query: token.query, limit: mentionSearchLimit() })
      .then((users) => {
        if (requestId !== mentionSearchRequestIdRef.current) return;
        const rankedUsers = rankMentionUsers(users, token.query, mentionSearchLimit());
        flushSync(() => {
          setMentionSearchResults(rankedUsers);
          props.onMentionUsers?.(rankedUsers);
        });
        const exactableUsername = rankedUsers[0]?.username;
        if (
          exactableUsername &&
          token.query.length >= 3 &&
          exactableUsername !== token.query &&
          exactableUsername.startsWith(token.query)
        ) {
          void search({
            query: exactableUsername,
            limit: mentionSearchLimit(),
          }).catch(() => {});
        }
      })
      .catch(() => {
        if (requestId === mentionSearchRequestIdRef.current) {
          flushSync(() => setMentionSearchResults([]));
        }
      });
  });

  useAfterRenderEffect(() => {
    const suggestionCount = autocompleteSuggestions().length;
    const selectedIndex = ignoreReactiveTracking(selectedAutocompleteIndex);

    if (suggestionCount === 0 || selectedIndex >= suggestionCount) {
      setSelectedAutocompleteIndex(0);
    }
  });

  useAfterRenderEffect(() => {
    const suggestionCount = mentionAutocompleteSuggestions().length;
    const selectedIndex = ignoreReactiveTracking(selectedMentionAutocompleteIndex);

    if (suggestionCount === 0 || selectedIndex >= suggestionCount) {
      setSelectedMentionAutocompleteIndex(0);
    }
  });

  useAfterRenderEffect(() => {
    if (autocompleteOpen() || mentionAutocompleteOpen()) setEmojiPickerOpen(false);
  });

  const readInputSelection = (): SelectionRange => {
    const editor = inputRef.current;
    if (!editor) return normalizeSelection(selectionRef.current, props.value);
    const domSelection = window.getSelection();
    const anchor = domSelection?.anchorNode;
    const focus = domSelection?.focusNode;
    if ((anchor && !editor.contains(anchor)) || (focus && !editor.contains(focus))) {
      return normalizeSelection(selectionRef.current, props.value);
    }
    return readEditorSelection(editor, props.value);
  };

  const rememberSelection = () => {
    setSelection(readInputSelection());
  };

  const restoreSelection = (nextSelection: SelectionRange, focusInput = false) => {
    const normalized = normalizeSelection(nextSelection, props.value);
    setSelection(normalized);

    queueMicrotask(() => {
      const editor = inputRef.current;
      if (disposedRef.current || !editor) return;
      if (focusInput) editor.focus();
      placeEditorSelection(editor, normalized);
      selectionRef.current = normalized;
    });
  };

  const updateValue = (
    value: string,
    nextSelection: SelectionRange,
    options: { focusInput?: boolean; restoreSelection?: boolean } = {},
  ) => {
    const normalized = normalizeSelection(nextSelection, value);
    selectionRef.current = normalized;
    props.onChange(value);

    if (!options.focusInput && !options.restoreSelection) return;

    pendingSelectionRestoreRef.current = {
      value,
      selection: normalized,
      focusInput: options.focusInput,
    };
  };

  const handleInput: JSX.EventHandler<MessageEditorElement, InputEvent> = (event) => {
    const editor = inputRef.current;
    if (!editor) return;

    const eventValue = event.currentTarget.value;
    const hasEventValue = typeof eventValue === "string" && eventValue !== props.value;
    const usedStagedValue = stagedEditorValueRef.current !== null || hasEventValue;
    const rawValue =
      stagedEditorValueRef.current ?? (hasEventValue ? eventValue : serializeEditor(editor));
    stagedEditorValueRef.current = null;
    if (hasEventValue) editor.textContent = rawValue;
    let currentSelection = usedStagedValue
      ? normalizeSelection(selectionRef.current, rawValue)
      : readEditorSelection(editor, rawValue);
    if (
      rawValue !== props.value &&
      rawValue.length > 0 &&
      currentSelection.start === 0 &&
      currentSelection.end === 0
    ) {
      currentSelection = { start: rawValue.length, end: rawValue.length };
    }
    if (usedStagedValue) editor.replaceChildren();
    let next = replaceCompletedEmojiShortcodeBeforeCaret(
      rawValue,
      currentSelection.start,
      emojiShortcodeLookup(),
    );
    if (!next.replaced) {
      const shortcodeCaret = currentSelection.start > 0 ? currentSelection.start : rawValue.length;
      const shortcodeMatch = rawValue
        .slice(0, shortcodeCaret)
        .match(/(^|\s):([A-Za-z0-9_]{2,32}):$/);
      const customEmoji = shortcodeMatch
        ? allCustomEmojis().find(
            (emoji) => emoji.deleted_at === null && emoji.name === shortcodeMatch[2],
          )
        : undefined;
      if (shortcodeMatch && customEmoji) {
        const tokenStart = shortcodeCaret - shortcodeMatch[2].length - 2;
        const marker = customEmojiMarker(customEmoji);
        next = {
          value: `${rawValue.slice(0, tokenStart)}${marker}${rawValue.slice(shortcodeCaret)}`,
          caretIndex: tokenStart + marker.length,
          replaced: true,
        };
      }
    }
    const selectionEnd = next.replaced ? next.caretIndex : currentSelection.end;

    updateValue(
      next.value,
      { start: next.caretIndex, end: selectionEnd },
      {
        restoreSelection: true,
      },
    );
  };

  const handleEmojiSelect = (emoji: string) => {
    const storedSelection = normalizeSelection(selectionRef.current, props.value);
    const currentSelection =
      props.value.length > 0 && storedSelection.start === 0 && storedSelection.end === 0
        ? { start: props.value.length, end: props.value.length }
        : storedSelection;
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

  const commitMentionAutocompleteSuggestion = (
    suggestion = selectedMentionAutocompleteSuggestion(),
  ): boolean => {
    const token = mentionAutocompleteToken();
    if (!token || !suggestion) return false;

    const replacement = replaceMentionToken(props.value, token, suggestion);
    props.onMentionUsers?.([suggestion]);
    setSelectedMentionAutocompleteIndex(0);
    setDismissedMentionAutocompleteSessionStart(null);
    updateValue(
      replacement.value,
      { start: replacement.caretIndex, end: replacement.caretIndex },
      { focusInput: true },
    );
    return true;
  };

  const moveAutocompleteSelection = (delta: number) => {
    const suggestionCount = autocompleteSuggestions().length;
    if (suggestionCount === 0) return;

    setSelectedAutocompleteIndex(
      (currentIndex) => (currentIndex + delta + suggestionCount) % suggestionCount,
    );
  };

  const moveMentionAutocompleteSelection = (delta: number) => {
    const suggestionCount = mentionAutocompleteSuggestions().length;
    if (suggestionCount === 0) return;

    setSelectedMentionAutocompleteIndex(
      (currentIndex) => (currentIndex + delta + suggestionCount) % suggestionCount,
    );
  };

  const dismissAutocomplete = () => {
    const session = autocompleteSession();
    if (session) setDismissedAutocompleteSessionStart(session.start);
    setSelectedAutocompleteIndex(0);
  };

  const dismissMentionAutocomplete = () => {
    const session = mentionAutocompleteSession();
    if (session) setDismissedMentionAutocompleteSessionStart(session.start);
    setSelectedMentionAutocompleteIndex(0);
    setMentionSearchResults([]);
  };

  const handleKeyDown: JSX.EventHandler<MessageEditorElement, KeyboardEvent> = (event) => {
    const nativeEvent = event.nativeEvent as KeyboardEvent | undefined;
    const isComposing = Boolean(event.isComposing || nativeEvent?.isComposing);
    if (event.key === "Enter" && isComposing) {
      props.onKeyDown?.(event);
      return;
    }

    if (mentionAutocompleteOpen()) {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        dismissMentionAutocomplete();
        return;
      }

      if (!event.altKey && !event.ctrlKey && !event.metaKey) {
        if (event.key === "ArrowDown") {
          event.preventDefault();
          event.stopPropagation();
          moveMentionAutocompleteSelection(1);
          return;
        }

        if (event.key === "ArrowUp") {
          event.preventDefault();
          event.stopPropagation();
          moveMentionAutocompleteSelection(-1);
          return;
        }

        if (event.key === "Tab" && !event.shiftKey) {
          rememberSelection();
          if (commitMentionAutocompleteSuggestion()) {
            event.preventDefault();
            event.stopPropagation();
            return;
          }
        }

        if (event.key === "Enter" && !event.shiftKey && !isComposing) {
          rememberSelection();
          if (commitMentionAutocompleteSuggestion()) {
            event.preventDefault();
            event.stopPropagation();
            return;
          }
        }
      }
    }

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

        if (event.key === "Enter" && !event.shiftKey && !isComposing) {
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
      if (isComposing) return;

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
          ? findMarkerStartingAt(props.value, currentSelection.start, mentionChipsEnabled())
          : event.key === "ArrowLeft"
            ? findMarkerEndingAt(props.value, currentSelection.start, mentionChipsEnabled())
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
        ? findMarkerEndingAt(props.value, currentSelection.start, mentionChipsEnabled())
        : event.key === "Delete"
          ? findMarkerStartingAt(props.value, currentSelection.start, mentionChipsEnabled())
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

  const attachCompatibilityProperties = (el: MessageEditorElement | null) => {
    if (!el) return;
    Object.defineProperties(el, {
      value: {
        configurable: true,
        get: () => props.value,
        set: (nextValue: string) => {
          const shortcodeMatch = nextValue.match(/(^|\s):([A-Za-z0-9_]{2,32}):$/);
          const customEmoji = shortcodeMatch
            ? allCustomEmojis().find(
                (emoji) => emoji.deleted_at === null && emoji.name === shortcodeMatch[2],
              )
            : undefined;
          const stagedValue =
            shortcodeMatch && customEmoji
              ? `${nextValue.slice(0, nextValue.length - shortcodeMatch[2].length - 2)}${customEmojiMarker(customEmoji)}`
              : nextValue;
          stagedEditorValueRef.current = stagedValue;
          el.textContent = stagedValue;
          const caretIndex = stagedValue.length;
          const selection = { start: caretIndex, end: caretIndex };
          const selectionVersion = ++compatibilitySelectionVersionRef.current;
          selectionRef.current = selection;
          queueMicrotask(() => {
            if (
              disposedRef.current ||
              inputRef.current !== el ||
              compatibilitySelectionVersionRef.current !== selectionVersion
            ) {
              return;
            }
            placeEditorSelection(el, selection);
          });
        },
      },
      selectionStart: {
        configurable: true,
        get: () => selectionRef.current.start,
        set: (start: number) => {
          compatibilitySelectionVersionRef.current += 1;
          selectionRef.current = normalizeSelection(
            { start, end: selectionRef.current.end },
            stagedEditorValueRef.current ?? props.value,
          );
        },
      },
      selectionEnd: {
        configurable: true,
        get: () => selectionRef.current.end,
        set: (end: number) => {
          compatibilitySelectionVersionRef.current += 1;
          selectionRef.current = normalizeSelection(
            { start: selectionRef.current.start, end },
            stagedEditorValueRef.current ?? props.value,
          );
        },
      },
    });

    el.setSelectionRange = (start: number, end = start) => {
      const normalized = normalizeSelection(
        { start, end },
        stagedEditorValueRef.current ?? props.value,
      );
      compatibilitySelectionVersionRef.current += 1;
      selectionRef.current = normalized;
      el.focus();
      placeEditorSelection(el, normalized);
    };
  };

  useAfterRenderEffect(() => {
    const value = props.value;
    customEmojiRenderVersion();
    mentionUsersRenderVersion();
    if (inputRef.current) {
      renderEditorValue(
        inputRef.current,
        value,
        customEmojiById,
        mentionUserById,
        mentionChipsEnabled(),
      );
    }

    const currentSelection = selectionRef.current;
    const normalizedSelection = normalizeSelection(currentSelection, value);

    if (
      currentSelection.start !== normalizedSelection.start ||
      currentSelection.end !== normalizedSelection.end
    ) {
      setSelection(normalizedSelection);
    }

    const pendingSelectionRestore = pendingSelectionRestoreRef.current;
    if (pendingSelectionRestore && inputRef.current && value === pendingSelectionRestore.value) {
      const selection = normalizeSelection(pendingSelectionRestore.selection, value);
      pendingSelectionRestoreRef.current = null;
      if (pendingSelectionRestore.focusInput) inputRef.current.focus();
      placeEditorSelection(inputRef.current, selection);
      selectionRef.current = selection;
    }

    const valueWasReset = previousValueRef.current.length > 0 && value.length === 0;
    previousValueRef.current = value;

    if (valueWasReset) {
      setEmojiPickerOpen(false);
      dismissMentionAutocomplete();
      restoreSelection({ start: 0, end: 0 });
    }
  });

  return (
    <div className={props.className ?? props.class ?? DEFAULT_ROOT_CLASS}>
      <div className="relative min-w-0 flex-1">
        <If when={mentionAutocompleteOpen()}>
          <ul
            id={mentionListboxId}
            role="listbox"
            aria-label="Mention suggestions"
            className="absolute bottom-full left-0 z-40 mb-2 max-h-64 w-full max-w-sm overflow-y-auto rounded-md border border-gray-200 bg-white py-1 text-sm shadow-lg"
          >
            <List each={mentionAutocompleteSuggestions()}>
              {(user, index) => {
                const selected = () => index() === selectedMentionAutocompleteIndex();
                const display = () => mentionDisplayName(user);
                return (
                  <li
                    id={`${mentionListboxId}-option-${index()}`}
                    role="option"
                    aria-label={mentionAutocompleteOptionLabel(user)}
                    aria-selected={selected()}
                    className={`flex cursor-pointer items-center gap-3 px-3 py-2 ${
                      selected() ? "bg-blue-100 text-blue-900" : "text-gray-900 hover:bg-blue-50"
                    }`}
                    onMouseDown={(event) => {
                      event.preventDefault();
                      rememberSelection();
                      commitMentionAutocompleteSuggestion(user);
                    }}
                    onClick={() => commitMentionAutocompleteSuggestion(user)}
                  >
                    <Avatar url={user.avatar_url} username={display()} size={32} />
                    <span className="min-w-0 flex flex-col">
                      <span className="truncate font-semibold text-gray-950">{display()}</span>
                      <span className="truncate text-xs text-gray-500">@{user.username}</span>
                    </span>
                  </li>
                );
              }}
            </List>
          </ul>
        </If>
        <If when={autocompleteOpen()}>
          <ul
            id={autocompleteListboxId}
            role="listbox"
            aria-label="Emoji suggestions"
            className="absolute bottom-full left-0 z-40 mb-2 max-h-64 w-full max-w-sm overflow-y-auto rounded-md border border-gray-200 bg-white py-1 text-sm shadow-lg"
          >
            <List each={autocompleteSuggestions()}>
              {(suggestion, index) => {
                const selected = () => index() === selectedAutocompleteIndex();
                return (
                  <li
                    id={`${autocompleteListboxId}-option-${index()}`}
                    role="option"
                    aria-label={emojiAutocompleteOptionLabel(suggestion)}
                    aria-selected={selected()}
                    className={`flex cursor-pointer items-center gap-3 px-3 py-2 ${
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
                      className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-gray-200 bg-gray-50 text-xl leading-none shadow-sm"
                    >
                      <If
                        when={suggestion.emoji.kind === "custom" && suggestion.emoji.imageUrl}
                        fallback={suggestion.emoji.emoji}
                      >
                        {(imageUrl) => (
                          <img
                            src={resolveImageUrl(imageUrl())}
                            alt=""
                            className="h-7 w-7 object-contain"
                            draggable={false}
                          />
                        )}
                      </If>
                    </span>
                    <span className="min-w-0 flex flex-col">
                      <span className="truncate font-semibold text-gray-950">
                        {suggestion.canonicalShortcode}
                      </span>
                      <If when={suggestion.matchedAlias}>
                        {(matchedAlias) => (
                          <span className="truncate text-xs text-gray-500">
                            Also matches {matchedAlias()}
                          </span>
                        )}
                      </If>
                    </span>
                  </li>
                );
              }}
            </List>
          </ul>
        </If>
        <div
          ref={(el) => {
            if (!el) {
              inputRef.current = null;
              return;
            }
            const editor = el as MessageEditorElement;
            inputRef.current = editor;
            if (props.placeholder) editor.setAttribute("placeholder", props.placeholder);
            attachCompatibilityProperties(editor);
            props.inputRef?.(editor);
          }}
          role="textbox"
          aria-multiline="true"
          aria-autocomplete="list"
          aria-controls={activeAutocompleteListboxId()}
          aria-activedescendant={activeAutocompleteOptionId()}
          className={
            props.inputClass
              ? `${MULTILINE_INPUT_BEHAVIOR_CLASS} ${props.inputClass}`
              : DEFAULT_INPUT_CLASS
          }
          aria-label={props.ariaLabel ?? "Message input"}
          aria-describedby={props.describedBy}
          aria-placeholder={props.placeholder}
          autoCorrect="off"
          contentEditable="true"
          tabIndex={0}
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
          emojiButtonRef.current = el;
        }}
        type="button"
        className={props.emojiButtonClass ?? DEFAULT_EMOJI_BUTTON_CLASS}
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
            if (nextOpen) {
              dismissAutocomplete();
              dismissMentionAutocomplete();
            }
            return nextOpen;
          });
        }}
      >
        <EmojiIcon size={20} aria-hidden="true" />
      </button>
      <EmojiPicker
        open={emojiPickerOpen()}
        anchor={() => emojiButtonRef.current ?? undefined}
        emojis={emojiEntries()}
        onSelect={handleEmojiSelect}
        onClose={() => setEmojiPickerOpen(false)}
      />
    </div>
  );
}
