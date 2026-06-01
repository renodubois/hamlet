import type { CustomEmoji } from "../api";
import type { EmojiEntry } from "./emoji-data";

export interface CustomEmojiMarkerToken {
  type: "custom-emoji";
  marker: string;
  animated: boolean;
  storedName: string;
  id: number;
}

export type CustomEmojiTextToken = { type: "text"; value: string } | CustomEmojiMarkerToken;

export const CUSTOM_EMOJI_MARKER_RE = /<(a?):([A-Za-z0-9_]{2,32}):(\d{1,15})>/g;

export function customEmojiMarker(emoji: Pick<CustomEmoji, "id" | "name" | "animated">): string {
  return `<${emoji.animated ? "a" : ""}:${emoji.name}:${emoji.id}>`;
}

export function customEmojiToEntry(emoji: CustomEmoji): EmojiEntry {
  return {
    kind: "custom",
    emoji: customEmojiMarker(emoji),
    shortcodes: [`:${emoji.name}:`],
    category: "Custom",
    id: emoji.id,
    name: emoji.name,
    marker: customEmojiMarker(emoji),
    imageUrl: emoji.image_url,
    animated: emoji.animated,
    deletedAt: emoji.deleted_at,
  };
}

export function customEmojisToEntries(emojis: readonly CustomEmoji[]): EmojiEntry[] {
  return emojis.map(customEmojiToEntry);
}

export function parseCustomEmojiMarkers(text: string): CustomEmojiTextToken[] {
  const tokens: CustomEmojiTextToken[] = [];
  let cursor = 0;

  CUSTOM_EMOJI_MARKER_RE.lastIndex = 0;
  let match: RegExpExecArray | null = CUSTOM_EMOJI_MARKER_RE.exec(text);
  while (match !== null) {
    const start = match.index;
    if (start > cursor) {
      tokens.push({ type: "text", value: text.slice(cursor, start) });
    }

    tokens.push({
      type: "custom-emoji",
      marker: match[0],
      animated: match[1] === "a",
      storedName: match[2],
      id: Number(match[3]),
    });
    cursor = start + match[0].length;
    match = CUSTOM_EMOJI_MARKER_RE.exec(text);
  }

  if (cursor < text.length) {
    tokens.push({ type: "text", value: text.slice(cursor) });
  }

  return tokens;
}
