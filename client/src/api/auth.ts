import { apiFetch } from "./client";

export interface User {
  id: number;
  username: string;
  display_name: string | null;
  email: string | null;
  email_verified: boolean;
  avatar_url: string | null;
}

export async function getMe(signal?: AbortSignal): Promise<User | null> {
  const res = await apiFetch("/me", { signal });
  if (res.status === 401) return null;
  if (!res.ok) throw new Error(`Current user request failed (${res.status})`);
  return res.json() as Promise<User>;
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
  const res = await apiFetch("/logout", { method: "POST" });
  if (!res.ok) throw new Error(`Logout failed (${res.status})`);
}

async function readApiError(res: Response, fallback: string): Promise<Error> {
  try {
    const body = (await res.json()) as { error?: { message?: string } };
    const message = body.error?.message;
    if (message) return new Error(message);
  } catch {
    // Fall through to the generic status message.
  }
  return new Error(`${fallback} (${res.status})`);
}

export async function changePassword(currentPassword: string, newPassword: string): Promise<void> {
  const res = await apiFetch("/me/password", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ current_password: currentPassword, new_password: newPassword }),
  });
  if (!res.ok) throw await readApiError(res, "Password change failed");
}

export async function updateDisplayName(displayName: string | null): Promise<User> {
  const res = await apiFetch("/me", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ display_name: displayName }),
  });
  if (!res.ok) throw new Error(`Display name update failed (${res.status})`);
  return res.json() as Promise<User>;
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
