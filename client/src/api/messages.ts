import { assertValidMessagePhotos } from "../photo-validation";
import { apiFetch } from "./client";
import type { Channel } from "./channels";
import type { PublicUser } from "./users";

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

export interface MessageAttachment {
  id: number;
  message_id: number;
  position: number;
  content_type: string;
  byte_size: number;
  width: number;
  height: number;
  url: string;
  thumbnail_url: string;
  thumbnail_content_type: string;
  thumbnail_byte_size: number;
  thumbnail_width: number;
  thumbnail_height: number;
}

export interface MessageReference {
  id: number;
  user_id: number;
  channel_id: number;
  created_at: number;
  deleted_at?: number | null;
  text: string;
  attachment_count?: number;
  username: string;
  display_name: string | null;
  avatar_url: string | null;
}

export type ReactionSummary =
  | {
      kind: "native";
      emoji: string;
      count: number;
      me_reacted: boolean;
      reactors?: string[];
    }
  | {
      kind: "custom";
      emoji_id: number;
      name: string;
      image_url: string;
      animated: boolean;
      deleted_at?: number | null;
      count: number;
      me_reacted: boolean;
      reactors?: string[];
    };

export type ReactionRequest =
  | {
      kind: "native";
      emoji: string;
    }
  | {
      kind: "custom";
      emoji_id: number;
      name?: string;
      image_url?: string;
      animated?: boolean;
    };

function reactionRequestBody(
  reaction: ReactionRequest,
): { kind: "native"; emoji: string } | { kind: "custom"; emoji_id: number } {
  return reaction.kind === "native"
    ? { kind: "native", emoji: reaction.emoji }
    : { kind: "custom", emoji_id: reaction.emoji_id };
}

export type MentionUser = PublicUser;

