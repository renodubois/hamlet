import { LocalAudioTrack, Room, Track } from "livekit-client";
import { VOICE_INPUT_STORAGE_KEY, getNoiseSuppressionEnabled } from "./settings";

/**
 * Replace the default mic publication with a track that passes through a
 * GainNode so user-configured input volume takes effect. Skipped when the
 * gain is exactly 1.0 — the native capture path has lower latency and
 * fewer moving parts.
 */
export async function applyInputGain(room: Room, gain: number): Promise<void> {
  if (Math.abs(gain - 1) < 0.01) return;
  const audioCtx = new AudioContext();
  const inputDeviceId = localStorage.getItem(VOICE_INPUT_STORAGE_KEY) ?? "";
  const stream = await navigator.mediaDevices.getUserMedia({
    audio: {
      deviceId: inputDeviceId ? { exact: inputDeviceId } : undefined,
      noiseSuppression: getNoiseSuppressionEnabled(),
      echoCancellation: true,
      autoGainControl: true,
    },
  });
  const source = audioCtx.createMediaStreamSource(stream);
  const gainNode = audioCtx.createGain();
  gainNode.gain.value = gain;
  const dest = audioCtx.createMediaStreamDestination();
  source.connect(gainNode).connect(dest);
  const processed = dest.stream.getAudioTracks()[0];
  if (!processed) return;
  const localTrack = new LocalAudioTrack(processed, undefined, false);
  // Unpublish the default mic track, then publish the gain-adjusted one.
  const existing = room.localParticipant.getTrackPublication(Track.Source.Microphone);
  if (existing?.audioTrack) {
    await room.localParticipant.unpublishTrack(existing.audioTrack);
  }
  await room.localParticipant.publishTrack(localTrack, { source: Track.Source.Microphone });
}
