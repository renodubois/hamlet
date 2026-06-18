import type { PublicUser } from "../api";

export interface MentionMarkerToken {
  type: "mention";
  marker: string;
  id: number;
}

export type MentionTextToken = { type: "text"; value: string } | MentionMarkerToken;

export interface TextSelectionRange {
  start: number;
  end: number;
}

export interface MentionAutocompleteToken extends TextSelectionRange {
  query: string;
}

export interface MentionReplacement {
  value: string;
  caretIndex: number;
  marker: string;
}

type MentionSearchRank = "exact" | "prefix" | "substring" | "fuzzy" | "empty";
type MentionSearchFieldRank = "username" | "display_name" | "empty";

const MENTION_MARKER_RE = /<@(\d{1,15})>/g;
const MENTION_QUERY_PATTERN = /^[\p{L}\p{N}_.-]{0,64}$/u;
const MENTION_BOUNDARY_PUNCTUATION = new Set(["(", "[", "{", '"', "'", "`", "“", "‘"]);
const TRAILING_DURABLE_MARKER_RE = /(?:<@\d{1,15}>|<a?:[A-Za-z0-9_]{2,32}:\d{1,15}>)$/;

const MENTION_SEARCH_RANK_ORDER: Record<MentionSearchRank, number> = {
  exact: 0,
  prefix: 1,
  substring: 2,
  fuzzy: 3,
  empty: 4,
};

const MENTION_SEARCH_FIELD_ORDER: Record<MentionSearchFieldRank, number> = {
  username: 0,
  display_name: 1,
  empty: 2,
};

function clampIndex(index: number, value: string): number {
  return Math.min(Math.max(index, 0), value.length);
}

function normalizeSelection(selection: TextSelectionRange, value: string): TextSelectionRange {
  const start = clampIndex(selection.start, value);
  const end = clampIndex(selection.end, value);
  return start <= end ? { start, end } : { start: end, end: start };
}

function previousCodePoint(value: string): string | undefined {
  return Array.from(value).at(-1);
}

function hasValidMentionBoundary(value: string, tokenStart: number): boolean {
  if (tokenStart === 0) return true;

  const prefix = value.slice(0, tokenStart);
  const previous = previousCodePoint(prefix);
  if (!previous) return true;

  return (
    /\s/u.test(previous) ||
    MENTION_BOUNDARY_PUNCTUATION.has(previous) ||
    TRAILING_DURABLE_MARKER_RE.test(prefix)
  );
}

function fuzzyMatch(candidate: string, query: string): boolean {
  let queryIndex = 0;
  for (const char of candidate) {
    if (char === query[queryIndex]) queryIndex += 1;
    if (queryIndex >= query.length) return true;
  }
  return query.length === 0;
}

function scoreText(candidate: string, query: string): MentionSearchRank | null {
  if (candidate === query) return "exact";
  if (candidate.startsWith(query)) return "prefix";
  if (candidate.includes(query)) return "substring";
  if (fuzzyMatch(candidate, query)) return "fuzzy";
  return null;
}

function scoreUser(
  user: PublicUser,
  query: string,
): { rank: MentionSearchRank; field: MentionSearchFieldRank } | null {
  if (query.length === 0) return { rank: "empty", field: "empty" };

  const candidates: { rank: MentionSearchRank; field: MentionSearchFieldRank }[] = [];
  const usernameRank = scoreText(user.username.toLowerCase(), query);
  if (usernameRank) candidates.push({ rank: usernameRank, field: "username" });

  if (user.display_name) {
    const displayNameRank = scoreText(user.display_name.toLowerCase(), query);
    if (displayNameRank) candidates.push({ rank: displayNameRank, field: "display_name" });
  }

  candidates.sort((a, b) => {
    const rankDelta = MENTION_SEARCH_RANK_ORDER[a.rank] - MENTION_SEARCH_RANK_ORDER[b.rank];
    if (rankDelta !== 0) return rankDelta;
    return MENTION_SEARCH_FIELD_ORDER[a.field] - MENTION_SEARCH_FIELD_ORDER[b.field];
  });

  return candidates[0] ?? null;
}

