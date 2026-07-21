import { describe, expect, it } from "vitest";
import type {
  Message,
  MessageEmbedsUpdated,
  MessageReactionsUpdated,
  Thread,
  ThreadReplyCreated,
  ThreadReplyDeleted,
} from "../api";
import { makeMessage } from "../test/fixtures";
import {
  createThreadState,
  isValidThreadPayload,
  threadReducer,
  type ThreadAction,
  type ThreadState,
} from "./thread-reducer";

const channelId = 10;
const rootMessageId = 100;
const generation = 1;

function message(id: number, overrides: Partial<Message> = {}): Message {
  return makeMessage({
    id,
    user_id: id,
    channel_id: channelId,
    parent_id: id === rootMessageId ? null : rootMessageId,
    created_at: id,
    text: `message ${id}`,
    username: `user${id}`,
    ...overrides,
  });
}

function thread(replies: readonly Message[] = [], overrides: Partial<Thread> = {}): Thread {
  return {
    root: message(rootMessageId),
    replies: [...replies],
    has_more_replies: false,
    ...overrides,
  };
}

function identity() {
  return { channelId, rootMessageId, generation } as const;
}

function started(): ThreadState {
  return threadReducer(createThreadState(channelId, rootMessageId), {
    type: "initial-load-started",
    ...identity(),
  });
}

function ready(replies: readonly Message[] = []): ThreadState {
  return threadReducer(started(), {
    type: "initial-load-succeeded",
    ...identity(),
    thread: thread(replies),
  });
}

type ActionWithoutIdentity = ThreadAction extends infer Action
  ? Action extends { channelId: number; rootMessageId: number; generation: number }
    ? Omit<Action, "channelId" | "rootMessageId" | "generation">
    : never
  : never;

function dispatch(state: ThreadState, action: ActionWithoutIdentity) {
  return threadReducer(state, { ...action, ...identity() } as ThreadAction);
}

function created(reply: Message): ThreadReplyCreated {
  return {
    channel_id: channelId,
    root_message_id: rootMessageId,
    reply,
    thread_summary: { reply_count: 1, last_reply_created_at: reply.created_at ?? reply.id },
  };
}

function deleted(replyId: number): ThreadReplyDeleted {
  return { channel_id: channelId, root_message_id: rootMessageId, reply_id: replyId };
}

describe("threadReducer initial load and live journal", () => {
  it("applies a live reply immediately and keeps it exactly once after the snapshot", () => {
    const reply = message(102);
    const live = dispatch(started(), { type: "reply-created", event: created(reply) });

    expect(live.replies).toEqual([reply]);
    expect(live.liveActionsDuringLoad).toHaveLength(1);

    const loaded = dispatch(live, {
      type: "initial-load-succeeded",
      thread: thread([message(101), message(102)]),
    });
    expect(loaded.replies.map(({ id }) => id)).toEqual([101, 102]);
    expect(loaded.replies[1]).toBe(reply);
    expect(loaded.liveActionsDuringLoad).toEqual([]);
  });

  it("dedupes duplicate replies in the initial snapshot", () => {
    const first = message(101, { text: "first copy" });
    const latest = message(101, { text: "latest copy" });
    const loaded = dispatch(started(), {
      type: "initial-load-succeeded",
      thread: thread([first, latest]),
    });

    expect(loaded.replies).toEqual([latest]);
  });

  it("prevents a reply deleted during loading from returning in the initial snapshot", () => {
    const state = dispatch(started(), { type: "reply-deleted", event: deleted(101) });
    const loaded = dispatch(state, {
      type: "initial-load-succeeded",
      thread: thread([message(101)]),
    });

    expect(loaded.replies).toEqual([]);
    expect(loaded.deletedReplyIds.has(101)).toBe(true);
  });

  it("replays an update that arrived before its reply was in memory", () => {
    const updated = message(101, { text: "edited" });
    const state = dispatch(started(), { type: "message-updated", message: updated });
    const loaded = dispatch(state, {
      type: "initial-load-succeeded",
      thread: thread([message(101)]),
    });

    expect(loaded.replies[0]).toBe(updated);
  });

  it("retains live state and exposes an initial-load failure", () => {
    const reply = message(101);
    const state = dispatch(started(), { type: "reply-created", event: created(reply) });
    const error = new Error("offline");
    const failed = dispatch(state, { type: "initial-load-failed", error });

    expect(failed.status).toBe("error");
    expect(failed.error).toBe(error);
    expect(failed.replies).toEqual([reply]);
  });
});

