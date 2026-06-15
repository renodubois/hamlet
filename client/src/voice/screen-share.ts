import type { ScreenShareStopped, ScreenShareStream } from "../api";

export function screenShareDisplayName(stream: ScreenShareStream): string {
  return stream.display_name?.trim() || stream.username;
}

export function screenShareKey(stream: ScreenShareStopped | ScreenShareStream): string {
  return [
    stream.channel_id,
    stream.sharer_user_id,
    stream.participant_identity,
    stream.track_sid,
  ].join(":");
}

export function isSameScreenShare(
  a: ScreenShareStream,
  b: ScreenShareStopped | ScreenShareStream,
): boolean {
  return screenShareKey(a) === screenShareKey(b);
}
