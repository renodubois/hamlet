import type { CameraStream, CameraVideoStopped } from "../api";

export function cameraDisplayName(stream: CameraStream): string {
  return stream.display_name?.trim() || stream.username;
}

export function cameraKey(stream: CameraStream | CameraVideoStopped): string {
  return [
    stream.channel_id,
    stream.sharer_user_id,
    stream.participant_identity,
    stream.track_sid,
  ].join(":");
}

export function isSameCameraStream(a: CameraStream, b: CameraStream | CameraVideoStopped): boolean {
  return cameraKey(a) === cameraKey(b);
}

export function sortCameraStreams(streams: CameraStream[]): CameraStream[] {
  return [...streams].sort((a, b) => {
    if (a.started_at !== b.started_at) return a.started_at - b.started_at;
    if (a.channel_id !== b.channel_id) return a.channel_id - b.channel_id;
    if (a.sharer_user_id !== b.sharer_user_id) return a.sharer_user_id - b.sharer_user_id;
    const participant = a.participant_identity.localeCompare(b.participant_identity);
    if (participant !== 0) return participant;
    return a.track_sid.localeCompare(b.track_sid);
  });
}
