import { describe, expect, test } from "vitest";
import type { ReactionSummary } from "../api";
import {
  applyOptimisticReaction,
  mergeReactionUpdateForViewer,
  reactionSummariesEqual,
} from "./reaction-summaries";

const thumbs = { kind: "native", emoji: "👍" } as const;

describe("reaction summary optimistic updates", () => {
  test("adds a brand-new reaction after existing summaries", () => {
    const current: ReactionSummary[] = [
      { kind: "native", emoji: "❤️", count: 2, me_reacted: false },
    ];

    expect(applyOptimisticReaction(current, thumbs, "add")).toEqual([
      current[0],
      { kind: "native", emoji: "👍", count: 1, me_reacted: true, reactors: ["You"] },
    ]);
  });

  test("toggles unreacted and reacted pills", () => {
    const unreacted: ReactionSummary[] = [
      { kind: "native", emoji: "👍", count: 1, me_reacted: false },
    ];
    expect(applyOptimisticReaction(unreacted, thumbs, "toggle")).toEqual([
      { kind: "native", emoji: "👍", count: 2, me_reacted: true, reactors: ["You"] },
    ]);

    const reacted: ReactionSummary[] = [
      { kind: "native", emoji: "👍", count: 1, me_reacted: true, reactors: ["You"] },
    ];
    expect(applyOptimisticReaction(reacted, thumbs, "toggle")).toEqual([]);
  });

  test("adds and matches custom reactions by immutable id", () => {
    const current: ReactionSummary[] = [
      {
        kind: "custom",
        emoji_id: 123,
        name: "old_party",
        image_url: "/uploads/emojis/123.webp?v=1",
        animated: false,
        count: 1,
        me_reacted: false,
        reactors: ["Alice"],
      },
    ];

    expect(
      applyOptimisticReaction(
        current,
        {
          kind: "custom",
          emoji_id: 123,
          name: "party",
          image_url: "/uploads/emojis/123.webp?v=2",
          animated: true,
        },
        "add",
      ),
    ).toEqual([
      {
        kind: "custom",
        emoji_id: 123,
        name: "old_party",
        image_url: "/uploads/emojis/123.webp?v=1",
        animated: false,
        count: 2,
        me_reacted: true,
        reactors: ["You", "Alice"],
      },
    ]);
  });

  test("remove is idempotent when the user has not reacted", () => {
    const current: ReactionSummary[] = [
      { kind: "native", emoji: "👍", count: 3, me_reacted: false },
    ];

    expect(applyOptimisticReaction(current, thumbs, "remove")).toEqual(current);
  });

  test("compares summaries by ordered reaction fields", () => {
    const current: ReactionSummary[] = [
      { kind: "native", emoji: "👍", count: 3, me_reacted: true, reactors: ["You", "A"] },
    ];

    expect(reactionSummariesEqual(current, [{ ...current[0] }])).toBe(true);
    expect(
      reactionSummariesEqual(current, [
        { kind: "native", emoji: "👍", count: 4, me_reacted: true, reactors: ["You", "A"] },
      ]),
    ).toBe(false);
    expect(
      reactionSummariesEqual(current, [
        { kind: "native", emoji: "👍", count: 3, me_reacted: true, reactors: ["A", "You"] },
      ]),
    ).toBe(false);
  });

  test("uses actor-personalized SSE state for the current user", () => {
    const incoming: ReactionSummary[] = [
      { kind: "native", emoji: "👍", count: 1, me_reacted: true, reactors: ["You"] },
    ];

    expect(mergeReactionUpdateForViewer([], incoming, 1, 1)).toEqual(incoming);
  });

  test("preserves viewer pressed state for another user's SSE reaction update", () => {
    const current: ReactionSummary[] = [
      { kind: "native", emoji: "👍", count: 2, me_reacted: true, reactors: ["You", "Bob"] },
    ];
    const incoming: ReactionSummary[] = [
      { kind: "native", emoji: "👍", count: 1, me_reacted: false, reactors: ["Bob"] },
    ];

    expect(mergeReactionUpdateForViewer(current, incoming, 2, 1)).toEqual([
      { kind: "native", emoji: "👍", count: 1, me_reacted: true, reactors: ["Bob"] },
    ]);
  });
});
