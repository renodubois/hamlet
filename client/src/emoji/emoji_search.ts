import type { EmojiEntry } from "./emoji_data";

export function normalizeEmojiQuery(query: string): string {
  return query.trim().toLowerCase().replace(/\s+/g, " ");
}

function stripBoundaryColons(value: string): string {
  return value.replace(/^:+|:+$/g, "");
}

function searchableTerms(emoji: EmojiEntry): string[] {
  const shortcodes = emoji.shortcodes.flatMap((shortcode) => [
    shortcode,
    stripBoundaryColons(shortcode),
  ]);
  return [emoji.name, ...emoji.keywords, ...shortcodes].map(normalizeEmojiQuery);
}

export function searchEmojis(query: string, emojis: readonly EmojiEntry[]): readonly EmojiEntry[] {
  const normalized = normalizeEmojiQuery(query);
  if (!normalized) return emojis;

  const queryTerms = [normalized, stripBoundaryColons(normalized)].filter(
    (term, index, terms) => term.length > 0 && terms.indexOf(term) === index,
  );

  return emojis.filter((emoji) => {
    const terms = searchableTerms(emoji);
    return queryTerms.some((queryTerm) => terms.some((term) => term.includes(queryTerm)));
  });
}
