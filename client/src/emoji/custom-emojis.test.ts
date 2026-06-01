import { describe, expect, test } from "vitest";
import { customEmojiMarker, customEmojiToEntry, parseCustomEmojiMarkers } from "./custom-emojis";

const staticEmoji = {
  id: 123,
  name: "party_parrot",
  image_url: "/uploads/emojis/123.webp?v=1",
  animated: false,
  created_by_user_id: 1,
  created_at: 1,
  updated_at: 1,
  deleted_at: null,
};

const animatedEmoji = {
  ...staticEmoji,
  id: 456,
  name: "dance",
  image_url: "/uploads/emojis/456.gif?v=1",
  animated: true,
};

describe("custom emoji markers", () => {
  test("builds canonical static and animated markers", () => {
    expect(customEmojiMarker(staticEmoji)).toBe("<:party_parrot:123>");
    expect(customEmojiMarker(animatedEmoji)).toBe("<a:dance:456>");
  });

  test("turns custom emoji DTOs into searchable picker entries", () => {
    expect(customEmojiToEntry(staticEmoji)).toMatchObject({
      kind: "custom",
      emoji: "<:party_parrot:123>",
      shortcodes: [":party_parrot:"],
      category: "Custom",
      id: 123,
      imageUrl: "/uploads/emojis/123.webp?v=1",
      animated: false,
      deletedAt: null,
    });
  });

  test("splits text around valid custom emoji markers", () => {
    expect(parseCustomEmojiMarkers("hi <:party_parrot:123> <a:dance:456>!")).toEqual([
      { type: "text", value: "hi " },
      {
        type: "custom-emoji",
        marker: "<:party_parrot:123>",
        animated: false,
        storedName: "party_parrot",
        id: 123,
      },
      { type: "text", value: " " },
      {
        type: "custom-emoji",
        marker: "<a:dance:456>",
        animated: true,
        storedName: "dance",
        id: 456,
      },
      { type: "text", value: "!" },
    ]);
  });
});
