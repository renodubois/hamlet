import type {
  Message,
  MessageDeleted,
  MessageEmbedsUpdated,
  MessageReactionsUpdated,
  ThreadReplyCreated,
  ThreadReplyDeleted,
} from "../api/messages";
import { messageReferenceFromMessage, messageReferencesTarget } from "../api/messages";
import type { PublicUser } from "../api/users";
import type { ResourceStatus } from "../hooks/use-resource";
import { mergeReactionUpdateForViewer } from "../reactions/reaction-summaries";

export interface ChannelMessageState {
  channelId: number;
  generation: number;
  status: ResourceStatus;
  error: unknown;
  messages: readonly Message[];
  liveActionsDuringLoad: readonly ChannelLiveAction[];
  deletedMessageIds: ReadonlySet<number>;
}

type ScopedAction = {
  channelId: number;
  generation: number;
};

export type ChannelLiveAction =
  | (ScopedAction & { type: "messageCreated"; message: Message })
  | (ScopedAction & { type: "messageUpdated"; message: Message })
  | (ScopedAction & { type: "messageHardDeleted"; deletion: MessageDeleted })
  | (ScopedAction & { type: "messageEmbedsUpdated"; update: MessageEmbedsUpdated })
  | (ScopedAction & {
      type: "messageReactionsUpdated";
      update: MessageReactionsUpdated;
      currentUserId: number | null;
    })
  | (ScopedAction & { type: "threadSummaryCreated"; update: ThreadReplyCreated })
  | (ScopedAction & { type: "threadSummaryDeleted"; update: ThreadReplyDeleted })
  | (ScopedAction & { type: "currentUserProfileUpdated"; user: PublicUser });

export type ChannelMessageAction =
  | { type: "loadStarted"; channelId: number; generation: number }
  | {
      type: "loadSucceeded";
      channelId: number;
      generation: number;
      messages: readonly Message[];
    }
  | { type: "loadFailed"; channelId: number; generation: number; error: unknown }
  | ChannelLiveAction;

export function createChannelMessageState(channelId: number, generation = 0): ChannelMessageState {
  return {
    channelId,
    generation,
    status: "idle",
    error: null,
    messages: [],
    liveActionsDuringLoad: [],
    deletedMessageIds: new Set(),
  };
}

function chronology(message: Message): readonly [number, number] {
  return [message.created_at ?? message.id, message.id];
}

function sortMessages(messages: readonly Message[]): Message[] {
  return [...messages].sort((left, right) => {
    const [leftCreatedAt, leftId] = chronology(left);
    const [rightCreatedAt, rightId] = chronology(right);
    return leftCreatedAt - rightCreatedAt || leftId - rightId;
  });
}

function canonicalSnapshot(
  messages: readonly Message[],
  channelId: number,
  deletedMessageIds: ReadonlySet<number>,
): Message[] {
  const byId = new Map<number, Message>();
  for (const message of messages) {
    if (
      message.channel_id === channelId &&
      message.parent_id == null &&
      !deletedMessageIds.has(message.id)
    ) {
      byId.set(message.id, message);
    }
  }
  return sortMessages([...byId.values()]);
}

function semanticallyEqual(left: unknown, right: unknown): boolean {
  return left === right || JSON.stringify(left) === JSON.stringify(right);
}

function upsertMessage(messages: readonly Message[], message: Message): readonly Message[] {
  const index = messages.findIndex((candidate) => candidate.id === message.id);
  if (index < 0) return sortMessages([...messages, message]);
  if (semanticallyEqual(messages[index], message)) return messages;
  const next = [...messages];
  next[index] = message;
  return sortMessages(next);
}

