import { describe, expect, test } from "vitest";
import type { PublicUser } from "../api";
import {
  findActiveMentionToken,
  hydratedMentionsIncludeUser,
  mentionDisplayName,
  mentionMarker,
  messageMentionsCurrentUser,
  parseMentionMarkers,
  rankMentionUsers,
  replaceMentionToken,
} from "./mentions";

const BOB: PublicUser = {
  id: 2,
  username: "bob",
  display_name: "Bobby",
  avatar_url: null,
};

describe("mention helpers", () => {
  test("tokenizes durable mention markers while preserving surrounding text", () => {
    expect(parseMentionMarkers("hi <@2> and <@123456789012345> ok <@abc>")).toEqual([
      { type: "text", value: "hi " },
      { type: "mention", marker: "<@2>", id: 2 },
      { type: "text", value: " and " },
      { type: "mention", marker: "<@123456789012345>", id: 123456789012345 },
      { type: "text", value: " ok <@abc>" },
    ]);
  });

  test.each([
    ["at the start with an empty query", "@", 1, { start: 0, end: 1, query: "" }],
    ["after whitespace", "hello @bo", "hello @bo".length, { start: 6, end: 9, query: "bo" }],
    ["after a newline", "hello\n@bo", "hello\n@bo".length, { start: 6, end: 9, query: "bo" }],
    ["after opening punctuation", "(@bo", "(@bo".length, { start: 1, end: 4, query: "bo" }],
    [
      "after another durable marker",
      "<@2>@ali",
      "<@2>@ali".length,
      { start: "<@2>".length, end: "<@2>@ali".length, query: "ali" },
    ],
  ])("detects an active @ token %s", (_, value, caret, expected) => {
    expect(findActiveMentionToken(value, { start: Number(caret), end: Number(caret) })).toEqual(
      expected,
    );
  });

  test.each([
    ["selected text", "@bo", 0, 3],
    ["email-like text", "alice@example", "alice@example".length, "alice@example".length],
    [
      "URL path",
      "https://example.test/@bob",
      "https://example.test/@bob".length,
      "https://example.test/@bob".length,
    ],
    ["word-attached token", "hello@bob", "hello@bob".length, "hello@bob".length],
    ["dot punctuation boundary", ".@bob", ".@bob".length, ".@bob".length],
    ["slash punctuation boundary", "/@bob", "/@bob".length, "/@bob".length],
    ["durable marker being typed", "<@2", "<@2".length, "<@2".length],
    ["invalid query punctuation", "@bob!", "@bob!".length, "@bob!".length],
  ])("suppresses active @ token detection for %s", (_, value, start, end) => {
    expect(findActiveMentionToken(value, { start: Number(start), end: Number(end) })).toBeNull();
  });

  test("replaces a token with a durable marker and exactly one trailing space", () => {
    const token = findActiveMentionToken("hello @bo world", {
      start: "hello @bo".length,
      end: "hello @bo".length,
    });

    if (token === null) throw new Error("expected active mention token");
    expect(replaceMentionToken("hello @bo world", token, BOB)).toEqual({
      value: "hello <@2> world",
      caretIndex: "hello <@2> ".length,
      marker: "<@2>",
    });
  });

  test("formats mention labels from display name or username and marker ids", () => {
    expect(mentionMarker(BOB)).toBe("<@2>");
    expect(mentionDisplayName(BOB)).toBe("Bobby");
    expect(mentionDisplayName({ ...BOB, display_name: null })).toBe("bob");
  });

  test("detects hydrated current-user mentions while ignoring null users and tombstones", () => {
    expect(hydratedMentionsIncludeUser([BOB], BOB.id)).toBe(true);
    expect(hydratedMentionsIncludeUser([BOB], 99)).toBe(false);
    expect(hydratedMentionsIncludeUser([BOB], null)).toBe(false);
    expect(messageMentionsCurrentUser({ mentions: [BOB], deleted_at: null }, BOB.id)).toBe(true);
    expect(messageMentionsCurrentUser({ mentions: [BOB], deleted_at: 1 }, BOB.id)).toBe(false);
  });

  test("ranks users deterministically by match quality, field, username, and id", () => {
    const users: PublicUser[] = [
      { id: 5, username: "zali", display_name: null, avatar_url: null },
      { id: 4, username: "bobby", display_name: "Ali", avatar_url: null },
      { id: 3, username: "alice", display_name: null, avatar_url: null },
      { id: 2, username: "ali", display_name: "Zed", avatar_url: null },
      { id: 1, username: "alex", display_name: "A L I", avatar_url: null },
      { id: 6, username: "unmatched", display_name: null, avatar_url: null },
    ];

    expect(rankMentionUsers(users, "ali").map((user) => user.id)).toEqual([2, 4, 3, 5, 1]);
    expect(rankMentionUsers(users, "").map((user) => user.id)).toEqual([1, 2, 3, 4, 6, 5]);
  });
});
