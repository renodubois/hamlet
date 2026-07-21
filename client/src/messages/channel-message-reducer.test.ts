import { describe, expect, it } from "vitest";
import type {
  Message,
  MessageEmbedsUpdated,
  MessageReactionsUpdated,
  ThreadReplyCreated,
  ThreadReplyDeleted,
} from "../api/messages";
import {
  channelMessageReducer,
  createChannelMessageState,
  type ChannelMessageAction,
  type ChannelMessageState,
} from "./channel-message-reducer";

const CHANNEL_ID = 10;
const GENERATION = 1;

function message(id: number, overrides: Partial<Message> = {}): Message {
  return {
    id,
    user_id: 1,
    channel_id: CHANNEL_ID,
    parent_id: null,
    reply_to_message_id: null,
    reply_to: null,
    created_at: id * 10,
    deleted_at: null,
    text: `message ${id}`,
    username: "alice",
    display_name: null,
    avatar_url: null,
    suppress_embeds: false,
    mentions: [],
    attachments: [],
    embeds: [],
    reactions: [],
    ...overrides,
  };
}

function started(
  channelId = CHANNEL_ID,
  generation = GENERATION,
  initial = createChannelMessageState(channelId),
): ChannelMessageState {
  return channelMessageReducer(initial, { type: "loadStarted", channelId, generation });
}

function reduce(state: ChannelMessageState, ...actions: ChannelMessageAction[]) {
  return actions.reduce(channelMessageReducer, state);
}