function patchReferences(messages: readonly Message[], target: Message): readonly Message[] {
  let changed = false;
  const reference = messageReferenceFromMessage(target);
  const next = messages.map((message) => {
    if (!messageReferencesTarget(message, target.id)) return message;
    if (
      message.reply_to_message_id === target.id &&
      semanticallyEqual(message.reply_to, reference)
    ) {
      return message;
    }
    changed = true;
    return {
      ...message,
      reply_to_message_id: target.id,
      reply_to: reference,
    };
  });
  return changed ? next : messages;
}

function applyMessageUpdate(messages: readonly Message[], message: Message): readonly Message[] {
  const targetIndex = messages.findIndex((candidate) => candidate.id === message.id);
  let withTarget: readonly Message[] = messages;
  if (targetIndex >= 0 && !semanticallyEqual(messages[targetIndex], message)) {
    const replacement = [...messages];
    replacement[targetIndex] = message;
    withTarget = replacement;
  }
  const next = patchReferences(withTarget, message);
  return withTarget !== messages ? sortMessages(next) : next;
}

function applyHardDelete(messages: readonly Message[], targetId: number): readonly Message[] {
  let changed = false;
  const next: Message[] = [];
  for (const message of messages) {
    if (message.id === targetId) {
      changed = true;
      continue;
    }
    if (messageReferencesTarget(message, targetId) && message.reply_to !== null) {
      next.push({ ...message, reply_to_message_id: targetId, reply_to: null });
      changed = true;
    } else {
      next.push(message);
    }
  }
  return changed ? next : messages;
}

function patchMessage(
  messages: readonly Message[],
  messageId: number,
  patch: (message: Message) => Message,
): readonly Message[] {
  const index = messages.findIndex((message) => message.id === messageId);
  if (index < 0) return messages;
  const current = messages[index] as Message;
  const patched = patch(current);
  if (patched === current) return messages;
  const next = [...messages];
  next[index] = patched;
  return next;
}

function applyProfileUpdate(messages: readonly Message[], user: PublicUser): readonly Message[] {
  let changed = false;
  const next = messages.map((message): Message => {
    const authored = message.user_id === user.id;
    const reference = message.reply_to;
    const referenced = reference?.user_id === user.id;
    if (!authored && !referenced) return message;
    const authorChanged =
      authored &&
      (message.username !== user.username ||
        message.display_name !== user.display_name ||
        message.avatar_url !== user.avatar_url);
    const referenceChanged =
      referenced &&
      reference != null &&
      (reference.username !== user.username ||
        reference.display_name !== user.display_name ||
        reference.avatar_url !== user.avatar_url);
    if (!authorChanged && !referenceChanged) return message;
    changed = true;
    return {
      ...message,
      ...(authorChanged
        ? {
            username: user.username,
            display_name: user.display_name,
            avatar_url: user.avatar_url,
          }
        : {}),
      ...(referenceChanged && reference
        ? {
            reply_to: {
              ...reference,
              username: user.username,
              display_name: user.display_name,
              avatar_url: user.avatar_url,
            },
          }
        : {}),
    };
  });
  return changed ? next : messages;
}