describe("threadReducer reply, root, and reference transitions", () => {
  it("dedupes reply creation and sorts by (created_at, id)", () => {
    const later = message(103, { created_at: 20 });
    const earlier = message(102, { created_at: 10 });
    let state = ready([later]);
    state = dispatch(state, { type: "reply-created", event: created(earlier) });
    state = dispatch(state, { type: "reply-created", event: created(earlier) });

    expect(state.replies.map(({ id }) => id)).toEqual([102, 103]);
  });

  it("removes a reply, records its ID, and cannot recreate it", () => {
    const existing = message(101);
    let state = dispatch(ready([existing]), { type: "reply-deleted", event: deleted(existing.id) });
    state = dispatch(state, { type: "reply-created", event: created(message(existing.id)) });

    expect(state.replies).toEqual([]);
    expect(state.deletedReplyIds.has(existing.id)).toBe(true);
  });

  it("replaces only the updated root or reply and supports tombstones", () => {
    const first = message(101);
    const second = message(102);
    const initial = ready([first, second]);
    const root = message(rootMessageId, { text: "new root", deleted_at: 123 });
    const rootUpdated = dispatch(initial, { type: "message-updated", message: root });
    const reply = message(101, { text: "new reply", deleted_at: 456 });
    const replyUpdated = dispatch(rootUpdated, { type: "message-updated", message: reply });

    expect(rootUpdated.root).toBe(root);
    expect(rootUpdated.replies[0]).toBe(first);
    expect(replyUpdated.replies[0]).toBe(reply);
    expect(replyUpdated.replies[1]).toBe(second);
    expect(replyUpdated.root).toBe(root);
  });

  it("hard-deletes a root while retaining a tombstoned root update", () => {
    const tombstone = dispatch(ready(), {
      type: "message-updated",
      message: message(rootMessageId, { deleted_at: 1 }),
    });
    expect(tombstone.root?.deleted_at).toBe(1);

    const hardDeleted = dispatch(tombstone, {
      type: "message-deleted",
      messageId: rootMessageId,
    });
    expect(hardDeleted.root).toBeNull();
    expect(hardDeleted.rootHardDeleted).toBe(true);
  });

  it("keeps a hard root deletion across stale updates, snapshots, pages, and refreshes", () => {
    const deletedState = dispatch(ready([message(101)]), {
      type: "message-deleted",
      messageId: rootMessageId,
    });

    expect(
      dispatch(deletedState, {
        type: "message-updated",
        message: message(rootMessageId, { text: "stale root" }),
      }),
    ).toBe(deletedState);
    expect(
      dispatch(deletedState, { type: "initial-load-succeeded", thread: thread([message(102)]) }),
    ).toBe(deletedState);
    expect(
      dispatch(deletedState, { type: "older-page-succeeded", thread: thread([message(99)]) }),
    ).toBe(deletedState);
    expect(
      threadReducer(deletedState, {
        type: "initial-load-started",
        channelId,
        rootMessageId,
        generation: generation + 1,
      }),
    ).toBe(deletedState);
    expect(dispatch(deletedState, { type: "message-deleted", messageId: rootMessageId })).toBe(
      deletedState,
    );
  });

  it("patches every inline reference when an external target updates", () => {
    const targetId = 500;
    const reference = {
      id: targetId,
      user_id: 1,
      channel_id: channelId,
      created_at: 1,
      text: "old",
      username: "old",
      display_name: null,
      avatar_url: null,
    };
    const root = message(rootMessageId, { reply_to_message_id: targetId, reply_to: reference });
    const first = message(101, { reply_to_message_id: targetId, reply_to: reference });
    const untouched = message(102);
    const initial = dispatch(started(), {
      type: "initial-load-succeeded",
      thread: thread([first, untouched], { root }),
    });
    const target = message(targetId, { parent_id: null, text: "new", username: "target" });
    const updated = dispatch(initial, { type: "message-updated", message: target });

    expect(updated.root?.reply_to?.text).toBe("new");
    expect(updated.replies[0]?.reply_to?.text).toBe("new");
    expect(updated.replies[1]).toBe(untouched);
  });

  it("marks every inline reference unavailable after target deletion", () => {
    const targetId = 500;
    const reference = {
      id: targetId,
      user_id: 1,
      channel_id: channelId,
      created_at: 1,
      text: "old",
      username: "old",
      display_name: null,
      avatar_url: null,
    };
    const root = message(rootMessageId, { reply_to_message_id: targetId, reply_to: reference });
    const reply = message(101, { reply_to_message_id: targetId, reply_to: reference });
    const state = dispatch(started(), {
      type: "initial-load-succeeded",
      thread: thread([reply], { root }),
    });
    const deletedState = dispatch(state, { type: "message-deleted", messageId: targetId });

    expect(deletedState.root?.reply_to).toBeNull();
    expect(deletedState.replies[0]?.reply_to).toBeNull();
  });
});

