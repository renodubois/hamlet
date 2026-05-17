import { describe, expect, test } from "vitest";
import { CONSERVATIVE_EMOJIS } from "./emoji-data";

describe("built-in emoji data", () => {
  test("every emoji has at least one shortcode", () => {
    for (const entry of CONSERVATIVE_EMOJIS) {
      expect(entry.shortcodes.length, `${entry.emoji} shortcode count`).toBeGreaterThan(0);
      expect(
        entry.shortcodes.every((shortcode) => shortcode.trim().length > 0),
        `${entry.emoji} shortcodes are non-empty`,
      ).toBe(true);
    }
  });

  test("every shortcode is unique", () => {
    const seen = new Map<string, string>();
    const duplicates: string[] = [];

    for (const entry of CONSERVATIVE_EMOJIS) {
      for (const shortcode of entry.shortcodes) {
        const normalized = shortcode.toLowerCase();
        const previousEmoji = seen.get(normalized);
        if (previousEmoji !== undefined) {
          duplicates.push(`${shortcode} (${previousEmoji}, ${entry.emoji})`);
        } else {
          seen.set(normalized, entry.emoji);
        }
      }
    }

    expect(duplicates).toEqual([]);
  });
});