function applyLiveAction(
  messages: readonly Message[],
  action: ChannelLiveAction,
): readonly Message[] {
  switch (action.type) {
    case "messageCreated":
      return action.message.channel_id === action.channelId && action.message.parent_id == null
        ? upsertMessage(messages, action.message)
        : messages;
    case "messageUpdated":
      return action.message.channel_id === action.channelId
        ? applyMessageUpdate(messages, action.message)
        : messages;
    case "messageHardDeleted":
      return action.deletion.channel_id === action.channelId
        ? applyHardDelete(messages, action.deletion.id)
        : messages;
    case "messageEmbedsUpdated":
      return action.update.channel_id === action.channelId
        ? patchMessage(messages, action.update.id, (message) =>
            message.suppress_embeds === action.update.suppress_embeds &&
            semanticallyEqual(message.embeds, action.update.embeds)
              ? message
              : {
                  ...message,
                  suppress_embeds: action.update.suppress_embeds,
                  embeds: action.update.embeds,
                },
          )
        : messages;
    case "messageReactionsUpdated":
      return action.update.channel_id === action.channelId
        ? patchMessage(messages, action.update.id, (message) => {
            const reactions = mergeReactionUpdateForViewer(
              message.reactions ?? [],
              action.update.reactions,
              action.update.user_id,
              action.currentUserId,
            );
            return semanticallyEqual(message.reactions ?? [], reactions)
              ? message
              : { ...message, reactions };
          })
        : messages;
    case "threadSummaryCreated":
      return action.update.channel_id === action.channelId
        ? patchMessage(messages, action.update.root_message_id, (message) =>
            message.parent_id == null
              ? { ...message, thread_summary: action.update.thread_summary }
              : message,
          )
        : messages;
    case "threadSummaryDeleted":
      return action.update.channel_id === action.channelId
        ? patchMessage(messages, action.update.root_message_id, (message) =>
            message.parent_id == null
              ? { ...message, thread_summary: action.update.thread_summary ?? undefined }
              : message,
          )
        : messages;
    case "currentUserProfileUpdated":
      return applyProfileUpdate(messages, action.user);
  }
}

export function channelMessageReducer(
  state: ChannelMessageState,
  action: ChannelMessageAction,
): ChannelMessageState {
  if (action.type === "loadStarted") {
    if (action.generation <= state.generation) return state;
    return {
      channelId: action.channelId,
      generation: action.generation,
      status: "loading",
      error: null,
      messages: action.channelId === state.channelId ? state.messages : [],
      liveActionsDuringLoad: [],
      deletedMessageIds: action.channelId === state.channelId ? state.deletedMessageIds : new Set(),
    };
  }

  if (action.channelId !== state.channelId || action.generation !== state.generation) {
    return state;
  }

  if (action.type === "loadSucceeded") {
    let messages: readonly Message[] = canonicalSnapshot(
      action.messages,
      state.channelId,
      state.deletedMessageIds,
    );
    for (const liveAction of state.liveActionsDuringLoad) {
      messages = applyLiveAction(messages, liveAction);
    }
    return {
      ...state,
      status: "ready",
      error: null,
      messages,
      liveActionsDuringLoad: [],
    };
  }

  if (action.type === "loadFailed") {
    return {
      ...state,
      status: "error",
      error: action.error,
      liveActionsDuringLoad: [],
    };
  }

  const payloadChannelId =
    action.type === "messageCreated" || action.type === "messageUpdated"
      ? action.message.channel_id
      : action.type === "messageHardDeleted"
        ? action.deletion.channel_id
        : action.type === "messageEmbedsUpdated" ||
            action.type === "messageReactionsUpdated" ||
            action.type === "threadSummaryCreated" ||
            action.type === "threadSummaryDeleted"
          ? action.update.channel_id
          : action.channelId;
  // Do not journal or tombstone malformed actions whose outer scope claims the
  // active channel while their payload belongs to another channel.
  if (payloadChannelId !== state.channelId) return state;
  if (action.type === "messageCreated" && state.deletedMessageIds.has(action.message.id)) {
    return state;
  }
  const messages = applyLiveAction(state.messages, action);
  const liveActionsDuringLoad =
    state.status === "loading"
      ? [...state.liveActionsDuringLoad, action]
      : state.liveActionsDuringLoad;
  let deletedMessageIds = state.deletedMessageIds;
  if (action.type === "messageHardDeleted" && !deletedMessageIds.has(action.deletion.id)) {
    const nextDeletedMessageIds = new Set(deletedMessageIds);
    nextDeletedMessageIds.add(action.deletion.id);
    deletedMessageIds = nextDeletedMessageIds;
  }
  if (
    messages === state.messages &&
    liveActionsDuringLoad === state.liveActionsDuringLoad &&
    deletedMessageIds === state.deletedMessageIds
  ) {
    return state;
  }
  return { ...state, messages, liveActionsDuringLoad, deletedMessageIds };
}