describe("threadReducer scoped field events", () => {
  it("patches embeds on the root and one reply only", () => {
    const first = message(101);
    const second = message(102);
    const embed = {
      id: 1,
      message_id: first.id,
      url: "https://example.com",
      title: null,
      description: null,
      image_url: null,
      site_name: null,
      embed_type: "link" as const,
      iframe_url: null,
      iframe_width: null,
      iframe_height: null,
    };
    const event: MessageEmbedsUpdated = {
      id: first.id,
      channel_id: channelId,
      suppress_embeds: true,
      embeds: [embed],
    };
    const state = dispatch(ready([first, second]), { type: "embeds-updated", event });

    expect(state.replies[0]?.embeds).toEqual([embed]);
    expect(state.replies[0]?.suppress_embeds).toBe(true);
    expect(state.replies[1]).toBe(second);

    const rootEvent = { ...event, id: rootMessageId, suppress_embeds: false };
    const rootState = dispatch(state, { type: "embeds-updated", event: rootEvent });
    expect(rootState.root?.embeds).toEqual([embed]);
    expect(rootState.replies).toBe(state.replies);
  });

  it("patches reactions on the root or correctly identified reply", () => {
    const reply = message(101);
    const reaction = { kind: "native" as const, emoji: "👍", count: 2, me_reacted: false };
    const replyEvent: MessageReactionsUpdated = {
      id: reply.id,
      channel_id: channelId,
      parent_id: rootMessageId,
      root_message_id: rootMessageId,
      user_id: 9,
      reactions: [reaction],
    };
    const state = dispatch(ready([reply]), {
      type: "reactions-updated",
      event: replyEvent,
      currentUserId: 1,
    });
    expect(state.replies[0]?.reactions).toEqual([reaction]);

    const rootEvent = { ...replyEvent, id: rootMessageId, parent_id: null };
    const rootState = dispatch(state, {
      type: "reactions-updated",
      event: rootEvent,
      currentUserId: 1,
    });
    expect(rootState.root?.reactions).toEqual([reaction]);
    expect(rootState.replies).toBe(state.replies);
  });

  it("preserves state and object identity for semantic duplicate updates", () => {
    const reaction = { kind: "native" as const, emoji: "👍", count: 2, me_reacted: false };
    const reply = message(101, { reactions: [reaction] });
    const state = ready([reply]);

    expect(dispatch(state, { type: "message-updated", message: { ...reply } })).toBe(state);
    expect(
      dispatch(state, {
        type: "embeds-updated",
        event: {
          id: reply.id,
          channel_id: channelId,
          suppress_embeds: reply.suppress_embeds,
          embeds: [...reply.embeds],
        },
      }),
    ).toBe(state);
    expect(
      dispatch(state, {
        type: "reactions-updated",
        event: {
          id: reply.id,
          channel_id: channelId,
          root_message_id: rootMessageId,
          user_id: 9,
          reactions: [{ ...reaction }],
        },
        currentUserId: 1,
      }),
    ).toBe(state);
  });

  it("ignores embeds and reactions for another channel/root or unknown row", () => {
    const state = ready([message(101)]);
    const embeds: MessageEmbedsUpdated = {
      id: 101,
      channel_id: 99,
      suppress_embeds: true,
      embeds: [],
    };
    const reactions: MessageReactionsUpdated = {
      id: 101,
      channel_id: channelId,
      root_message_id: 999,
      user_id: 1,
      reactions: [],
    };
    expect(dispatch(state, { type: "embeds-updated", event: embeds })).toBe(state);
    expect(dispatch(state, { type: "reactions-updated", event: reactions, currentUserId: 1 })).toBe(
      state,
    );
  });
});

