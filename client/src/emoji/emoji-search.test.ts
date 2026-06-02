import { describe, expect, test } from "vitest";
import { CONSERVATIVE_EMOJIS, type EmojiEntry } from "./emoji-data";
import { normalizeEmojiQuery, searchEmojiResults, searchEmojis } from "./emoji-search";

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

  test("ranks exact, prefix, then substring matches with stable ordering inside ties", () => {
    const rankedEmojis: readonly EmojiEntry[] = [
      { emoji: "💛", shortcodes: [":yellow_heart:"], category: "Smileys & Emotion" },
      { emoji: "❤️", shortcodes: [":heart:"], category: "Smileys & Emotion" },
      { emoji: "😍", shortcodes: [":heart_eyes:"], category: "Smileys & Emotion" },
      { emoji: "💚", shortcodes: [":green_heart:"], category: "Smileys & Emotion" },
      { emoji: "💙", shortcodes: [":blue_heart:"], category: "Smileys & Emotion" },
    ];

    expect(emojiGlyphs(searchEmojis("heart", rankedEmojis))).toEqual([
      "❤️",
      "😍",
      "💛",
      "💚",
      "💙",
    ]);
  });

  test("keeps built-in plain heart first for bare heart searches", () => {
    expect(emojiGlyphs(searchEmojis("heart", CONSERVATIVE_EMOJIS))[0]).toBe("❤️");
  });

  test("prioritizes custom emoji in exact and prefix ties without changing substring tie order", () => {
    const tiedEmojis: readonly EmojiEntry[] = [
      { kind: "native", emoji: "🎉", shortcodes: [":party:"], category: "Activities" },
      {
        kind: "custom",
        emoji: "<:party:123>",
        shortcodes: [":party:"],
        category: "Custom",
        id: 123,
        name: "party",
        marker: "<:party:123>",
        imageUrl: "/uploads/emojis/123.webp?v=1",
        animated: false,
        deletedAt: null,
      },
      { kind: "native", emoji: "🥳", shortcodes: [":party_face:"], category: "Smileys & Emotion" },
      {
        kind: "custom",
        emoji: "<:after_party:456>",
        shortcodes: [":after_party:"],
        category: "Custom",
        id: 456,
        name: "after_party",
        marker: "<:after_party:456>",
        imageUrl: "/uploads/emojis/456.webp?v=1",
        animated: false,
        deletedAt: null,
      },
    ];

    expect(emojiGlyphs(searchEmojis("party", tiedEmojis))).toEqual([
      "<:party:123>",
      "🎉",
      "🥳",
      "<:after_party:456>",
    ]);
    expect(emojiGlyphs(searchEmojis("arty", tiedEmojis))).toEqual([
      "🎉",
      "<:party:123>",
      "🥳",
      "<:after_party:456>",
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

  test("returns canonical shortcodes and matched aliases for explaining results", () => {
    expect(searchEmojiResults("+1", EMOJIS)[0]).toMatchObject({
      emoji: EMOJIS[2],
      canonicalShortcode: ":thumbsup:",
      matchedShortcode: ":+1:",
      matchedAlias: ":+1:",
      matchKind: "exact",
    });
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
