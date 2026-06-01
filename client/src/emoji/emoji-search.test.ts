import { describe, expect, test } from "vitest";
import { CONSERVATIVE_EMOJIS, type EmojiEntry } from "./emoji-data";
import { normalizeEmojiQuery, searchEmojis } from "./emoji-search";

const EMOJIS: readonly EmojiEntry[] = [
  {
    emoji: "😄",
    shortcodes: [":smile:"],
    category: "Smileys & Emotion",
  },
  {
    emoji: "❤️",
    shortcodes: [":heart:"],
    category: "Smileys & Emotion",
  },
  {
    emoji: "👍",
    shortcodes: [":thumbsup:", ":+1:"],
    category: "People & Body",
  },
  {
    emoji: "👎",
    shortcodes: [":thumbsdown:", ":-1:"],
    category: "People & Body",
  },
  {
    emoji: "✅",
    shortcodes: [":white_check_mark:"],
    category: "Symbols",
  },
  {
    kind: "custom",
    emoji: "<:party_parrot:123>",
    shortcodes: [":party_parrot:"],
    category: "Custom",
    id: 123,
    name: "party_parrot",
    marker: "<:party_parrot:123>",
    imageUrl: "/uploads/emojis/123.webp?v=1",
    animated: false,
    deletedAt: null,
  },
];

const emojiGlyphs = (emojis: readonly EmojiEntry[]) => emojis.map((emoji) => emoji.emoji);

describe("emoji search", () => {
  test("normalizes case, boundary colons, and separators", () => {
    expect(normalizeEmojiQuery("  :White Check-Mark:  ")).toBe("whitecheckmark");
  });

  test("empty query and colon-only query return every emoji", () => {
    expect(searchEmojis("", EMOJIS)).toEqual(EMOJIS);
    expect(searchEmojis("   ", EMOJIS)).toEqual(EMOJIS);
    expect(searchEmojis(":", EMOJIS)).toEqual(EMOJIS);
  });

  test("matches shortcodes with and without boundary colons", () => {
    expect(emojiGlyphs(searchEmojis(":smile:", EMOJIS))).toEqual(["😄"]);
    expect(emojiGlyphs(searchEmojis("smile", EMOJIS))).toEqual(["😄"]);
    expect(emojiGlyphs(searchEmojis(":smile", EMOJIS))).toEqual(["😄"]);
    expect(emojiGlyphs(searchEmojis("smile:", EMOJIS))).toEqual(["😄"]);
  });

  test("keeps built-in plain and yellow hearts first for bare heart searches", () => {
    expect(emojiGlyphs(searchEmojis("heart", CONSERVATIVE_EMOJIS)).slice(0, 2)).toEqual([
      "❤️",
      "💛",
    ]);
  });

  test("matches partial shortcodes", () => {
    expect(emojiGlyphs(searchEmojis("smi", EMOJIS))).toEqual(["😄"]);
    expect(emojiGlyphs(searchEmojis("thumb", EMOJIS))).toEqual(["👍", "👎"]);
  });

  test("matches custom emoji shortcodes with the native search normalization", () => {
    expect(emojiGlyphs(searchEmojis("party parrot", EMOJIS))).toEqual(["<:party_parrot:123>"]);
    expect(emojiGlyphs(searchEmojis(":PARTY-PARROT:", EMOJIS))).toEqual(["<:party_parrot:123>"]);
  });

  test("matches symbolic shortcodes without collapsing signs", () => {
    expect(emojiGlyphs(searchEmojis("+1", EMOJIS))).toEqual(["👍"]);
    expect(emojiGlyphs(searchEmojis("-1", EMOJIS))).toEqual(["👎"]);
  });

  test("normalizes spaces, underscores, and hyphens as equivalent and optional", () => {
    expect(emojiGlyphs(searchEmojis("white_check_mark", EMOJIS))).toEqual(["✅"]);
    expect(emojiGlyphs(searchEmojis("white check mark", EMOJIS))).toEqual(["✅"]);
    expect(emojiGlyphs(searchEmojis("white-check-mark", EMOJIS))).toEqual(["✅"]);
    expect(emojiGlyphs(searchEmojis("whitecheckmark", EMOJIS))).toEqual(["✅"]);
  });

  test("is case-insensitive", () => {
    expect(emojiGlyphs(searchEmojis("WHITE CHECK", EMOJIS))).toEqual(["✅"]);
  });

  test("does not match names or keywords", () => {
    expect(searchEmojis("favorite", EMOJIS)).toEqual([]);
    expect(searchEmojis("red", EMOJIS)).toEqual([]);
  });

  test("does not strip internal colons", () => {
    expect(searchEmojis("sm:ile", EMOJIS)).toEqual([]);
  });

  test("returns an empty list when nothing matches", () => {
    expect(searchEmojis("does-not-exist", EMOJIS)).toEqual([]);
  });
});