describe("threadReducer pagination", () => {
  it("tracks start and failure without changing messages", () => {
    const state = ready([message(103)]);
    const loading = dispatch(state, { type: "older-page-started" });
    const error = new Error("failed");
    const failed = dispatch(loading, { type: "older-page-failed", error });

    expect(loading.olderStatus).toBe("loading");
    expect(loading.replies).toBe(state.replies);
    expect(failed.olderStatus).toBe("error");
    expect(failed.olderError).toBe(error);
    expect(failed.replies).toBe(state.replies);
  });

  it("dedupes, orders older replies, retains current objects, and updates has-more", () => {
    const current = message(103, { created_at: 30 });
    const duplicate = message(103, { created_at: 30, text: "stale page copy" });
    const oldest = message(101, { created_at: 10 });
    const middle = message(102, { created_at: 20 });
    const state = dispatch(ready([current]), { type: "older-page-started" });
    const loaded = dispatch(state, {
      type: "older-page-succeeded",
      thread: thread([middle, duplicate, oldest], { has_more_replies: true }),
    });

    expect(loaded.replies.map(({ id }) => id)).toEqual([101, 102, 103]);
    expect(loaded.replies[2]).toBe(current);
    expect(loaded.hasMoreReplies).toBe(true);
    expect(loaded.olderStatus).toBe("idle");
  });

  it("does not resurrect a reply deleted while an older page was pending", () => {
    let state = dispatch(ready([message(103)]), { type: "older-page-started" });
    state = dispatch(state, { type: "reply-deleted", event: deleted(101) });
    state = dispatch(state, {
      type: "older-page-succeeded",
      thread: thread([message(101), message(102)]),
    });

    expect(state.replies.map(({ id }) => id)).toEqual([102, 103]);
  });

  it("tombstones an unseen reply hard-deleted while an older page is pending", () => {
    let state = dispatch(ready([message(103)]), { type: "older-page-started" });
    state = dispatch(state, { type: "message-deleted", messageId: 101 });
    state = dispatch(state, {
      type: "older-page-succeeded",
      thread: thread([message(101), message(102)]),
    });

    expect(state.replies.map(({ id }) => id)).toEqual([102, 103]);
    expect(state.deletedReplyIds.has(101)).toBe(true);
  });
});

describe("thread reducer identity and payload validation", () => {
  it("validates root ID, channel, and top-level identity, including malformed data", () => {
    expect(isValidThreadPayload(thread(), channelId, rootMessageId)).toBe(true);
    expect(isValidThreadPayload(thread([], { root: message(999) }), channelId, rootMessageId)).toBe(
      false,
    );
    expect(
      isValidThreadPayload(
        thread([], { root: message(rootMessageId, { channel_id: 99 }) }),
        channelId,
        rootMessageId,
      ),
    ).toBe(false);
    expect(
      isValidThreadPayload(
        thread([], { root: message(rootMessageId, { parent_id: 1 }) }),
        channelId,
        rootMessageId,
      ),
    ).toBe(false);
    expect(isValidThreadPayload(null, channelId, rootMessageId)).toBe(false);
    expect(isValidThreadPayload({ root: null }, channelId, rootMessageId)).toBe(false);
  });

  it("does not publish an invalid initial or older payload", () => {
    const loading = started();
    const invalidInitial = dispatch(loading, {
      type: "initial-load-succeeded",
      thread: thread([], { root: message(rootMessageId, { channel_id: 99 }) }),
    });
    expect(invalidInitial).toBe(loading);

    const state = dispatch(ready([message(103)]), { type: "older-page-started" });
    const invalidOlder = dispatch(state, {
      type: "older-page-succeeded",
      thread: thread([message(101)], { root: message(999) }),
    });
    expect(invalidOlder).toBe(state);
  });

  it("returns the same state for wrong channel, root, or stale generation", () => {
    const state = ready([message(101)]);
    const base = { type: "reply-created" as const, event: created(message(102)) };
    expect(threadReducer(state, { ...base, channelId: 99, rootMessageId, generation })).toBe(state);
    expect(threadReducer(state, { ...base, channelId, rootMessageId: 999, generation })).toBe(
      state,
    );
    expect(threadReducer(state, { ...base, channelId, rootMessageId, generation: 0 })).toBe(state);
  });

  it("accepts only a newer generation load start for the same root", () => {
    const state = ready([message(101)]);
    const stale = threadReducer(state, {
      type: "initial-load-started",
      channelId,
      rootMessageId,
      generation,
    });
    const next = threadReducer(state, {
      type: "initial-load-started",
      channelId,
      rootMessageId,
      generation: generation + 1,
    });

    expect(stale).toBe(state);
    expect(next.generation).toBe(2);
    expect(next.status).toBe("loading");
    expect(next.replies).toBe(state.replies);
  });

  it("retains unaffected root and reply object identity", () => {
    const first = message(101);
    const second = message(102);
    const state = ready([first, second]);
    const updated = dispatch(state, {
      type: "message-updated",
      message: message(101, { text: "edited" }),
    });

    expect(updated.root).toBe(state.root);
    expect(updated.replies[1]).toBe(second);
  });

  it("rejects malformed reply create/update payload identities", () => {
    const state = ready([message(101)]);
    const wrongCreate = created(message(102, { parent_id: 999 }));
    const wrongUpdate = message(101, { parent_id: 999 });

    expect(dispatch(state, { type: "reply-created", event: wrongCreate })).toBe(state);
    expect(dispatch(state, { type: "message-updated", message: wrongUpdate })).toBe(state);
  });
});
