import { describe, expect, test } from "vitest";
import type { Channel } from "../api";
import {
  channelMentionMarker,
  findActiveChannelToken,
  parseChannelMarkers,
  rankChannelMentions,
  replaceChannelToken,
} from "./channel-mentions";

const CHANNELS: Channel[] = [
  { id: 20, name: "random", position: 2, type: "text" },
  { id: 10, name: "general", position: 0, type: "text" },
  { id: 30, name: "voice-chat", position: 1, type: "voice" },
  { id: 40, name: "project-planning", position: 3, type: "text" },
];

describe("channel mention helpers", () => {
  test("formats and parses durable channel markers", () => {
    expect(channelMentionMarker(CHANNELS[0])).toBe("<#20>");
    expect(parseChannelMarkers("go to <#10> or <#999> now")).toEqual([
      { type: "text", value: "go to " },
      { type: "channel", marker: "<#10>", id: 10 },
      { type: "text", value: " or " },
      { type: "channel", marker: "<#999>", id: 999 },
      { type: "text", value: " now" },
    ]);
  });

  test.each([
    ["start of draft", "#gen", "#gen".length, { start: 0, end: 4, query: "gen" }],
    ["after whitespace", "hello #gen", "hello #gen".length, { start: 6, end: 10, query: "gen" }],
    ["after opening punctuation", "(#gen", "(#gen".length, { start: 1, end: 5, query: "gen" }],
    ["after a durable marker", "<@2>#gen", "<@2>#gen".length, { start: 4, end: 8, query: "gen" }],
  ])("finds an active # token at a valid boundary %s", (_, value, caret, expected) => {
    expect(findActiveChannelToken(value, { start: caret, end: caret })).toEqual(expected);
  });

  test.each([
    ["selected text", "#gen", 0, "#gen".length],
    ["word-attached", "abc#gen", "abc#gen".length, "abc#gen".length],
    [
      "URL fragment",
      "https://example.test/#gen",
      "https://example.test/#gen".length,
      "https://example.test/#gen".length,
    ],
    [
      "invalid query character",
      "hello #general!",
      "hello #general!".length,
      "hello #general!".length,
    ],
  ])("does not find a channel token for %s", (_, value, start, end) => {
    expect(findActiveChannelToken(value, { start, end })).toBeNull();
  });

  test("replaces an active token with a durable marker and trailing space", () => {
    expect(
      replaceChannelToken(
        "hello #gen world",
        { start: "hello ".length, end: "hello #gen".length, query: "gen" },
        CHANNELS[1],
      ),
    ).toEqual({
      value: "hello <#10> world",
      caretIndex: "hello <#10> ".length,
      marker: "<#10>",
    });
  });

  test("ranks text channels deterministically and excludes voice channels", () => {
    expect(rankChannelMentions(CHANNELS, "", 10).map((channel) => channel.name)).toEqual([
      "general",
      "random",
      "project-planning",
    ]);
    expect(rankChannelMentions(CHANNELS, "plan", 10).map((channel) => channel.name)).toEqual([
      "project-planning",
    ]);
    expect(rankChannelMentions(CHANNELS, "voice", 10)).toEqual([]);
  });
});
