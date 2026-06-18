import { apiFetch } from "./client";

export interface PublicUser {
  id: number;
  username: string;
  display_name: string | null;
  avatar_url: string | null;
}

export interface SearchUsersOptions {
  query?: string;
  limit?: number;
}

export async function searchUsers(options: SearchUsersOptions = {}): Promise<PublicUser[]> {
  const params = new URLSearchParams();
  if (options.query !== undefined) params.set("q", options.query);
  if (options.limit !== undefined) params.set("limit", String(options.limit));
  const query = params.toString();
  const res = await apiFetch(`/users${query ? `?${query}` : ""}`);
  if (!res.ok) throw new Error(`User search failed (${res.status})`);
  return res.json() as Promise<PublicUser[]>;
}
