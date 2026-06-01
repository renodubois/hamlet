import { apiFetch } from "./client";
import type { Channel } from "./channels";

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

export interface ThreadSummary {
  reply_count: number;
  last_reply_created_at: number;
}

export interface Message {
  id: number;
  user_id: number;
  channel_id: number;
  parent_id?: number | null;
  created_at?: number;
  deleted_at?: number | null;
  text: string;
  username: string;
  display_name: string | null;
  avatar_url: string | null;
  suppress_embeds: boolean;
  embeds: Embed[];
  thread_summary?: ThreadSummary;
}

export interface Thread {
  root: Message;
  replies: Message[];
  has_more_replies: boolean;
}

export interface ParticipatedThreadPreview {
  channel: Channel;
  root: Message;
  reply_count: number;
  last_reply_created_at: number;
  recent_replies: Message[];
}

export interface ThreadPageOptions {
  limit?: number;
  beforeCreatedAt?: number;
  beforeId?: number;
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

export interface ThreadReplyCreated {
  channel_id: number;
  root_message_id: number;
  reply: Message;
  thread_summary: ThreadSummary;
}

export interface ThreadReplyDeleted {
  channel_id: number;
  root_message_id: number;
  reply_id: number;
  thread_summary?: ThreadSummary | null;
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

export async function listParticipatedThreads(): Promise<ParticipatedThreadPreview[]> {
  const res = await apiFetch("/threads/participated");
  if (!res.ok) throw new Error(`Participated threads load failed (${res.status})`);
  return res.json() as Promise<ParticipatedThreadPreview[]>;
}

export async function getThread(
  rootMessageId: number,
  options: ThreadPageOptions = {},
): Promise<Thread> {
  const params = new URLSearchParams();
  if (options.limit !== undefined) params.set("limit", String(options.limit));
  if (options.beforeCreatedAt !== undefined) {
    params.set("before_created_at", String(options.beforeCreatedAt));
  }
  if (options.beforeId !== undefined) params.set("before_id", String(options.beforeId));
  const query = params.toString();
  const res = await apiFetch(`/thread/${rootMessageId}${query ? `?${query}` : ""}`);
  if (!res.ok) throw new Error(`Thread load failed (${res.status})`);
  return res.json() as Promise<Thread>;
}

export async function sendThreadReply(rootMessageId: number, text: string): Promise<Message> {
  const res = await apiFetch(`/thread/${rootMessageId}/reply`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ text }),
  });
  if (!res.ok) throw new Error(`Thread reply failed (${res.status})`);
  return res.json() as Promise<Message>;
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
