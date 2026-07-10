import type { Channel } from "../api";

export interface ChannelMarkerToken {
  type: "channel";
  marker: string;
  id: number;
}

export type ChannelTextToken = { type: "text"; value: string } | ChannelMarkerToken;

export interface TextSelectionRange {
  start: number;
  end: number;
}

export interface ChannelAutocompleteToken extends TextSelectionRange {
  query: string;
}

export interface ChannelReplacement {
  value: string;
  caretIndex: number;
  marker: string;
}

type ChannelSearchRank = "exact" | "prefix" | "substring" | "fuzzy" | "empty";

const CHANNEL_MARKER_RE = /<#(\d{1,15})>/g;
const CHANNEL_QUERY_PATTERN = /^[A-Za-z0-9_-]{0,64}$/;
const CHANNEL_BOUNDARY_PUNCTUATION = new Set(["(", "[", "{", '"', "'", "`", "“", "‘"]);
const TRAILING_DURABLE_MARKER_RE = /(?:<#\d{1,15}>|<@\d{1,15}>|<a?:[A-Za-z0-9_]{2,32}:\d{1,15}>)$/;

const CHANNEL_SEARCH_RANK_ORDER: Record<ChannelSearchRank, number> = {
  exact: 0,
  prefix: 1,
  substring: 2,
  fuzzy: 3,
  empty: 4,
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

function hasValidChannelBoundary(value: string, tokenStart: number): boolean {
  if (tokenStart === 0) return true;

  const prefix = value.slice(0, tokenStart);
  const previous = previousCodePoint(prefix);
  if (!previous) return true;

  return (
    /\s/u.test(previous) ||
    CHANNEL_BOUNDARY_PUNCTUATION.has(previous) ||
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

function scoreText(candidate: string, query: string): ChannelSearchRank | null {
  if (query.length === 0) return "empty";
  if (candidate === query) return "exact";
  if (candidate.startsWith(query)) return "prefix";
  if (candidate.includes(query)) return "substring";
  if (fuzzyMatch(candidate, query)) return "fuzzy";
  return null;
}

function compareStrings(a: string, b: string): number {
  return a.localeCompare(b, undefined, { sensitivity: "base" });
}

export function channelMentionMarker(channel: Pick<Channel, "id">): string {
  return `<#${channel.id}>`;
}

export function parseChannelMarkers(text: string): ChannelTextToken[] {
  const tokens: ChannelTextToken[] = [];
  let cursor = 0;

  CHANNEL_MARKER_RE.lastIndex = 0;
  let match: RegExpExecArray | null = CHANNEL_MARKER_RE.exec(text);
  while (match !== null) {
    const start = match.index;
    if (start > cursor) tokens.push({ type: "text", value: text.slice(cursor, start) });

    tokens.push({
      type: "channel",
      marker: match[0],
      id: Number(match[1]),
    });
    cursor = start + match[0].length;
    match = CHANNEL_MARKER_RE.exec(text);
  }

  if (cursor < text.length) tokens.push({ type: "text", value: text.slice(cursor) });
  return tokens;
}

export function findActiveChannelToken(
  value: string,
  currentSelection: TextSelectionRange,
): ChannelAutocompleteToken | null {
  const selection = normalizeSelection(currentSelection, value);
  if (selection.start !== selection.end) return null;

  const tokenEnd = selection.start;
  const tokenStart = value.lastIndexOf("#", tokenEnd - 1);
  if (tokenStart < 0) return null;

  const query = value.slice(tokenStart + 1, tokenEnd);
  if (!CHANNEL_QUERY_PATTERN.test(query)) return null;
  if (!hasValidChannelBoundary(value, tokenStart)) return null;

  return { start: tokenStart, end: tokenEnd, query };
}

export function replaceChannelToken(
  value: string,
  token: ChannelAutocompleteToken,
  channel: Pick<Channel, "id">,
): ChannelReplacement {
  const marker = channelMentionMarker(channel);
  const suffixStart = value[token.end] === " " ? token.end + 1 : token.end;
  const replacement = `${marker} `;
  const nextValue = `${value.slice(0, token.start)}${replacement}${value.slice(suffixStart)}`;

  return {
    value: nextValue,
    caretIndex: token.start + replacement.length,
    marker,
  };
}

export function rankChannelMentions(
  channels: readonly Channel[],
  rawQuery: string,
  limit = channels.length,
): Channel[] {
  const query = rawQuery.trim().toLowerCase();

  return channels
    .filter((channel) => channel.type === "text")
    .map((channel) => ({ channel, score: scoreText(channel.name.toLowerCase(), query) }))
    .filter(
      (entry): entry is { channel: Channel; score: ChannelSearchRank } => entry.score !== null,
    )
    .sort((a, b) => {
      const rankDelta = CHANNEL_SEARCH_RANK_ORDER[a.score] - CHANNEL_SEARCH_RANK_ORDER[b.score];
      if (rankDelta !== 0) return rankDelta;

      const positionDelta = a.channel.position - b.channel.position;
      if (positionDelta !== 0) return positionDelta;

      const nameDelta = compareStrings(a.channel.name, b.channel.name);
      if (nameDelta !== 0) return nameDelta;

      return a.channel.id - b.channel.id;
    })
    .slice(0, limit)
    .map((entry) => entry.channel);
}
