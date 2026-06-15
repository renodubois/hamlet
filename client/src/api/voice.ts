import { apiFetch } from "./client";

export interface VoiceParticipant {
  user_id: number;
  channel_id: number;
  username: string;
  avatar_url: string | null;
}

export interface VoiceParticipantLeft {
  channel_id: number;
  user_id: number;
}

export interface VoiceParticipantSpeaking {
  channel_id: number;
  user_id: number;
  speaking: boolean;
}

export type ScreenShareSource = "screen_share";

export interface ScreenShareStream {
  channel_id: number;
  sharer_user_id: number;
  username: string;
  display_name: string | null;
  avatar_url: string | null;
  participant_identity: string;
  track_sid: string;
  track_name: string;
  source: ScreenShareSource;
  started_at: number;
}

export interface ScreenShareStopped {
  channel_id: number;
  sharer_user_id: number;
  participant_identity: string;
  track_sid: string;
}

export interface VoiceToken {
  url: string;
  token: string;
  room: string;
}

export async function getVoiceToken(channelId: number): Promise<VoiceToken> {
  const res = await apiFetch(`/voice/token/${channelId}`, { method: "POST" });
  if (!res.ok) throw new Error(`Voice token fetch failed (${res.status})`);
  return res.json() as Promise<VoiceToken>;
}

export async function listVoiceParticipants(channelId: number): Promise<VoiceParticipant[]> {
  const res = await apiFetch(`/voice/participants/${channelId}`);
  if (!res.ok) throw new Error(`Voice participants fetch failed (${res.status})`);
  return res.json() as Promise<VoiceParticipant[]>;
}

export async function listScreenShareStreams(channelId?: number): Promise<ScreenShareStream[]> {
  const query = channelId == null ? "" : `?channel_id=${encodeURIComponent(String(channelId))}`;
  const res = await apiFetch(`/voice/screen-shares${query}`);
  if (!res.ok) throw new Error(`Screen share streams fetch failed (${res.status})`);
  return res.json() as Promise<ScreenShareStream[]>;
}

export async function postVoiceSpeaking(channelId: number, speaking: boolean): Promise<void> {
  await apiFetch(`/voice/speaking`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ channel_id: channelId, speaking }),
  }).catch(() => {
    // Best-effort — losing a transition just means a momentary stale indicator.
  });
}
