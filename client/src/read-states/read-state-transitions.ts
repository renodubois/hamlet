import type { Message, ReadStateSummary } from "../api";

export type ReadStateByChannel = Record<number, ReadStateSummary>;

export type ReadStateJournalEntry =
  | { kind: "summary"; summary: ReadStateSummary }
  | { kind: "message"; message: Message; currentUserId: number };

export function applyReadStateSnapshot(snapshot: readonly ReadStateSummary[]): ReadStateByChannel {
  const next: ReadStateByChannel = {};
  for (const summary of snapshot) {
    next[summary.channel_id] = { ...summary };
  }
  return next;
}

function compareCursor(left: ReadStateSummary, right: ReadStateSummary): number {
  if (left.last_read_created_at !== right.last_read_created_at) {
    return left.last_read_created_at - right.last_read_created_at;
  }
  return left.last_read_message_id - right.last_read_message_id;
}

/** True when an authoritative summary may replace the currently accepted one. */
export function isReadStateSummaryCurrent(
  current: ReadStateSummary | undefined,
  incoming: ReadStateSummary,
): boolean {
  if (!current) return true;
  if (incoming.updated_at !== current.updated_at) return incoming.updated_at > current.updated_at;
  return compareCursor(incoming, current) >= 0;
}

export function applyReadStateUpdate(
  states: ReadStateByChannel,
  summary: ReadStateSummary,
): ReadStateByChannel {
  if (!isReadStateSummaryCurrent(states[summary.channel_id], summary)) return states;
  return { ...states, [summary.channel_id]: { ...summary } };
}

function isAfterReadCursor(summary: ReadStateSummary, message: Message): boolean {
  if (message.created_at == null) return false;
  if (message.created_at !== summary.last_read_created_at) {
    return message.created_at > summary.last_read_created_at;
  }
  return message.id > summary.last_read_message_id;
}

function mentionsUser(message: Message, userId: number): boolean {
  return message.mentions.some((mention) => mention.id === userId);
}

export function applyIncomingTopLevelMessage(
  states: ReadStateByChannel,
  message: Message,
  currentUserId: number,
): ReadStateByChannel {
  if (
    message.parent_id != null ||
    message.deleted_at != null ||
    message.user_id === currentUserId
  ) {
    return states;
  }

  const current = states[message.channel_id];
  if (!current || !isAfterReadCursor(current, message)) return states;

  return {
    ...states,
    [message.channel_id]: {
      ...current,
      has_unread: true,
      mention_count: current.mention_count + (mentionsUser(message, currentUserId) ? 1 : 0),
    },
  };
}

/** Replays realtime changes observed after a snapshot began over that snapshot. */
export function mergeReadStateSnapshot(
  snapshot: readonly ReadStateSummary[],
  journal: readonly ReadStateJournalEntry[],
): ReadStateByChannel {
  return journal.reduce((states, entry) => {
    if (entry.kind === "summary") return applyReadStateUpdate(states, entry.summary);
    return applyIncomingTopLevelMessage(states, entry.message, entry.currentUserId);
  }, applyReadStateSnapshot(snapshot));
}

export function readStateForChannel(
  states: ReadStateByChannel,
  channelId: number,
): ReadStateSummary | undefined {
  return states[channelId];
}

export function channelHasUnread(states: ReadStateByChannel, channelId: number): boolean {
  return readStateForChannel(states, channelId)?.has_unread ?? false;
}

export function channelMentionCount(states: ReadStateByChannel, channelId: number): number {
  return readStateForChannel(states, channelId)?.mention_count ?? 0;
}
