import type {
  Message,
  MessageEmbedsUpdated,
  MessageReactionsUpdated,
  Thread,
  ThreadReplyCreated,
  ThreadReplyDeleted,
} from "../api";
import type { ResourceStatus } from "../hooks/use-resource";
import { messageReferenceFromMessage, messageReferencesTarget } from "../api";
import {
  mergeReactionUpdateForViewer,
  reactionSummariesEqual,
} from "../reactions/reaction-summaries";

export type OlderThreadStatus = "idle" | "loading" | "error";

export interface ThreadState {
  channelId: number;
  rootMessageId: number;
  generation: number;
  status: ResourceStatus;
  // `null` is the explicit no-error sentinel.
  // oxlint-disable-next-line typescript/no-redundant-type-constituents
  error: unknown | null;
  root: Message | null;
  replies: readonly Message[];
  hasMoreReplies: boolean;
  liveActionsDuringLoad: readonly ThreadLiveAction[];
  deletedReplyIds: ReadonlySet<number>;
  rootHardDeleted: boolean;
  olderStatus: OlderThreadStatus;
  // oxlint-disable-next-line typescript/no-redundant-type-constituents
  olderError: unknown | null;
}

interface ThreadIdentity {
  channelId: number;
  rootMessageId: number;
  generation: number;
}

export type ThreadLiveAction =
  | (ThreadIdentity & { type: "reply-created"; event: ThreadReplyCreated })
  | (ThreadIdentity & { type: "reply-deleted"; event: ThreadReplyDeleted })
  | (ThreadIdentity & { type: "message-updated"; message: Message })
  | (ThreadIdentity & { type: "message-deleted"; messageId: number })
  | (ThreadIdentity & { type: "embeds-updated"; event: MessageEmbedsUpdated })
  | (ThreadIdentity & {
      type: "reactions-updated";
      event: MessageReactionsUpdated;
      currentUserId: number | null;
    });

export type ThreadAction =
  | (ThreadIdentity & { type: "initial-load-started" })
  | (ThreadIdentity & { type: "initial-load-succeeded"; thread: Thread })
  | (ThreadIdentity & { type: "initial-load-failed"; error: unknown })
  | (ThreadIdentity & { type: "older-page-started" })
  | (ThreadIdentity & { type: "older-page-succeeded"; thread: Thread })
  | (ThreadIdentity & { type: "older-page-failed"; error: unknown })
  | ThreadLiveAction;

export function createThreadState(
  channelId: number,
  rootMessageId: number,
  generation = 0,
): ThreadState {
  return {
    channelId,
    rootMessageId,
    generation,
    status: "idle",
    error: null,
    root: null,
    replies: [],
    hasMoreReplies: false,
    liveActionsDuringLoad: [],
    deletedReplyIds: new Set(),
    rootHardDeleted: false,
    olderStatus: "idle",
    olderError: null,
  };
}

/** Validate the security-sensitive identity fields of a GET /thread payload. */
export function isValidThreadPayload(
  thread: unknown,
  channelId: number,
  rootMessageId: number,
): thread is Thread {
  if (typeof thread !== "object" || thread === null || !("root" in thread)) return false;
  const root = thread.root;
  if (typeof root !== "object" || root === null) return false;
  return (
    "id" in root &&
    root.id === rootMessageId &&
    "channel_id" in root &&
    root.channel_id === channelId &&
    (!("parent_id" in root) || root.parent_id == null)
  );
}

function sameIdentity(state: ThreadState, action: ThreadIdentity): boolean {
  return (
    state.channelId === action.channelId &&
    state.rootMessageId === action.rootMessageId &&
    state.generation === action.generation
  );
}

function sameRoot(state: ThreadState, action: ThreadIdentity): boolean {
  return state.channelId === action.channelId && state.rootMessageId === action.rootMessageId;
}

function chronology(message: Message): readonly [number, number] {
  return [message.created_at ?? message.id, message.id];
}

function compareMessages(left: Message, right: Message): number {
  const [leftCreated, leftId] = chronology(left);
  const [rightCreated, rightId] = chronology(right);
  return leftCreated - rightCreated || leftId - rightId;
}

function canonicalReplies(state: ThreadState, replies: readonly Message[]): Message[] {
  const byId = new Map<number, Message>();
  for (const reply of replies) {
    if (
      reply.channel_id === state.channelId &&
      reply.parent_id === state.rootMessageId &&
      !state.deletedReplyIds.has(reply.id)
    ) {
      const existing = state.replies.find((candidate) => candidate.id === reply.id);
      byId.set(reply.id, existing && semanticallyEqual(existing, reply) ? existing : reply);
    }
  }
  return [...byId.values()].sort(compareMessages);
}

function semanticallyEqual(left: unknown, right: unknown): boolean {
  return left === right || JSON.stringify(left) === JSON.stringify(right);
}

