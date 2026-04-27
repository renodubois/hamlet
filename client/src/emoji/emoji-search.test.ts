import { describe, expect, test } from "vitest";
import type { EmojiEntry } from "./emoji-data";
import { normalizeEmojiQuery, searchEmojis } from "./emoji-search";

const EMOJIS: readonly EmojiEntry[] = [
  {
    emoji: "😄",
    name: "grinning face with smiling eyes",
    shortcodes: [":smile:"],
    keywords: ["happy", "laugh"],
    category: "Smileys & Emotion",
  },
  {
    emoji: "❤️",
    name: "red heart",
    shortcodes: [":heart:"],
    keywords: ["love", "favorite"],
    category: "Smileys & Emotion",
  },
  {
    emoji: "👍",
    name: "thumbs up",
    shortcodes: [":thumbsup:", ":+1:"],
    keywords: ["approve", "yes"],
    category: "People & Body",
  },
];

describe("emoji search", () => {
  test("normalizes case and whitespace", () => {
    expect(normalizeEmojiQuery("  Big   SMILE  ")).toBe("big smile");
  });

  test("empty query returns every emoji", () => {
    expect(searchEmojis("", EMOJIS)).toEqual(EMOJIS);
    expect(searchEmojis("   ", EMOJIS)).toEqual(EMOJIS);
  });

  test("matches emoji names", () => {
    expect(searchEmojis("heart", EMOJIS).map((emoji) => emoji.emoji)).toEqual(["❤️"]);
  });

  test("matches keywords", () => {
    expect(searchEmojis("favorite", EMOJIS).map((emoji) => emoji.emoji)).toEqual(["❤️"]);
  });

  test("matches shortcodes with colons", () => {
    expect(searchEmojis(":smile:", EMOJIS).map((emoji) => emoji.emoji)).toEqual(["😄"]);
  });

  test("matches shortcodes without colons", () => {
    expect(searchEmojis("thumbsup", EMOJIS).map((emoji) => emoji.emoji)).toEqual(["👍"]);
  });

  test("matches symbolic shortcodes", () => {
    expect(searchEmojis("+1", EMOJIS).map((emoji) => emoji.emoji)).toEqual(["👍"]);
  });

  test("is case-insensitive", () => {
    expect(searchEmojis("LOVE", EMOJIS).map((emoji) => emoji.emoji)).toEqual(["❤️"]);
  });

  test("returns an empty list when nothing matches", () => {
    expect(searchEmojis("does-not-exist", EMOJIS)).toEqual([]);
  });
});
