import type { EmojiEntry } from "./emoji-data";

export type EmojiSearchMatchKind = "none" | "exact" | "prefix" | "substring";

export interface EmojiSearchResult {
  emoji: EmojiEntry;
  canonicalShortcode: string;
  matchedShortcode: string;
  matchedAlias?: string;
  matchKind: EmojiSearchMatchKind;
}

const MATCH_RANK: Record<EmojiSearchMatchKind, number> = {
  exact: 0,
  prefix: 1,
  substring: 2,
  none: 3,
};

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

function getMatchKind(
  normalizedShortcode: string,
  normalizedQuery: string,
): Exclude<EmojiSearchMatchKind, "none"> | null {
  if (normalizedShortcode === normalizedQuery) return "exact";
  if (normalizedShortcode.startsWith(normalizedQuery)) return "prefix";
  if (normalizedShortcode.includes(normalizedQuery)) return "substring";
  return null;
}

function isCustomEmojiTieBreak(kind: EmojiSearchMatchKind): boolean {
  return kind === "exact" || kind === "prefix";
}

export function searchEmojiResults(
  query: string,
  emojis: readonly EmojiEntry[],
): readonly EmojiSearchResult[] {
  const normalized = normalizeEmojiQuery(query);

  if (!normalized) {
    return emojis.map((emoji) => {
      const canonicalShortcode = emoji.shortcodes[0] ?? "";
      return {
        emoji,
        canonicalShortcode,
        matchedShortcode: canonicalShortcode,
        matchKind: "none",
      };
    });
  }

  const matches: { result: EmojiSearchResult; originalIndex: number }[] = [];

  emojis.forEach((emoji, originalIndex) => {
    const canonicalShortcode = emoji.shortcodes[0] ?? "";
    let bestMatch: Exclude<EmojiSearchMatchKind, "none"> | null = null;
    let matchedShortcode = "";

    for (const shortcode of emoji.shortcodes) {
      const matchKind = getMatchKind(normalizeEmojiQuery(shortcode), normalized);
      if (!matchKind) continue;

      if (!bestMatch || MATCH_RANK[matchKind] < MATCH_RANK[bestMatch]) {
        bestMatch = matchKind;
        matchedShortcode = shortcode;
      }
    }

    if (!bestMatch) return;

    matches.push({
      result: {
        emoji,
        canonicalShortcode,
        matchedShortcode,
        matchedAlias: matchedShortcode !== canonicalShortcode ? matchedShortcode : undefined,
        matchKind: bestMatch,
      },
      originalIndex,
    });
  });

  return matches
    .sort((a, b) => {
      const rankDelta = MATCH_RANK[a.result.matchKind] - MATCH_RANK[b.result.matchKind];
      if (rankDelta !== 0) return rankDelta;

      if (isCustomEmojiTieBreak(a.result.matchKind)) {
        const aCustom = a.result.emoji.kind === "custom";
        const bCustom = b.result.emoji.kind === "custom";
        if (aCustom !== bCustom) return aCustom ? -1 : 1;
      }

      return a.originalIndex - b.originalIndex;
    })
    .map(({ result }) => result);
}

export function searchEmojis(query: string, emojis: readonly EmojiEntry[]): readonly EmojiEntry[] {
  return searchEmojiResults(query, emojis).map((result) => result.emoji);
}
