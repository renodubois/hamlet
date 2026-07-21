import { describe, expect, test } from "vitest";
import type { CameraStream, ScreenShareStream, VoiceParticipant } from "../api";
import {
  beginChannelPresenceBootstrap,
  channelPresenceReducer,
  createChannelPresenceState,
  type ChannelPresenceAction,
  type ChannelPresenceState,
} from "./channel-presence";

function participant(overrides: Partial<VoiceParticipant> = {}): VoiceParticipant {
  return {
    user_id: 1,
    channel_id: 10,
    username: "alice",
    avatar_url: null,
    muted: false,
    deafened: false,
    ...overrides,
  };
}

function screenShare(overrides: Partial<ScreenShareStream> = {}): ScreenShareStream {
  return {
    channel_id: 10,
    sharer_user_id: 1,
    username: "alice",
    display_name: null,
    avatar_url: null,
    participant_identity: "1",
    track_sid: "share-1",
    track_name: "screen",
    source: "screen_share",
    started_at: 100,
    ...overrides,
  };
}

function camera(overrides: Partial<CameraStream> = {}): CameraStream {
  return {
    channel_id: 10,
    sharer_user_id: 1,
    username: "alice",
    display_name: null,
    avatar_url: null,
    participant_identity: "1",
    track_sid: "camera-1",
    track_name: "camera",
    source: "camera",
    started_at: 100,
    ...overrides,
  };
}

function readyState(
  options: {
    participants?: readonly VoiceParticipant[];
    screenShares?: readonly ScreenShareStream[];
    cameraStreams?: readonly CameraStream[];
  } = {},
): ChannelPresenceState {
  let state = beginChannelPresenceBootstrap(createChannelPresenceState(10), 1);
  state = channelPresenceReducer(state, {
    type: "bootstrapSucceeded",
    channelId: 10,
    generation: 1,
    participants: options.participants ?? [participant()],
    screenShares: options.screenShares ?? [],
    cameraStreams: options.cameraStreams ?? [],
  });
  return state;
}

function reduce(state: ChannelPresenceState, ...actions: ChannelPresenceAction[]) {
  return actions.reduce(channelPresenceReducer, state);
}

