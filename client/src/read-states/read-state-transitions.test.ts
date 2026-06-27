import { describe, expect, test } from "vitest";
import {
  applyIncomingTopLevelMessage,
  applyReadStateSnapshot,
  applyReadStateUpdate,
  channelHasUnread,
  channelMentionCount,
  readStateForChannel,
} from "./read-state-transitions";
import type { Message } from "../api";

const summary = {
  channel_id: 10,
  has_unread: true,
  mention_count: 2,
  last_read_created_at: 100,
  last_read_message_id: 20,
  updated_at: 300,
};

function message(overrides: Partial<Message> = {}): Message {
  return {
    id: 21,
    user_id: 2,
    channel_id: 10,
    parent_id: null,
    created_at: 101,
    text: "hello",
    username: "bob",
    display_name: null,
    avatar_url: null,
    suppress_embeds: false,
    mentions: [],
    attachments: [],
    embeds: [],
    ...overrides,
  };
}

describe("read-state transitions", () => {
  test("indexes snapshot summaries by channel", () => {
    const states = applyReadStateSnapshot([summary]);

    expect(readStateForChannel(states, 10)).toEqual(summary);
    expect(readStateForChannel(states, 99)).toBeUndefined();
  });

  test("applies one authoritative read-state update", () => {
    const states = applyReadStateSnapshot([summary]);
    const updated = applyReadStateUpdate(states, {
      ...summary,
      has_unread: false,
      mention_count: 0,
    });

    expect(channelHasUnread(updated, 10)).toBe(false);
    expect(channelMentionCount(updated, 10)).toBe(0);
    expect(channelHasUnread(states, 10)).toBe(true);
  });

  test("optimistically marks unread top-level messages after the read cursor", () => {
    const states = applyReadStateSnapshot([{ ...summary, has_unread: false, mention_count: 0 }]);

    const updated = applyIncomingTopLevelMessage(states, message(), 1);

    expect(channelHasUnread(updated, 10)).toBe(true);
    expect(channelMentionCount(updated, 10)).toBe(0);
    expect(channelHasUnread(states, 10)).toBe(false);
  });

  test("increments mention badges only for creation-time top-level mentions", () => {
    const states = applyReadStateSnapshot([{ ...summary, has_unread: false, mention_count: 2 }]);

    const updated = applyIncomingTopLevelMessage(
      states,
      message({ mentions: [{ id: 1, username: "alice", display_name: null, avatar_url: null }] }),
      1,
    );

    expect(channelHasUnread(updated, 10)).toBe(true);
    expect(channelMentionCount(updated, 10)).toBe(3);
    expect(applyIncomingTopLevelMessage(states, message({ parent_id: 9 }), 1)).toBe(states);
    expect(applyIncomingTopLevelMessage(states, message({ user_id: 1 }), 1)).toBe(states);
    expect(applyIncomingTopLevelMessage(states, message({ deleted_at: 200 }), 1)).toBe(states);
  });

  test("uses tuple ordering to ignore messages at or before the read cursor", () => {
    const states = applyReadStateSnapshot([{ ...summary, has_unread: false, mention_count: 0 }]);

    expect(applyIncomingTopLevelMessage(states, message({ created_at: 99 }), 1)).toBe(states);
    expect(applyIncomingTopLevelMessage(states, message({ id: 20, created_at: 100 }), 1)).toBe(
      states,
    );
    expect(
      channelHasUnread(
        applyIncomingTopLevelMessage(states, message({ id: 21, created_at: 100 }), 1),
        10,
      ),
    ).toBe(true);
  });

  test("exposes ordinary unread and mention selectors", () => {
    const states = applyReadStateSnapshot([summary]);

    expect(channelHasUnread(states, 10)).toBe(true);
    expect(channelMentionCount(states, 10)).toBe(2);
    expect(channelHasUnread(states, 99)).toBe(false);
    expect(channelMentionCount(states, 99)).toBe(0);
  });
});
