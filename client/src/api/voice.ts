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

export async function postVoiceSpeaking(channelId: number, speaking: boolean): Promise<void> {
  await apiFetch(`/voice/speaking`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ channel_id: channelId, speaking }),
  }).catch(() => {
    // Best-effort — losing a transition just means a momentary stale indicator.
  });
}