export interface Message {
  id: number;
  user_id: number;
  channel_id: number;
  parent_id?: number | null;
  reply_to_message_id?: number | null;
  reply_to?: MessageReference | null;
  created_at?: number;
  deleted_at?: number | null;
  text: string;
  username: string;
  display_name: string | null;
  avatar_url: string | null;
  suppress_embeds: boolean;
  mentions: MentionUser[];
  attachments: MessageAttachment[];
  embeds: Embed[];
  reactions?: ReactionSummary[];
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

export interface SendMessageOptions {
  replyToMessageId?: number | null;
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

export interface MessageReactionsUpdated {
  id: number;
  channel_id: number;
  parent_id?: number | null;
  root_message_id?: number;
  user_id: number;
  reactions: ReactionSummary[];
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

export function messageReferenceFromMessage(message: Message): MessageReference {
  return {
    id: message.id,
    user_id: message.user_id,
    channel_id: message.channel_id,
    created_at: message.created_at ?? message.id,
    deleted_at: message.deleted_at ?? null,
    text: message.text,
    attachment_count: message.attachments.length,
    username: message.username,
    display_name: message.display_name,
    avatar_url: message.avatar_url,
  };
}

export function messageReferencesTarget(message: Message, targetId: number): boolean {
  return message.reply_to_message_id === targetId || message.reply_to?.id === targetId;
}

export async function listMessages(channelId: string, signal?: AbortSignal): Promise<Message[]> {
  const res = await apiFetch(`/messages/${channelId}`, { signal });
  if (!res.ok) throw new Error(`Message history load failed (${res.status})`);
  return res.json() as Promise<Message[]>;
}

export async function sendMessage(
  channelId: string,
  text: string,
  photos: readonly File[] = [],
  options: SendMessageOptions = {},
): Promise<Response> {
  if (photos.length === 0) {
    const body: { text: string; reply_to_message_id?: number } = { text };
    if (options.replyToMessageId != null) body.reply_to_message_id = options.replyToMessageId;
    return apiFetch(`/message/${channelId}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
  }

  await assertValidMessagePhotos(photos);

  const body = new FormData();
  body.append("text", text);
  if (options.replyToMessageId != null) {
    body.append("reply_to_message_id", String(options.replyToMessageId));
  }
  for (const photo of photos) body.append("photos", photo, photo.name);

  return apiFetch(`/message/${channelId}`, {
    method: "POST",
    body,
  });
}

export async function listParticipatedThreads(
  signal?: AbortSignal,
): Promise<ParticipatedThreadPreview[]> {
  const res = await apiFetch("/threads/participated", { signal });
  if (!res.ok) throw new Error(`Participated threads load failed (${res.status})`);
  return res.json() as Promise<ParticipatedThreadPreview[]>;
}

export async function getThread(
  rootMessageId: number,
  options: ThreadPageOptions = {},
  signal?: AbortSignal,
): Promise<Thread> {
  const params = new URLSearchParams();
  if (options.limit !== undefined) params.set("limit", String(options.limit));
  if (options.beforeCreatedAt !== undefined) {
    params.set("before_created_at", String(options.beforeCreatedAt));
  }
  if (options.beforeId !== undefined) params.set("before_id", String(options.beforeId));
  const query = params.toString();
  const res = await apiFetch(`/thread/${rootMessageId}${query ? `?${query}` : ""}`, { signal });
  if (!res.ok) throw new Error(`Thread load failed (${res.status})`);
  return res.json() as Promise<Thread>;
}

export async function sendThreadReply(
  rootMessageId: number,
  text: string,
  photos: readonly File[] = [],
  signal?: AbortSignal,
): Promise<Message> {
  let res: Response;
  if (photos.length === 0) {
    res = await apiFetch(`/thread/${rootMessageId}/reply`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ text }),
      signal,
    });
  } else {
    await assertValidMessagePhotos(photos);

    const body = new FormData();
    body.append("text", text);
    for (const photo of photos) body.append("photos", photo, photo.name);

    res = await apiFetch(`/thread/${rootMessageId}/reply`, {
      method: "POST",
      body,
      signal,
    });
  }
  if (!res.ok) throw new Error(`Thread reply failed (${res.status})`);
  return res.json() as Promise<Message>;
}

export async function editMessage(
  messageId: number,
  text: string,
  signal?: AbortSignal,
): Promise<Message> {
  const res = await apiFetch(`/message/${messageId}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ text }),
    signal,
  });
  if (!res.ok) throw new Error(`Message edit failed (${res.status})`);
  return res.json() as Promise<Message>;
}

export async function deleteMessage(messageId: number, signal?: AbortSignal): Promise<void> {
  const res = await apiFetch(`/message/${messageId}`, { method: "DELETE", signal });
  if (!res.ok) throw new Error(`Message delete failed (${res.status})`);
}

export async function setMessageEmbedsSuppressed(
  messageId: number,
  suppress: boolean,
  signal?: AbortSignal,
): Promise<MessageEmbedsUpdated> {
  const res = await apiFetch(`/message/${messageId}/suppress_embeds`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ suppress }),
    signal,
  });
  if (!res.ok) throw new Error(`Suppress embeds failed (${res.status})`);
  return res.json() as Promise<MessageEmbedsUpdated>;
}

export async function addMessageReaction(
  messageId: number,
  reaction: ReactionRequest,
  signal?: AbortSignal,
): Promise<ReactionSummary[]> {
  const res = await apiFetch(`/message/${messageId}/reactions`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(reactionRequestBody(reaction)),
    signal,
  });
  if (!res.ok) throw new Error(`Add reaction failed (${res.status})`);
  return res.json() as Promise<ReactionSummary[]>;
}

export async function removeMessageReaction(
  messageId: number,
  reaction: ReactionRequest,
  signal?: AbortSignal,
): Promise<ReactionSummary[]> {
  const res = await apiFetch(`/message/${messageId}/reactions`, {
    method: "DELETE",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(reactionRequestBody(reaction)),
    signal,
  });
  if (!res.ok) throw new Error(`Remove reaction failed (${res.status})`);
  return res.json() as Promise<ReactionSummary[]>;
}