describe("channel presence bootstrap", () => {
  test("starts idle and retains visible presence during reconnect loading", () => {
    const ready = readyState({ screenShares: [screenShare()] });
    const reconnecting = beginChannelPresenceBootstrap(ready, 2);

    expect(reconnecting).toMatchObject({ generation: 2, status: "loading", error: null });
    expect(reconnecting.participants).toBe(ready.participants);
    expect(reconnecting.screenShares).toBe(ready.screenShares);
    expect(reconnecting.journal).toEqual([]);
  });

  test("journals and replays ordered events over delayed snapshots", () => {
    const initial = beginChannelPresenceBootstrap(createChannelPresenceState(10), 1);
    const staleShare = screenShare();
    const staleCamera = camera();
    const state = reduce(
      initial,
      {
        type: "participantStatusChanged",
        generation: 1,
        status: { channel_id: 10, user_id: 2, muted: true, deafened: false },
      },
      {
        type: "participantLeft",
        generation: 1,
        participant: { channel_id: 10, user_id: 1 },
      },
      {
        type: "participantJoined",
        generation: 1,
        participant: participant({ user_id: 3, username: "carol" }),
      },
      {
        type: "participantSpeakingChanged",
        generation: 1,
        speaking: { channel_id: 10, user_id: 3, speaking: true },
      },
      {
        type: "screenShareStopped",
        generation: 1,
        stopped: staleShare,
      },
      {
        type: "cameraStopped",
        generation: 1,
        stopped: staleCamera,
      },
      {
        type: "screenShareStarted",
        generation: 1,
        stream: screenShare({ sharer_user_id: 3, participant_identity: "3", track_sid: "share-3" }),
      },
    );

    const completed = channelPresenceReducer(state, {
      type: "bootstrapSucceeded",
      channelId: 10,
      generation: 1,
      participants: [participant(), participant({ user_id: 2, username: "bob" })],
      screenShares: [staleShare],
      cameraStreams: [staleCamera],
    });

    expect(completed.status).toBe("ready");
    expect(completed.journal).toEqual([]);
    expect(completed.participants.map((current) => current.user_id)).toEqual([2, 3]);
    expect(completed.participants[0]).toMatchObject({ muted: true, deafened: false });
    expect([...completed.speakingUserIds]).toEqual([3]);
    expect(completed.screenShares.map((stream) => stream.track_sid)).toEqual(["share-3"]);
    expect(completed.cameraStreams).toEqual([]);
  });

  test("a departure before discovery prevents stale owned media from being resurrected", () => {
    let loading = beginChannelPresenceBootstrap(createChannelPresenceState(10), 1);
    loading = channelPresenceReducer(loading, {
      type: "participantLeft",
      generation: 1,
      participant: { channel_id: 10, user_id: 1 },
    });
    loading = channelPresenceReducer(loading, {
      type: "cameraStarted",
      generation: 1,
      stream: camera(),
    });

    const completed = channelPresenceReducer(loading, {
      type: "bootstrapSucceeded",
      channelId: 10,
      generation: 1,
      participants: [participant()],
      screenShares: [screenShare()],
      cameraStreams: [camera()],
    });

    expect(completed.participants).toEqual([]);
    expect(completed.screenShares).toEqual([]);
    expect(completed.cameraStreams).toEqual([]);
  });

  test("preserves event order when a participant leaves and rejoins during bootstrap", () => {
    const loading = beginChannelPresenceBootstrap(createChannelPresenceState(10), 1);
    const updated = reduce(
      loading,
      { type: "participantLeft", generation: 1, participant: { channel_id: 10, user_id: 1 } },
      {
        type: "participantJoined",
        generation: 1,
        participant: participant({ username: "alice again" }),
      },
    );
    const completed = channelPresenceReducer(updated, {
      type: "bootstrapSucceeded",
      channelId: 10,
      generation: 1,
      participants: [participant({ username: "stale alice" })],
      screenShares: [],
      cameraStreams: [],
    });

    expect(completed.participants).toEqual([participant({ username: "alice again" })]);
    expect(completed.departedParticipantIds.has(1)).toBe(false);
  });

  test("a reconnect generation rejects pre-connection completion and becomes ready only from the new snapshot", () => {
    const reconnecting = beginChannelPresenceBootstrap(readyState(), 2);
    const staleCompletion = channelPresenceReducer(reconnecting, {
      type: "bootstrapSucceeded",
      channelId: 10,
      generation: 1,
      participants: [],
      screenShares: [],
      cameraStreams: [],
    });
    expect(staleCompletion).toBe(reconnecting);
    expect(staleCompletion.status).toBe("loading");

    const withEvent = channelPresenceReducer(reconnecting, {
      type: "participantJoined",
      generation: 2,
      participant: participant({ user_id: 2, username: "bob" }),
    });
    const ready = channelPresenceReducer(withEvent, {
      type: "bootstrapSucceeded",
      channelId: 10,
      generation: 2,
      participants: [participant()],
      screenShares: [],
      cameraStreams: [],
    });

    expect(ready.status).toBe("ready");
    expect(ready.participants.map((current) => current.user_id)).toEqual([1, 2]);
  });

  test("a failed reconnect retains proven presence and applies events received while loading", () => {
    const proven = readyState({ screenShares: [screenShare()], cameraStreams: [camera()] });
    const loading = reduce(beginChannelPresenceBootstrap(proven, 2), {
      type: "participantJoined",
      generation: 2,
      participant: participant({ user_id: 2, username: "bob" }),
    });
    const error = new Error("offline");
    const failed = channelPresenceReducer(loading, {
      type: "bootstrapFailed",
      channelId: 10,
      generation: 2,
      error,
    });

    expect(failed).toMatchObject({ status: "error", error });
    expect(failed.participants.map((current) => current.user_id)).toEqual([1, 2]);
    expect(failed.screenShares).toEqual(proven.screenShares);
    expect(failed.cameraStreams).toEqual(proven.cameraStreams);
    expect(failed.journal).toEqual([]);
  });

  test("a partial bootstrap updates successful endpoints and retains failed endpoint data", () => {
    const oldShare = screenShare();
    const oldCamera = camera();
    const loading = beginChannelPresenceBootstrap(
      readyState({ screenShares: [oldShare], cameraStreams: [oldCamera] }),
      2,
    );
    const replacementCamera = camera({ track_sid: "camera-2", started_at: 200 });
    const failed = channelPresenceReducer(loading, {
      type: "bootstrapFailed",
      channelId: 10,
      generation: 2,
      error: new Error("participants unavailable"),
      screenShares: [],
      cameraStreams: [replacementCamera],
    });

    expect(failed.status).toBe("error");
    expect(failed.participants).toEqual(loading.participants);
    expect(failed.screenShares).toEqual([]);
    expect(failed.cameraStreams).toEqual([replacementCamera]);
  });

  test("records current-generation failure and ignores stale failure", () => {
    const loading = beginChannelPresenceBootstrap(createChannelPresenceState(10), 2);
    const stale = channelPresenceReducer(loading, {
      type: "bootstrapFailed",
      channelId: 10,
      generation: 1,
      error: new Error("stale"),
    });
    expect(stale).toBe(loading);

    const error = new Error("offline");
    const failed = channelPresenceReducer(loading, {
      type: "bootstrapFailed",
      channelId: 10,
      generation: 2,
      error,
    });
    expect(failed).toMatchObject({ status: "error", error });
  });
});

