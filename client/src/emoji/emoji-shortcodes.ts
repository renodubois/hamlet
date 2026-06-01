import { CONSERVATIVE_EMOJIS, type EmojiEntry } from "./emoji-data";

export interface EmojiShortcodeReplacement {
  value: string;
  caretIndex: number;
  replaced: boolean;
}

const OPENING_PUNCTUATION_BOUNDARIES = new Set(["(", "[", "{", "<", '"', "'", "`", "“", "‘"]);
const TRAILING_EMOJI_MODIFIERS = /(?:[\uFE0E\uFE0F]|\p{Emoji_Modifier})+$/u;
const TRAILING_EMOJI_VARIATION_SELECTOR = /\uFE0F(?:\p{Emoji_Modifier})*$/u;
const TRAILING_CUSTOM_EMOJI_MARKER = /<(?:a?):[A-Za-z0-9_]{2,32}:\d{1,15}>$/;
const EMOJI_PRESENTATION_CHARACTER = /\p{Emoji_Presentation}/u;
const EXTENDED_PICTOGRAPHIC_CHARACTER = /\p{Extended_Pictographic}/u;

function clampCaretIndex(value: string, caretIndex: number): number {
  return Math.min(Math.max(caretIndex, 0), value.length);
}

function hasEmojiBoundary(value: string, boundaryIndex: number): boolean {
  const prefix = value.slice(0, boundaryIndex);
  const hasEmojiVariationSelector = TRAILING_EMOJI_VARIATION_SELECTOR.test(prefix);
  const normalizedPrefix = prefix.replace(TRAILING_EMOJI_MODIFIERS, "");
  const codePoints = Array.from(normalizedPrefix);
  const previousCodePoint = codePoints[codePoints.length - 1];

  return (
    previousCodePoint !== undefined &&
    (EMOJI_PRESENTATION_CHARACTER.test(previousCodePoint) ||
      (hasEmojiVariationSelector && EXTENDED_PICTOGRAPHIC_CHARACTER.test(previousCodePoint)))
  );
}

function hasValidShortcodeBoundary(value: string, tokenStart: number): boolean {
  if (tokenStart === 0) return true;

  const previousCharacter = value[tokenStart - 1];
  return (
    /\s/.test(previousCharacter) ||
    OPENING_PUNCTUATION_BOUNDARIES.has(previousCharacter) ||
    hasEmojiBoundary(value, tokenStart) ||
    TRAILING_CUSTOM_EMOJI_MARKER.test(value.slice(0, tokenStart))
  );
}

export function createEmojiShortcodeLookup(
  emojis: readonly EmojiEntry[] = CONSERVATIVE_EMOJIS,
): ReadonlyMap<string, string> {
  const lookup = new Map<string, string>();

  for (const entry of emojis) {
    for (const shortcode of entry.shortcodes) {
      const key = shortcode.toLowerCase();
      if (!lookup.has(key)) lookup.set(key, entry.emoji);
    }
  }

  return lookup;
}

const BUILT_IN_EMOJI_SHORTCODES = createEmojiShortcodeLookup();

export function lookupEmojiShortcode(
  shortcode: string,
  lookup: ReadonlyMap<string, string> = BUILT_IN_EMOJI_SHORTCODES,
): string | undefined {
  return lookup.get(shortcode.toLowerCase());
}

export function replaceCompletedEmojiShortcodeBeforeCaret(
  value: string,
  caretIndex = value.length,
  lookup: ReadonlyMap<string, string> = BUILT_IN_EMOJI_SHORTCODES,
): EmojiShortcodeReplacement {
  const boundedCaretIndex = clampCaretIndex(value, caretIndex);

  if (boundedCaretIndex < 3 || value[boundedCaretIndex - 1] !== ":") {
    return { value, caretIndex: boundedCaretIndex, replaced: false };
  }

  const emojis: string[] = [];
  let chainStart = boundedCaretIndex;
  let tokenEnd = boundedCaretIndex;

  while (tokenEnd >= 3 && value[tokenEnd - 1] === ":") {
    const tokenStart = value.lastIndexOf(":", tokenEnd - 2);
    if (tokenStart === -1) break;

    const token = value.slice(tokenStart, tokenEnd);
    const emoji = lookupEmojiShortcode(token, lookup);
    if (!emoji) break;

    emojis.unshift(emoji);
    chainStart = tokenStart;
    tokenEnd = tokenStart;
  }

  if (emojis.length === 0 || !hasValidShortcodeBoundary(value, chainStart)) {
    return { value, caretIndex: boundedCaretIndex, replaced: false };
  }

  const replacement = emojis.join("");
  const nextValue = `${value.slice(0, chainStart)}${replacement}${value.slice(boundedCaretIndex)}`;
  return {
    value: nextValue,
    caretIndex: chainStart + replacement.length,
    replaced: true,
  };
}
