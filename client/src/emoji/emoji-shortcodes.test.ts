import { describe, expect, test } from "vitest";
import type { EmojiEntry } from "./emoji-data";
import {
  createEmojiShortcodeLookup,
  lookupEmojiShortcode,
  replaceCompletedEmojiShortcodeBeforeCaret,
} from "./emoji-shortcodes";

function replaceAtEnd(value: string) {
  return replaceCompletedEmojiShortcodeBeforeCaret(value, value.length);
}

const CUSTOM_EMOJIS: readonly EmojiEntry[] = [
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
];

describe("emoji shortcode replacement", () => {
  test("looks up exact built-in aliases case-insensitively", () => {
    expect(lookupEmojiShortcode(":grinning:")).toBe("😀");
    expect(lookupEmojiShortcode(":GRINNING:")).toBe("😀");
    expect(lookupEmojiShortcode(":satisfied:")).toBe("😆");
    expect(lookupEmojiShortcode(":red_car:")).toBe("🚗");
    expect(lookupEmojiShortcode(":WHITE_CHECK_MARK:")).toBe("✅");
    expect(lookupEmojiShortcode(":+1:")).toBe("👍");
    expect(lookupEmojiShortcode(":-1:")).toBe("👎");
  });

  test("does not normalize fuzzy or unregistered shortcode-looking text", () => {
    expect(lookupEmojiShortcode(":smil:")).toBeUndefined();
    expect(lookupEmojiShortcode(":white-check-mark:")).toBeUndefined();
    expect(lookupEmojiShortcode(":white check mark:")).toBeUndefined();
    expect(lookupEmojiShortcode(":whitecheckmark:")).toBeUndefined();
    expect(lookupEmojiShortcode("grinning")).toBeUndefined();
  });

  test("replaces a completed shortcode immediately before the caret", () => {
    expect(replaceAtEnd(":grinning:")).toEqual({
      value: "😀",
      caretIndex: "😀".length,
      replaced: true,
    });
    expect(replaceAtEnd("hello :GRINNING:")).toEqual({
      value: "hello 😀",
      caretIndex: "hello 😀".length,
      replaced: true,
    });
    expect(replaceAtEnd("done :satisfied:").value).toBe("done 😆");
    expect(replaceAtEnd("ship it :+1:").value).toBe("ship it 👍");
  });

  test("replaces adjacent completed shortcode chains before the caret", () => {
    expect(replaceAtEnd(":grinning::heart:")).toEqual({
      value: "😀❤️",
      caretIndex: "😀❤️".length,
      replaced: true,
    });
    expect(replaceAtEnd("ship it :+1::white_check_mark:").value).toBe("ship it 👍✅");
  });

  test("maps active custom emoji shortcodes to durable markers", () => {
    const lookup = createEmojiShortcodeLookup(CUSTOM_EMOJIS);

    expect(lookupEmojiShortcode(":PARTY:", lookup)).toBe("<:party:123>");
    expect(replaceCompletedEmojiShortcodeBeforeCaret("hello :party:", undefined, lookup)).toEqual({
      value: "hello <:party:123>",
      caretIndex: "hello <:party:123>".length,
      replaced: true,
    });
    expect(
      replaceCompletedEmojiShortcodeBeforeCaret("<:party:123>:party:", undefined, lookup),
    ).toEqual({
      value: "<:party:123><:party:123>",
      caretIndex: "<:party:123><:party:123>".length,
      replaced: true,
    });
  });

  test("supports start, whitespace, opening-punctuation, and emoji boundaries", () => {
    expect(replaceAtEnd(":heart:").value).toBe("❤️");
    expect(replaceAtEnd("hello :heart:").value).toBe("hello ❤️");
    expect(replaceAtEnd("(:heart:").value).toBe("(❤️");
    expect(replaceAtEnd("😀:heart:").value).toBe("😀❤️");
    expect(replaceAtEnd("❤️:grinning:").value).toBe("❤️😀");
  });

  test("keeps word-attached and unrecognized shortcodes literal", () => {
    expect(replaceAtEnd("abc:grinning:")).toEqual({
      value: "abc:grinning:",
      caretIndex: "abc:grinning:".length,
      replaced: false,
    });
    expect(replaceAtEnd("hello :white-check-mark:")).toEqual({
      value: "hello :white-check-mark:",
      caretIndex: "hello :white-check-mark:".length,
      replaced: false,
    });
    expect(replaceAtEnd("abc:grinning::heart:")).toEqual({
      value: "abc:grinning::heart:",
      caretIndex: "abc:grinning::heart:".length,
      replaced: false,
    });
  });

  test("only rewrites the completed token immediately before the caret", () => {
    const result = replaceAtEnd(":smile: stays literal, then :grinning:");

    expect(result).toEqual({
      value: ":smile: stays literal, then 😀",
      caretIndex: ":smile: stays literal, then 😀".length,
      replaced: true,
    });
  });

  test("preserves text before and after a mid-draft caret replacement", () => {
    const value = "say :grinning::heart: please";
    const caretIndex = "say :grinning::heart:".length;

    expect(replaceCompletedEmojiShortcodeBeforeCaret(value, caretIndex)).toEqual({
      value: "say 😀❤️ please",
      caretIndex: "say 😀❤️".length,
      replaced: true,
    });
  });

  test("does nothing when the caret is not immediately after a closing colon", () => {
    expect(replaceAtEnd(":grinning:!")).toEqual({
      value: ":grinning:!",
      caretIndex: ":grinning:!".length,
      replaced: false,
    });
    expect(replaceAtEnd(":grinning")).toEqual({
      value: ":grinning",
      caretIndex: ":grinning".length,
      replaced: false,
    });
  });
});