function compareStrings(a: string, b: string): number {
  if (a < b) return -1;
  if (a > b) return 1;
  return 0;
}

export function mentionMarker(user: Pick<PublicUser, "id">): string {
  return `<@${user.id}>`;
}

export function mentionDisplayName(user: Pick<PublicUser, "username" | "display_name">): string {
  return user.display_name ?? user.username;
}

export function hydratedMentionsIncludeUser(
  mentions: readonly Pick<PublicUser, "id">[] | null | undefined,
  userId: number | null | undefined,
): boolean {
  return userId != null && (mentions?.some((mention) => mention.id === userId) ?? false);
}

export function messageMentionsCurrentUser(
  message: {
    deleted_at?: number | null;
    mentions?: readonly Pick<PublicUser, "id">[] | null;
  },
  currentUserId: number | null | undefined,
): boolean {
  return message.deleted_at == null && hydratedMentionsIncludeUser(message.mentions, currentUserId);
}

export function parseMentionMarkers(text: string): MentionTextToken[] {
  const tokens: MentionTextToken[] = [];
  let cursor = 0;

  MENTION_MARKER_RE.lastIndex = 0;
  let match: RegExpExecArray | null = MENTION_MARKER_RE.exec(text);
  while (match !== null) {
    const start = match.index;
    if (start > cursor) tokens.push({ type: "text", value: text.slice(cursor, start) });

    tokens.push({
      type: "mention",
      marker: match[0],
      id: Number(match[1]),
    });
    cursor = start + match[0].length;
    match = MENTION_MARKER_RE.exec(text);
  }

  if (cursor < text.length) tokens.push({ type: "text", value: text.slice(cursor) });
  return tokens;
}

export function findActiveMentionToken(
  value: string,
  currentSelection: TextSelectionRange,
): MentionAutocompleteToken | null {
  const selection = normalizeSelection(currentSelection, value);
  if (selection.start !== selection.end) return null;

  const tokenEnd = selection.start;
  const tokenStart = value.lastIndexOf("@", tokenEnd - 1);
  if (tokenStart < 0) return null;

  const query = value.slice(tokenStart + 1, tokenEnd);
  if (!MENTION_QUERY_PATTERN.test(query)) return null;
  if (!hasValidMentionBoundary(value, tokenStart)) return null;

  return { start: tokenStart, end: tokenEnd, query };
}

export function replaceMentionToken(
  value: string,
  token: MentionAutocompleteToken,
  user: Pick<PublicUser, "id">,
): MentionReplacement {
  const marker = mentionMarker(user);
  const suffixStart = value[token.end] === " " ? token.end + 1 : token.end;
  const replacement = `${marker} `;
  const nextValue = `${value.slice(0, token.start)}${replacement}${value.slice(suffixStart)}`;

  return {
    value: nextValue,
    caretIndex: token.start + replacement.length,
    marker,
  };
}

export function rankMentionUsers(
  users: readonly PublicUser[],
  rawQuery: string,
  limit = users.length,
): PublicUser[] {
  const query = rawQuery.trim().toLowerCase();

  return users
    .map((user) => ({ user, score: scoreUser(user, query) }))
    .filter(
      (
        entry,
      ): entry is {
        user: PublicUser;
        score: { rank: MentionSearchRank; field: MentionSearchFieldRank };
      } => entry.score !== null,
    )
    .sort((a, b) => {
      const rankDelta =
        MENTION_SEARCH_RANK_ORDER[a.score.rank] - MENTION_SEARCH_RANK_ORDER[b.score.rank];
      if (rankDelta !== 0) return rankDelta;

      const fieldDelta =
        MENTION_SEARCH_FIELD_ORDER[a.score.field] - MENTION_SEARCH_FIELD_ORDER[b.score.field];
      if (fieldDelta !== 0) return fieldDelta;

      const usernameDelta = compareStrings(
        a.user.username.toLowerCase(),
        b.user.username.toLowerCase(),
      );
      if (usernameDelta !== 0) return usernameDelta;

      return a.user.id - b.user.id;
    })
    .slice(0, limit)
    .map((entry) => entry.user);
}