function upsertReply(replies: readonly Message[], reply: Message): readonly Message[] {
  const index = replies.findIndex((candidate) => candidate.id === reply.id);
  if (index >= 0 && semanticallyEqual(replies[index], reply)) return replies;
  const next =
    index < 0 ? [...replies, reply] : replies.map((item, i) => (i === index ? reply : item));
  return next.sort(compareMessages);
}

function patchReference(candidate: Message, target: Message): Message {
  if (!messageReferencesTarget(candidate, target.id)) return candidate;
  const reference = messageReferenceFromMessage(target);
  if (
    candidate.reply_to_message_id === target.id &&
    semanticallyEqual(candidate.reply_to, reference)
  ) {
    return candidate;
  }
  return { ...candidate, reply_to_message_id: target.id, reply_to: reference };
}

function invalidateReference(candidate: Message, targetId: number): Message {
  if (!messageReferencesTarget(candidate, targetId)) return candidate;
  if (candidate.reply_to_message_id === targetId && candidate.reply_to == null) return candidate;
  return { ...candidate, reply_to_message_id: targetId, reply_to: null };
}

function applyLiveAction(state: ThreadState, action: ThreadLiveAction): ThreadState {
  if (state.rootHardDeleted) return state;
  switch (action.type) {
    case "reply-created": {
      const { event } = action;
      if (
        event.channel_id !== state.channelId ||
        event.root_message_id !== state.rootMessageId ||
        event.reply.channel_id !== state.channelId ||
        event.reply.parent_id !== state.rootMessageId ||
        state.deletedReplyIds.has(event.reply.id)
      ) {
        return state;
      }
      const replies = upsertReply(state.replies, event.reply);
      return replies === state.replies ? state : { ...state, replies };
    }
    case "reply-deleted": {
      const { event } = action;
      if (event.channel_id !== state.channelId || event.root_message_id !== state.rootMessageId) {
        return state;
      }
      const replies = state.replies.filter((reply) => reply.id !== event.reply_id);
      if (state.deletedReplyIds.has(event.reply_id) && replies.length === state.replies.length) {
        return state;
      }
      const deletedReplyIds = new Set(state.deletedReplyIds);
      deletedReplyIds.add(event.reply_id);
      return { ...state, replies, deletedReplyIds };
    }
    case "message-updated": {
      const { message } = action;
      if (message.channel_id !== state.channelId) return state;
      const isRoot = message.id === state.rootMessageId;
      const replyIndex = state.replies.findIndex((reply) => reply.id === message.id);
      if (isRoot && message.parent_id != null) return state;
      if (replyIndex >= 0 && message.parent_id !== state.rootMessageId) return state;

      let changed = false;
      let root = state.root;
      if (isRoot) {
        if (!semanticallyEqual(root, message)) {
          changed = true;
          root = message;
        }
      } else if (root) {
        const patched = patchReference(root, message);
        changed ||= patched !== root;
        root = patched;
      }

      const replies = state.replies.map((reply, index) => {
        const next =
          index === replyIndex && !semanticallyEqual(reply, message)
            ? message
            : patchReference(reply, message);
        changed ||= next !== reply;
        return next;
      });
      return changed ? { ...state, root, replies } : state;
    }
    case "message-deleted": {
      const { messageId } = action;
      let changed = false;
      let root = state.root;
      let replies = state.replies;
      let deletedReplyIds = state.deletedReplyIds;
      let rootHardDeleted: boolean = state.rootHardDeleted;

      if (messageId === state.rootMessageId) {
        if (rootHardDeleted) return state;
        changed = true;
        root = null;
        rootHardDeleted = true;
      } else {
        const hadReply = replies.some((reply) => reply.id === messageId);
        if (hadReply) {
          replies = replies.filter((reply) => reply.id !== messageId);
          changed = true;
        }
        if (!deletedReplyIds.has(messageId)) {
          const nextDeletedReplyIds = new Set(deletedReplyIds);
          nextDeletedReplyIds.add(messageId);
          deletedReplyIds = nextDeletedReplyIds;
          changed = true;
        }
      }

      if (root) {
        const patched = invalidateReference(root, messageId);
        changed ||= patched !== root;
        root = patched;
      }
      replies = replies.map((reply) => {
        const patched = invalidateReference(reply, messageId);
        changed ||= patched !== reply;
        return patched;
      });
      return changed ? { ...state, root, replies, deletedReplyIds, rootHardDeleted } : state;
    }
    case "embeds-updated": {
      const { event } = action;
      if (event.channel_id !== state.channelId) return state;
      if (state.root?.id === event.id) {
        if (
          state.root.suppress_embeds === event.suppress_embeds &&
          semanticallyEqual(state.root.embeds, event.embeds)
        )
          return state;
        return {
          ...state,
          root: { ...state.root, suppress_embeds: event.suppress_embeds, embeds: event.embeds },
        };
      }
      const index = state.replies.findIndex((reply) => reply.id === event.id);
      if (index < 0) return state;
      const current = state.replies[index];
      if (
        !current ||
        (current.suppress_embeds === event.suppress_embeds &&
          semanticallyEqual(current.embeds, event.embeds))
      )
        return state;
      const updated = { ...current, suppress_embeds: event.suppress_embeds, embeds: event.embeds };
      return {
        ...state,
        replies: state.replies.map((reply, i) => (i === index ? updated : reply)),
      };
    }
    case "reactions-updated": {
      const { event } = action;
      if (event.channel_id !== state.channelId) return state;
      const eventRootId = event.root_message_id ?? event.parent_id ?? event.id;
      if (event.id !== state.rootMessageId && eventRootId !== state.rootMessageId) return state;
      if (state.root?.id === event.id) {
        const reactions = mergeReactionUpdateForViewer(
          state.root.reactions ?? [],
          event.reactions,
          event.user_id,
          action.currentUserId,
        );
        if (reactionSummariesEqual(state.root.reactions ?? [], reactions)) return state;
        return { ...state, root: { ...state.root, reactions } };
      }
      const index = state.replies.findIndex((reply) => reply.id === event.id);
      if (index < 0) return state;
      const current = state.replies[index];
      if (!current) return state;
      const reactions = mergeReactionUpdateForViewer(
        current.reactions ?? [],
        event.reactions,
        event.user_id,
        action.currentUserId,
      );
      if (reactionSummariesEqual(current.reactions ?? [], reactions)) return state;
      const updated = { ...current, reactions };
      return {
        ...state,
        replies: state.replies.map((reply, i) => (i === index ? updated : reply)),
      };
    }
  }
}

