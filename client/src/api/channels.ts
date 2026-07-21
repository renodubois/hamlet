import { apiFetch } from "./client";

export type ChannelType = "text" | "voice";

export interface Channel {
  id: number;
  name: string;
  position: number;
  type: ChannelType;
}

export async function listChannels(signal?: AbortSignal): Promise<Channel[]> {
  const res = await apiFetch("/channels", { signal });
  if (!res.ok) throw new Error(`Channel list failed (${res.status})`);
  return res.json() as Promise<Channel[]>;
}

export async function createChannel(name: string, type: ChannelType = "text"): Promise<Response> {
  return apiFetch("/channel", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name, type }),
  });
}

export async function reorderChannels(ids: number[]): Promise<Channel[]> {
  const res = await apiFetch("/channels/order", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ ids }),
  });
  if (!res.ok) throw new Error(`Channel reorder failed (${res.status})`);
  return res.json() as Promise<Channel[]>;
}
