import { apiFetch } from "./client";

export interface UserTyping {
  channel_id: number;
  user_id: number;
  username: string;
}

export async function sendTyping(channelId: string): Promise<void> {
  await apiFetch(`/typing/${channelId}`, { method: "POST" }).catch(() => {
    // Best-effort — a dropped ping just shortens the other side's indicator by one tick.
  });
}