function hasValidLivePayload(state: ThreadState, action: ThreadLiveAction): boolean {
  switch (action.type) {
    case "reply-created":
      return (
        action.event.channel_id === state.channelId &&
        action.event.root_message_id === state.rootMessageId &&
        action.event.reply.channel_id === state.channelId &&
        action.event.reply.parent_id === state.rootMessageId
      );
    case "reply-deleted":
      return (
        action.event.channel_id === state.channelId &&
        action.event.root_message_id === state.rootMessageId
      );
    case "message-updated":
      return action.message.channel_id === state.channelId;
    case "message-deleted":
      return true;
    case "embeds-updated":
      return action.event.channel_id === state.channelId;
    case "reactions-updated": {
      const eventRootId = action.event.root_message_id ?? action.event.parent_id ?? action.event.id;
      return (
        action.event.channel_id === state.channelId &&
        (action.event.id === state.rootMessageId || eventRootId === state.rootMessageId)
      );
    }
  }
}

function withJournal(state: ThreadState, action: ThreadLiveAction): ThreadState {
  if (!hasValidLivePayload(state, action)) return state;
  const applied = applyLiveAction(state, action);
  if (state.status !== "loading") return applied;
  return { ...applied, liveActionsDuringLoad: [...applied.liveActionsDuringLoad, action] };
}

export function threadReducer(state: ThreadState, action: ThreadAction): ThreadState {
  if (action.type === "initial-load-started") {
    if (state.rootHardDeleted || !sameRoot(state, action) || action.generation <= state.generation)
      return state;
    return {
      ...state,
      generation: action.generation,
      status: "loading",
      error: null,
      liveActionsDuringLoad: [],
      olderStatus: "idle",
      olderError: null,
    };
  }
  if (!sameIdentity(state, action)) return state;

  switch (action.type) {
    case "initial-load-succeeded": {
      if (
        state.rootHardDeleted ||
        !isValidThreadPayload(action.thread, state.channelId, state.rootMessageId)
      )
        return state;
      let loaded: ThreadState = {
        ...state,
        status: "ready",
        error: null,
        root: action.thread.root,
        replies: canonicalReplies(state, action.thread.replies),
        hasMoreReplies: action.thread.has_more_replies,
        liveActionsDuringLoad: [],
      };
      for (const liveAction of state.liveActionsDuringLoad)
        loaded = applyLiveAction(loaded, liveAction);
      return loaded;
    }
    case "initial-load-failed":
      return {
        ...state,
        status: "error",
        error: action.error,
        liveActionsDuringLoad: [],
      };
    case "older-page-started":
      if (state.olderStatus === "loading") return state;
      return { ...state, olderStatus: "loading", olderError: null };
    case "older-page-succeeded": {
      if (
        state.rootHardDeleted ||
        !isValidThreadPayload(action.thread, state.channelId, state.rootMessageId)
      )
        return state;
      const byId = new Map(state.replies.map((reply) => [reply.id, reply]));
      for (const reply of action.thread.replies) {
        if (
          reply.channel_id === state.channelId &&
          reply.parent_id === state.rootMessageId &&
          !state.deletedReplyIds.has(reply.id) &&
          !byId.has(reply.id)
        ) {
          byId.set(reply.id, reply);
        }
      }
      return {
        ...state,
        replies: [...byId.values()].sort(compareMessages),
        hasMoreReplies: action.thread.has_more_replies,
        olderStatus: "idle",
        olderError: null,
      };
    }
    case "older-page-failed":
      return { ...state, olderStatus: "error", olderError: action.error };
    default:
      return withJournal(state, action);
  }
}
