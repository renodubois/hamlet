import { apiFetch } from "./client";

export interface CustomEmoji {
  id: number;
  name: string;
  image_url: string;
  animated: boolean;
  created_by_user_id: number;
  created_at: number;
  updated_at: number;
  deleted_at: number | null;
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

export async function listCustomEmojis(signal?: AbortSignal): Promise<CustomEmoji[]> {
  const res = await apiFetch("/emojis", { signal });
  if (!res.ok) throw new Error(`Emoji registry load failed (${res.status})`);
  return res.json() as Promise<CustomEmoji[]>;
}

export async function uploadCustomEmoji(name: string, file: Blob | File): Promise<CustomEmoji> {
  const form = new FormData();
  form.append("name", name);
  form.append("file", file);

  const res = await apiFetch("/emojis", {
    method: "POST",
    body: form,
  });
  if (!res.ok) throw await readApiError(res, "Emoji upload failed");
  return res.json() as Promise<CustomEmoji>;
}

export async function renameCustomEmoji(id: number, name: string): Promise<CustomEmoji> {
  const res = await apiFetch(`/emojis/${id}`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name }),
  });
  if (!res.ok) throw await readApiError(res, "Emoji rename failed");
  return res.json() as Promise<CustomEmoji>;
}

export async function deleteCustomEmoji(id: number): Promise<CustomEmoji> {
  const res = await apiFetch(`/emojis/${id}`, { method: "DELETE" });
  if (!res.ok) throw await readApiError(res, "Emoji delete failed");
  return res.json() as Promise<CustomEmoji>;
}

export async function restoreCustomEmoji(id: number): Promise<CustomEmoji> {
  const res = await apiFetch(`/emojis/${id}/restore`, { method: "POST" });
  if (!res.ok) throw await readApiError(res, "Emoji restore failed");
  return res.json() as Promise<CustomEmoji>;
}