describe("channelMessageReducer", () => {
  it("applies and journals a live create during loading, then replays it exactly once", () => {
    const live = message(2, { created_at: 20 });
    const duringLoad = channelMessageReducer(started(), {
      type: "messageCreated",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      message: live,
    });

    expect(duringLoad.messages).toEqual([live]);
    expect(duringLoad.liveActionsDuringLoad).toHaveLength(1);

    const loaded = channelMessageReducer(duringLoad, {
      type: "loadSucceeded",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      messages: [message(3, { created_at: 30 }), live, message(1, { created_at: 10 })],
    });

    expect(loaded.messages.map(({ id }) => id)).toEqual([1, 2, 3]);
    expect(loaded.messages.filter(({ id }) => id === live.id)).toHaveLength(1);
    expect(loaded.liveActionsDuringLoad).toEqual([]);
    expect(loaded.status).toBe("ready");
  });

  it("dedupes HTTP-before-SSE and SSE-before-HTTP creates by ID", () => {
    const fromHttp = message(1, { text: "HTTP" });
    const fromSse = message(1, { text: "SSE" });
    const loaded = channelMessageReducer(started(), {
      type: "loadSucceeded",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      messages: [fromHttp],
    });
    const httpBeforeSse = channelMessageReducer(loaded, {
      type: "messageCreated",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      message: fromSse,
    });
    expect(httpBeforeSse.messages).toEqual([fromSse]);

    const sseBeforeHttp = reduce(
      started(),
      {
        type: "messageCreated",
        channelId: CHANNEL_ID,
        generation: GENERATION,
        message: fromSse,
      },
      {
        type: "loadSucceeded",
        channelId: CHANNEL_ID,
        generation: GENERATION,
        messages: [fromHttp],
      },
    );
    expect(sseBeforeHttp.messages).toEqual([fromSse]);
  });

  it("sorts by created_at (falling back to ID), then by ID", () => {
    const state = channelMessageReducer(started(), {
      type: "loadSucceeded",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      messages: [
        message(5, { created_at: 20 }),
        message(4, { created_at: 20 }),
        message(2, { created_at: undefined }),
        message(1, { created_at: 30 }),
      ],
    });
    expect(state.messages.map(({ id }) => id)).toEqual([2, 4, 5, 1]);
  });

  it("replays a hard delete so a stale snapshot cannot resurrect the row or its references", () => {
    const target = message(1);
    const reference = message(2, {
      reply_to_message_id: target.id,
      reply_to: {
        id: target.id,
        user_id: target.user_id,
        channel_id: target.channel_id,
        created_at: target.created_at as number,
        deleted_at: null,
        text: target.text,
        username: target.username,
        display_name: target.display_name,
        avatar_url: target.avatar_url,
      },
    });
    const duringLoad = channelMessageReducer(started(), {
      type: "messageHardDeleted",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      deletion: { id: target.id, channel_id: CHANNEL_ID },
    });
    const loaded = channelMessageReducer(duringLoad, {
      type: "loadSucceeded",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      messages: [target, reference],
    });

    expect(loaded.messages).toHaveLength(1);
    expect(loaded.messages[0]).toMatchObject({
      id: reference.id,
      reply_to_message_id: target.id,
      reply_to: null,
    });
  });

  it("distinguishes a tombstone update from a hard delete when patching references", () => {
    const target = message(1);
    const reference = message(2, {
      reply_to_message_id: target.id,
      reply_to: null,
    });
    const ready = channelMessageReducer(started(), {
      type: "loadSucceeded",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      messages: [target, reference],
    });
    const tombstone = message(1, {
      deleted_at: 123,
      text: "",
      attachments: [],
      embeds: [],
      reactions: [],
    });
    const updated = channelMessageReducer(ready, {
      type: "messageUpdated",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      message: tombstone,
    });

    expect(updated.messages).toHaveLength(2);
    expect(updated.messages[0]).toBe(tombstone);
    expect(updated.messages[1]?.reply_to).toMatchObject({ id: target.id, deleted_at: 123 });

    const deleted = channelMessageReducer(updated, {
      type: "messageHardDeleted",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      deletion: { id: target.id, channel_id: CHANNEL_ID },
    });
    expect(deleted.messages).toHaveLength(1);
    expect(deleted.messages[0]?.reply_to).toBeNull();
  });

  it("patches an external referenced target without inserting it into the timeline", () => {
    const reference = message(2, {
      reply_to_message_id: 99,
      reply_to: null,
    });
    const ready = channelMessageReducer(started(), {
      type: "loadSucceeded",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      messages: [reference],
    });
    const external = message(99, { text: "updated target" });
    const updated = channelMessageReducer(ready, {
      type: "messageUpdated",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      message: external,
    });

    expect(updated.messages).toHaveLength(1);
    expect(updated.messages[0]?.reply_to).toMatchObject({ id: 99, text: "updated target" });
  });

  it("scopes embed, reaction, and thread-summary patches to one row", () => {
    const first = message(1, {
      reactions: [{ kind: "native", emoji: "👍", count: 1, me_reacted: true }],
    });
    const second = message(2);
    const ready = channelMessageReducer(started(), {
      type: "loadSucceeded",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      messages: [first, second],
    });
    const embedUpdate: MessageEmbedsUpdated = {
      id: first.id,
      channel_id: CHANNEL_ID,
      suppress_embeds: true,
      embeds: [
        {
          id: 50,
          message_id: first.id,
          url: "https://example.com",
          title: "Example",
          description: null,
          image_url: null,
          site_name: null,
          embed_type: "link",
          iframe_url: null,
          iframe_width: null,
          iframe_height: null,
        },
      ],
    };
    const reactionUpdate: MessageReactionsUpdated = {
      id: first.id,
      channel_id: CHANNEL_ID,
      user_id: 2,
      reactions: [{ kind: "native", emoji: "👍", count: 2, me_reacted: false }],
    };
    const created: ThreadReplyCreated = {
      channel_id: CHANNEL_ID,
      root_message_id: first.id,
      reply: message(20, { parent_id: first.id }),
      thread_summary: { reply_count: 2, last_reply_created_at: 200 },
    };
    const deleted: ThreadReplyDeleted = {
      channel_id: CHANNEL_ID,
      root_message_id: first.id,
      reply_id: 20,
      thread_summary: null,
    };

    const withEmbeds = channelMessageReducer(ready, {
      type: "messageEmbedsUpdated",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      update: embedUpdate,
    });
    expect(withEmbeds.messages[0]).toMatchObject({
      suppress_embeds: true,
      embeds: embedUpdate.embeds,
    });
    expect(withEmbeds.messages[1]).toBe(second);

    const withReactions = channelMessageReducer(withEmbeds, {
      type: "messageReactionsUpdated",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      update: reactionUpdate,
      currentUserId: 1,
    });
    expect(withReactions.messages[0]?.reactions).toEqual([
      { kind: "native", emoji: "👍", count: 2, me_reacted: true },
    ]);
    expect(withReactions.messages[1]).toBe(second);

    const withSummary = channelMessageReducer(withReactions, {
      type: "threadSummaryCreated",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      update: created,
    });
    expect(withSummary.messages[0]?.thread_summary).toBe(created.thread_summary);
    expect(withSummary.messages[1]).toBe(second);

    const withoutSummary = channelMessageReducer(withSummary, {
      type: "threadSummaryDeleted",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      update: deleted,
    });
    expect(withoutSummary.messages[0]?.thread_summary).toBeUndefined();
    expect(withoutSummary.messages[1]).toBe(second);
  });

  it("patches only current-user author and reference profile fields", () => {
    const own = message(1);
    const referring = message(2, {
      user_id: 2,
      username: "bob",
      reply_to_message_id: own.id,
      reply_to: {
        id: own.id,
        user_id: own.user_id,
        channel_id: CHANNEL_ID,
        created_at: own.created_at as number,
        text: own.text,
        username: own.username,
        display_name: own.display_name,
        avatar_url: own.avatar_url,
      },
    });
    const unaffected = message(3, { user_id: 3, username: "carol" });
    const ready = channelMessageReducer(started(), {
      type: "loadSucceeded",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      messages: [own, referring, unaffected],
    });
    const updated = channelMessageReducer(ready, {
      type: "currentUserProfileUpdated",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      user: { id: 1, username: "alice-new", display_name: "Alice", avatar_url: "/avatar" },
    });

    expect(updated.messages[0]).toMatchObject({
      username: "alice-new",
      display_name: "Alice",
      avatar_url: "/avatar",
    });
    expect(updated.messages[1]?.username).toBe("bob");
    expect(updated.messages[1]?.reply_to).toMatchObject({
      username: "alice-new",
      display_name: "Alice",
      avatar_url: "/avatar",
    });
    expect(updated.messages[2]).toBe(unaffected);
  });

  it("preserves unaffected message identity for message updates", () => {
    const first = message(1);
    const second = message(2);
    const ready = channelMessageReducer(started(), {
      type: "loadSucceeded",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      messages: [first, second],
    });
    const replacement = message(1, { text: "edited" });
    const updated = channelMessageReducer(ready, {
      type: "messageUpdated",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      message: replacement,
    });

    expect(updated.messages[0]).toBe(replacement);
    expect(updated.messages[1]).toBe(second);
  });

  it("treats semantically duplicate creates, updates, embeds, reactions, and profiles as no-ops", () => {
    const original = message(1);
    const ready = channelMessageReducer(started(), {
      type: "loadSucceeded",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      messages: [original],
    });
    const actions: ChannelMessageAction[] = [
      {
        type: "messageCreated",
        channelId: CHANNEL_ID,
        generation: GENERATION,
        message: { ...original },
      },
      {
        type: "messageUpdated",
        channelId: CHANNEL_ID,
        generation: GENERATION,
        message: { ...original },
      },
      {
        type: "messageEmbedsUpdated",
        channelId: CHANNEL_ID,
        generation: GENERATION,
        update: { id: original.id, channel_id: CHANNEL_ID, suppress_embeds: false, embeds: [] },
      },
      {
        type: "messageReactionsUpdated",
        channelId: CHANNEL_ID,
        generation: GENERATION,
        update: { id: original.id, channel_id: CHANNEL_ID, user_id: 2, reactions: [] },
        currentUserId: 1,
      },
      {
        type: "currentUserProfileUpdated",
        channelId: CHANNEL_ID,
        generation: GENERATION,
        user: { id: 1, username: "alice", display_name: null, avatar_url: null },
      },
    ];

    for (const action of actions) expect(channelMessageReducer(ready, action)).toBe(ready);
  });

  it("does not journal or tombstone a hard delete with a wrong-channel payload", () => {
    const loading = started();
    const malformed = channelMessageReducer(loading, {
      type: "messageHardDeleted",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      deletion: { id: 99, channel_id: 999 },
    });

    expect(malformed).toBe(loading);
    expect(malformed.liveActionsDuringLoad).toEqual([]);
    expect(malformed.deletedMessageIds.has(99)).toBe(false);
  });

  it("treats wrong-channel and stale-generation actions as identity no-ops", () => {
    const ready = channelMessageReducer(started(), {
      type: "loadSucceeded",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      messages: [message(1)],
    });
    const wrongChannel = channelMessageReducer(ready, {
      type: "messageCreated",
      channelId: 999,
      generation: GENERATION,
      message: message(2, { channel_id: 999 }),
    });
    const staleGeneration = channelMessageReducer(ready, {
      type: "messageHardDeleted",
      channelId: CHANNEL_ID,
      generation: GENERATION - 1,
      deletion: { id: 1, channel_id: CHANNEL_ID },
    });
    const staleLoad = channelMessageReducer(ready, {
      type: "loadSucceeded",
      channelId: CHANNEL_ID,
      generation: GENERATION - 1,
      messages: [],
    });

    expect(wrongChannel).toBe(ready);
    expect(staleGeneration).toBe(ready);
    expect(staleLoad).toBe(ready);
  });

  it("clears old rows immediately on a new-channel load and ignores the old completion", () => {
    const oldReady = channelMessageReducer(started(), {
      type: "loadSucceeded",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      messages: [message(1)],
    });
    const switched = channelMessageReducer(oldReady, {
      type: "loadStarted",
      channelId: 20,
      generation: 2,
    });
    expect(switched.messages).toEqual([]);
    expect(switched.channelId).toBe(20);
    expect(switched.status).toBe("loading");

    const staleCompletion = channelMessageReducer(switched, {
      type: "loadSucceeded",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      messages: [message(2)],
    });
    expect(staleCompletion).toBe(switched);
  });

  it("retains same-channel rows during refresh and applies live actions before success", () => {
    const existing = message(1);
    const ready = channelMessageReducer(started(), {
      type: "loadSucceeded",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      messages: [existing],
    });
    const refreshing = channelMessageReducer(ready, {
      type: "loadStarted",
      channelId: CHANNEL_ID,
      generation: 2,
    });
    expect(refreshing.messages[0]).toBe(existing);

    const live = message(2);
    const withLive = channelMessageReducer(refreshing, {
      type: "messageCreated",
      channelId: CHANNEL_ID,
      generation: 2,
      message: live,
    });
    const refreshed = channelMessageReducer(withLive, {
      type: "loadSucceeded",
      channelId: CHANNEL_ID,
      generation: 2,
      messages: [existing],
    });
    expect(refreshed.messages).toEqual([existing, live]);
  });

  it("keeps hard-delete tombstones across a later same-channel refresh", () => {
    const original = message(1);
    let state = channelMessageReducer(started(), {
      type: "loadSucceeded",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      messages: [original],
    });
    state = channelMessageReducer(state, {
      type: "messageHardDeleted",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      deletion: { id: original.id, channel_id: CHANNEL_ID },
    });
    state = channelMessageReducer(state, {
      type: "loadStarted",
      channelId: CHANNEL_ID,
      generation: 2,
    });
    state = channelMessageReducer(state, {
      type: "loadSucceeded",
      channelId: CHANNEL_ID,
      generation: 2,
      messages: [original],
    });

    expect(state.messages).toEqual([]);
    expect(state.deletedMessageIds.has(original.id)).toBe(true);
  });

  it("leaves a truly empty timeline after deleting the final message", () => {
    const ready = channelMessageReducer(started(), {
      type: "loadSucceeded",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      messages: [message(1)],
    });
    const empty = channelMessageReducer(ready, {
      type: "messageHardDeleted",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      deletion: { id: 1, channel_id: CHANNEL_ID },
    });
    expect(empty.messages).toEqual([]);
  });

  it("records load failures without replacing rows that live actions already applied", () => {
    const live = message(1);
    const duringLoad = channelMessageReducer(started(), {
      type: "messageCreated",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      message: live,
    });
    const error = new Error("offline");
    const failed = channelMessageReducer(duringLoad, {
      type: "loadFailed",
      channelId: CHANNEL_ID,
      generation: GENERATION,
      error,
    });
    expect(failed.messages).toEqual([live]);
    expect(failed.status).toBe("error");
    expect(failed.error).toBe(error);
    expect(failed.liveActionsDuringLoad).toEqual([]);
  });
});
