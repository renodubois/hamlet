import type { EmojiEntry } from "./emoji-data";

function stripBoundaryColons(value: string): string {
  return value.replace(/^:+|:+$/g, "");
}

function normalizeSeparators(value: string): string {
  if (/^[+-]\d+$/.test(value)) return value;
  return value.replace(/[\s_-]+/g, "");
}

export function normalizeEmojiQuery(query: string): string {
  return normalizeSeparators(stripBoundaryColons(query.trim().toLowerCase()));
}

export function searchEmojis(query: string, emojis: readonly EmojiEntry[]): readonly EmojiEntry[] {
  const normalized = normalizeEmojiQuery(query);
  if (!normalized) return emojis;

  return emojis.filter((emoji) =>
    emoji.shortcodes.some((shortcode) => normalizeEmojiQuery(shortcode).includes(normalized)),
  );
}
