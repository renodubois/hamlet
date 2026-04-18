const DEFAULT_SERVER = "http://localhost:3030";

export const CHANNEL_NAME_MAX_LEN = 128;

export interface User {
  id: number;
  username: string;
  email: string | null;
  email_verified: boolean;
  avatar_url: string | null;
}

export interface Channel {
  id: number;
  name: string;
  position: number;
}

export interface Message {
  id: number;
  user_id: number;
  channel_id: number;
  text: string;
  username: string;
  avatar_url: string | null;
}

export function getServerUrl(): string {
  return localStorage.getItem("hamlet.serverUrl") ?? DEFAULT_SERVER;
}

export function setServerUrl(url: string): void {
  localStorage.setItem("hamlet.serverUrl", url);
}

async function apiFetch(path: string, init?: RequestInit): Promise<Response> {
  return fetch(`${getServerUrl()}${path}`, { credentials: "include", ...init });
}

export async function getMe(): Promise<User | null> {
  try {
    const res = await apiFetch("/me");
    if (res.status === 401) return null;
    return res.json() as Promise<User>;
  } catch {
    return null;
  }
}

export async function login(username: string, password: string): Promise<Response> {
  return apiFetch("/login", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ username, password }),
  });
}

export async function register(
  username: string,
  password: string,
  email?: string,
): Promise<Response> {
  return apiFetch("/register", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ username, password, email: email ?? null }),
  });
}

export async function logout(): Promise<void> {
  await apiFetch("/logout", { method: "POST" });
}

export async function listChannels(): Promise<Channel[]> {
  const res = await apiFetch("/channels");
  return res.json() as Promise<Channel[]>;
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

export async function createChannel(name: string): Promise<Response> {
  return apiFetch("/channel", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name }),
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

export async function uploadAvatar(blob: Blob): Promise<User> {
  const form = new FormData();
  form.append("file", blob, "avatar.webp");
  const res = await apiFetch("/me/avatar", { method: "POST", body: form });
  if (!res.ok) throw new Error(`Avatar upload failed (${res.status})`);
  return res.json() as Promise<User>;
}

export async function deleteAvatar(): Promise<User> {
  const res = await apiFetch("/me/avatar", { method: "DELETE" });
  if (!res.ok) throw new Error(`Avatar delete failed (${res.status})`);
  return res.json() as Promise<User>;
}

export interface MessageDeleted {
  id: number;
  channel_id: number;
}

export type SSEEvent =
  | { kind: "message"; data: Message }
  | { kind: "message_updated"; data: Message }
  | { kind: "message_deleted"; data: MessageDeleted }
  | { kind: "channel_created"; data: Channel }
  | { kind: "channels_reordered"; data: Channel[] };

export function messagesEventSource(): EventSource {
  return new EventSource(`${getServerUrl()}/messages/subscribe`, {
    withCredentials: true,
  });
}
