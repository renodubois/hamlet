const DEFAULT_SERVER = "http://localhost:3030";

export interface User {
  id: number;
  username: string;
  email: string | null;
  email_verified: boolean;
}

export interface Channel {
  id: number;
  name: string;
}

export interface Message {
  id: number;
  user_id: number;
  channel_id: number;
  text: string;
  username: string;
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

export function messagesEventSource(): EventSource {
  return new EventSource(`${getServerUrl()}/messages/subscribe`, {
    withCredentials: true,
  });
}
