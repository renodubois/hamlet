import { apiFetch } from "./client";

export interface ReadStateSummary {
  channel_id: number;
  has_unread: boolean;
  mention_count: number;
  last_read_created_at: number;
  last_read_message_id: number;
  updated_at: number;
}

export async function listReadStates(signal?: AbortSignal): Promise<ReadStateSummary[]> {
  const res = await apiFetch("/read-states", { signal });
  if (!res.ok) throw new Error(`Read-state snapshot failed (${res.status})`);
  return res.json() as Promise<ReadStateSummary[]>;
}

export async function markChannelRead(
  channelId: number,
  lastVisibleMessageId: number,
): Promise<ReadStateSummary> {
  const res = await apiFetch(`/channels/${channelId}/read-state`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ last_visible_message_id: lastVisibleMessageId }),
  });
  if (!res.ok) throw new Error(`Mark read failed (${res.status})`);
  return res.json() as Promise<ReadStateSummary>;
}