describe("channel presence live transitions", () => {
  test("ignores wrong-channel and wrong-generation actions by identity", () => {
    const state = readyState();
    expect(
      channelPresenceReducer(state, {
        type: "participantJoined",
        generation: 0,
        participant: participant({ user_id: 2 }),
      }),
    ).toBe(state);
    expect(
      channelPresenceReducer(state, {
        type: "cameraStarted",
        generation: 1,
        stream: camera({ channel_id: 99 }),
      }),
    ).toBe(state);
    expect(
      channelPresenceReducer(state, {
        type: "bootstrapStarted",
        channelId: 99,
        generation: 2,
      }),
    ).toBe(state);
  });

  test("upserts participants, scopes status, and preserves unaffected participant identity", () => {
    const alice = participant();
    const bob = participant({ user_id: 2, username: "bob" });
    const state = readyState({ participants: [alice, bob] });
    const joined = channelPresenceReducer(state, {
      type: "participantJoined",
      generation: 1,
      participant: participant({ user_id: 2, username: "robert" }),
    });
    const status = channelPresenceReducer(joined, {
      type: "participantStatusChanged",
      generation: 1,
      status: { channel_id: 10, user_id: 2, muted: true, deafened: true },
    });

    expect(status.participants[0]).toBe(state.participants[0]);
    expect(status.participants[1]).toMatchObject({
      username: "robert",
      muted: true,
      deafened: true,
    });
    expect(
      channelPresenceReducer(status, {
        type: "participantStatusChanged",
        generation: 1,
        status: { channel_id: 10, user_id: 2, muted: true, deafened: true },
      }),
    ).toBe(status);
  });

  test("speaking transitions are immutable and leave clears speaking and owned media", () => {
    const populated = readyState({ screenShares: [screenShare()], cameraStreams: [camera()] });
    const speaking = channelPresenceReducer(populated, {
      type: "participantSpeakingChanged",
      generation: 1,
      speaking: { channel_id: 10, user_id: 1, speaking: true },
    });
    expect(
      channelPresenceReducer(speaking, {
        type: "participantSpeakingChanged",
        generation: 1,
        speaking: { channel_id: 10, user_id: 1, speaking: true },
      }),
    ).toBe(speaking);

    const left = channelPresenceReducer(speaking, {
      type: "participantLeft",
      generation: 1,
      participant: { channel_id: 10, user_id: 1 },
    });
    expect(left.participants).toEqual([]);
    expect(left.screenShares).toEqual([]);
    expect(left.cameraStreams).toEqual([]);
    expect(left.speakingUserIds.has(1)).toBe(false);
    expect(left.stoppedScreenShareKeys.has("10:1:1:share-1")).toBe(true);
    expect(left.stoppedCameraKeys.has("10:1:1:camera-1")).toBe(true);
  });

  test("creates announcements only for real camera/share starts and stops", () => {
    const state = readyState();
    const shareStarted = channelPresenceReducer(state, {
      type: "screenShareStarted",
      generation: 1,
      stream: screenShare(),
    });
    expect(shareStarted.screenShareAnnouncement).toEqual({
      kind: "started",
      stream: screenShare(),
    });
    expect(
      channelPresenceReducer(shareStarted, {
        type: "screenShareStarted",
        generation: 1,
        stream: screenShare(),
      }),
    ).toBe(shareStarted);

    const shareStopped = channelPresenceReducer(shareStarted, {
      type: "screenShareStopped",
      generation: 1,
      stopped: screenShare(),
    });
    expect(shareStopped.screenShareAnnouncement).toEqual({
      kind: "stopped",
      stream: screenShare(),
    });

    const cameraStarted = channelPresenceReducer(shareStopped, {
      type: "cameraStarted",
      generation: 1,
      stream: camera(),
    });
    const cameraStopped = channelPresenceReducer(cameraStarted, {
      type: "cameraStopped",
      generation: 1,
      stopped: camera(),
    });
    expect(cameraStarted.cameraAnnouncement).toEqual({ kind: "started", stream: camera() });
    expect(cameraStopped.cameraAnnouncement).toEqual({ kind: "stopped", stream: camera() });
  });

  test("stopped-track tombstones reject stale starts and delayed snapshot resurrection", () => {
    const state = readyState();
    const stopped = reduce(
      state,
      { type: "screenShareStopped", generation: 1, stopped: screenShare() },
      { type: "cameraStopped", generation: 1, stopped: camera() },
    );
    expect(
      channelPresenceReducer(stopped, {
        type: "screenShareStarted",
        generation: 1,
        stream: screenShare(),
      }),
    ).toBe(stopped);
    expect(
      channelPresenceReducer(stopped, {
        type: "cameraStarted",
        generation: 1,
        stream: camera(),
      }),
    ).toBe(stopped);

    let loading = beginChannelPresenceBootstrap(state, 2);
    loading = reduce(
      loading,
      { type: "screenShareStopped", generation: 2, stopped: screenShare() },
      { type: "cameraStopped", generation: 2, stopped: camera() },
    );
    const completed = channelPresenceReducer(loading, {
      type: "bootstrapSucceeded",
      channelId: 10,
      generation: 2,
      participants: [participant()],
      screenShares: [screenShare()],
      cameraStreams: [camera()],
    });
    expect(completed.screenShares).toEqual([]);
    expect(completed.cameraStreams).toEqual([]);
  });

  test("filters malformed cross-channel rows from bootstrap snapshots", () => {
    const loading = beginChannelPresenceBootstrap(createChannelPresenceState(10), 1);
    const completed = channelPresenceReducer(loading, {
      type: "bootstrapSucceeded",
      channelId: 10,
      generation: 1,
      participants: [participant(), participant({ channel_id: 99, user_id: 2 })],
      screenShares: [screenShare(), screenShare({ channel_id: 99, track_sid: "other" })],
      cameraStreams: [camera(), camera({ channel_id: 99, track_sid: "other" })],
    });

    expect(completed.participants).toHaveLength(1);
    expect(completed.screenShares).toHaveLength(1);
    expect(completed.cameraStreams).toHaveLength(1);
  });
});
