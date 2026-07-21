import { apiFetch } from "./client";

export interface VoiceParticipant {
  user_id: number;
  channel_id: number;
  username: string;
  avatar_url: string | null;
  muted: boolean;
  deafened: boolean;
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

export interface VoiceParticipantStatus {
  channel_id: number;
  user_id: number;
  muted: boolean;
  deafened: boolean;
}

export type ScreenShareSource = "screen_share";
export type CameraSource = "camera";

interface ActiveMediaStream<Source extends string> {
  channel_id: number;
  sharer_user_id: number;
  username: string;
  display_name: string | null;
  avatar_url: string | null;
  participant_identity: string;
  track_sid: string;
  track_name: string;
  source: Source;
  started_at: number;
}

export type ScreenShareStream = ActiveMediaStream<ScreenShareSource>;
export type CameraStream = ActiveMediaStream<CameraSource>;

export interface ScreenShareStopped {
  channel_id: number;
  sharer_user_id: number;
  participant_identity: string;
  track_sid: string;
}

export type CameraVideoStopped = ScreenShareStopped;

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

export async function listVoiceParticipants(
  channelId: number,
  signal?: AbortSignal,
): Promise<VoiceParticipant[]> {
  const res = await apiFetch(`/voice/participants/${channelId}`, { signal });
  if (!res.ok) throw new Error(`Voice participants fetch failed (${res.status})`);
  return res.json() as Promise<VoiceParticipant[]>;
}

function channelFilterQuery(channelId?: number): string {
  return channelId == null ? "" : `?channel_id=${encodeURIComponent(String(channelId))}`;
}

export async function listScreenShareStreams(
  channelId?: number,
  signal?: AbortSignal,
): Promise<ScreenShareStream[]> {
  const res = await apiFetch(`/voice/screen-shares${channelFilterQuery(channelId)}`, { signal });
  if (!res.ok) throw new Error(`Screen share streams fetch failed (${res.status})`);
  return res.json() as Promise<ScreenShareStream[]>;
}

export async function listCameraStreams(
  channelId?: number,
  signal?: AbortSignal,
): Promise<CameraStream[]> {
  const res = await apiFetch(`/voice/cameras${channelFilterQuery(channelId)}`, { signal });
  if (!res.ok) throw new Error(`Camera streams fetch failed (${res.status})`);
  return res.json() as Promise<CameraStream[]>;
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

export async function postVoiceStatus(muted: boolean, deafened: boolean): Promise<void> {
  await apiFetch(`/voice/status`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ muted, deafened }),
  }).catch(() => {
    // Best-effort — the local controls still work if a transient update is lost.
  });
}
