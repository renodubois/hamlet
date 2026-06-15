import type { ScreenShareStopped, ScreenShareStream } from "../../api";
import { isSameScreenShare, screenShareKey } from "../../voice/screen-share";
import type { HandlerState } from "./handlers";
import { FakeEventSource } from "./sse";

/**
 * Add or replace an active screen-share stream in the MSW voice fixture and
 * fan out the matching SSE start event to every fake EventSource subscriber.
 */
export function startMswScreenShare(
  state: HandlerState,
  stream: ScreenShareStream,
): ScreenShareStream {
  const streamsByKey = new Map(
    state.screenShareStreams.map((existing) => [screenShareKey(existing), existing]),
  );
  streamsByKey.set(screenShareKey(stream), stream);
  state.screenShareStreams = [...streamsByKey.values()];
  FakeEventSource.instances.forEach((source) => source.pushScreenShareStarted(stream));
  return stream;
}

/**
 * Remove an active screen-share stream from the MSW voice fixture and fan out
 * the matching SSE stop event to every fake EventSource subscriber.
 */
export function stopMswScreenShare(
  state: HandlerState,
  stopped: ScreenShareStopped,
): ScreenShareStopped {
  state.screenShareStreams = state.screenShareStreams.filter(
    (stream) => !isSameScreenShare(stream, stopped),
  );
  FakeEventSource.instances.forEach((source) => source.pushScreenShareStopped(stopped));
  return stopped;
}

export function stoppedScreenShareFrom(stream: ScreenShareStream): ScreenShareStopped {
  return {
    channel_id: stream.channel_id,
    sharer_user_id: stream.sharer_user_id,
    participant_identity: stream.participant_identity,
    track_sid: stream.track_sid,
  };
}
