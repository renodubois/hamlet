import { apiFetch } from "./client";

export type EmbedType = "link" | "photo" | "video" | "rich";

export interface Embed {
  id: number;
  message_id: number;
  url: string;
  title: string | null;
  description: string | null;
  image_url: string | null;
  site_name: string | null;
  embed_type: EmbedType;
  iframe_url: string | null;
  iframe_width: number | null;
  iframe_height: number | null;
}

export interface Message {
  id: number;
  user_id: number;
  channel_id: number;
  text: string;
  username: string;
  display_name: string | null;
  avatar_url: string | null;
  suppress_embeds: boolean;
  embeds: Embed[];
}

export interface MessageDeleted {
  id: number;
  channel_id: number;
}

export interface MessageEmbedsUpdated {
  id: number;
  channel_id: number;
  suppress_embeds: boolean;
  embeds: Embed[];
}

export function messageDisplayName(msg: Pick<Message, "username" | "display_name">): string {
  return msg.display_name ?? msg.username;
}

export async function listMessages(channelId: string): Promise<Message[]> {
  const res = await apiFetch(`/messages/${channelId}`);
  return res.json() as Promise<Message[]>;
}

export async function sendMessage(channelId: string, text: string): Promise<Response> {
  return apiFetch(`/message/${channelId}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ text }),
  });
}

export async function editMessage(messageId: number, text: string): Promise<Message> {
  const res = await apiFetch(`/message/${messageId}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ text }),
  });
  if (!res.ok) throw new Error(`Message edit failed (${res.status})`);
  return res.json() as Promise<Message>;
}

export async function deleteMessage(messageId: number): Promise<void> {
  const res = await apiFetch(`/message/${messageId}`, { method: "DELETE" });
  if (!res.ok) throw new Error(`Message delete failed (${res.status})`);
}

export async function setMessageEmbedsSuppressed(
  messageId: number,
  suppress: boolean,
): Promise<MessageEmbedsUpdated> {
  const res = await apiFetch(`/message/${messageId}/suppress_embeds`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ suppress }),
  });
  if (!res.ok) throw new Error(`Suppress embeds failed (${res.status})`);
  return res.json() as Promise<MessageEmbedsUpdated>;
}
