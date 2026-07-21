import { describe, expect, test } from "vitest";
import type { CameraStream } from "../api";
import type { RemoteVideoTrack } from "livekit-client";
import {
  createVoiceSnapshot,
  resetVoiceConnection,
  updateVoiceSnapshot,
  type RemoteCameraTile,
} from "./voice-state";

function camera(overrides: Partial<CameraStream> = {}): CameraStream {
  return {
    channel_id: 10,
    sharer_user_id: 2,
    username: "bob",
    display_name: null,
    avatar_url: null,
    participant_identity: "2",
    track_sid: "camera-1",
    track_name: "camera",
    source: "camera",
    started_at: 100,
    ...overrides,
  };
}

const remoteTrack = {} as RemoteVideoTrack;

describe("immutable voice state", () => {
  test("creates an explicit idle connection and media snapshot", () => {
    const snapshot = createVoiceSnapshot();

    expect(snapshot).toMatchObject({
      activeChannelId: null,
      connectionStatus: "idle",
      screenShareStatus: "off",
      screenSharePublicationVisible: false,
      cameraStatus: "off",
      muted: false,
      deafened: false,
      error: null,
    });
    expect(snapshot.remoteCameraTiles).toEqual([]);
    expect(snapshot.remoteCameraTilesByKey.size).toBe(0);
    expect(snapshot.speakingUserIds.size).toBe(0);
    expect(Object.isFrozen(snapshot)).toBe(true);
  });

  test("applies typed connection, control, and media transitions immutably", () => {
    const initial = createVoiceSnapshot();
    const connecting = updateVoiceSnapshot(initial, {
      connectionStatus: "connecting",
      muted: true,
      screenShareStatus: "starting",
      cameraStatus: "starting",
    });
    const connected = updateVoiceSnapshot(connecting, {
      activeChannelId: 10,
      connectionStatus: "connected",
      screenShareStatus: "on",
      screenSharePublicationVisible: true,
      cameraStatus: "on",
    });

    expect(initial.connectionStatus).toBe("idle");
    expect(connected).toMatchObject({
      activeChannelId: 10,
      connectionStatus: "connected",
      muted: true,
      screenShareStatus: "on",
      screenSharePublicationVisible: true,
      cameraStatus: "on",
    });
  });

  test("retains snapshot identity when no supplied field changes", () => {
    const snapshot = updateVoiceSnapshot(createVoiceSnapshot(), {
      connectionStatus: "connecting",
      muted: true,
    });

    expect(updateVoiceSnapshot(snapshot, { connectionStatus: "connecting", muted: true })).toBe(
      snapshot,
    );
    expect(updateVoiceSnapshot(snapshot, {})).toBe(snapshot);
  });

  test("publishes copied arrays, maps, and sets and keeps semantic no-ops stable", () => {
    const tiles: RemoteCameraTile[] = [{ stream: camera(), track: remoteTrack }];
    const speakers = new Set([2, 3]);
    const snapshot = updateVoiceSnapshot(createVoiceSnapshot(), {
      remoteCameraTiles: tiles,
      speakingUserIds: speakers,
    });

    tiles.push({ stream: camera({ track_sid: "camera-2" }), track: null });
    speakers.add(4);

    expect(snapshot.remoteCameraTiles).toHaveLength(1);
    expect(snapshot.remoteCameraTilesByKey.get("10:2:2:camera-1")?.track).toBe(remoteTrack);
    expect([...snapshot.speakingUserIds]).toEqual([2, 3]);
    expect(Object.isFrozen(snapshot.remoteCameraTiles)).toBe(true);

    const equivalent = updateVoiceSnapshot(snapshot, {
      remoteCameraTiles: [{ stream: camera(), track: remoteTrack }],
      speakingUserIds: new Set([3, 2]),
    });
    expect(equivalent).toBe(snapshot);
  });

  test("replaces collection identities only when their contents change", () => {
    const initial = updateVoiceSnapshot(createVoiceSnapshot(), {
      remoteCameraTiles: [{ stream: camera(), track: remoteTrack }],
      speakingUserIds: [2],
    });
    const changed = updateVoiceSnapshot(initial, {
      remoteCameraTiles: [{ stream: camera({ username: "robert" }), track: remoteTrack }],
      speakingUserIds: [2, 3],
    });

    expect(changed).not.toBe(initial);
    expect(changed.remoteCameraTiles).not.toBe(initial.remoteCameraTiles);
    expect(changed.remoteCameraTilesByKey).not.toBe(initial.remoteCameraTilesByKey);
    expect(changed.speakingUserIds).not.toBe(initial.speakingUserIds);
  });

  test("reset clears connection-owned media but preserves user controls", () => {
    const connected = updateVoiceSnapshot(createVoiceSnapshot(), {
      activeChannelId: 10,
      connectionStatus: "connected",
      muted: true,
      deafened: true,
      cameraStatus: "on",
      screenShareStatus: "on",
      screenSharePublicationVisible: true,
      remoteCameraTiles: [{ stream: camera(), track: remoteTrack }],
      speakingUserIds: [2],
      error: "old error",
    });
    const reset = resetVoiceConnection(connected);

    expect(reset).toMatchObject({
      activeChannelId: null,
      connectionStatus: "idle",
      muted: true,
      deafened: true,
      cameraStatus: "off",
      screenShareStatus: "off",
      screenSharePublicationVisible: false,
      error: "old error",
    });
    expect(reset.remoteCameraTiles).toEqual([]);
    expect(reset.speakingUserIds.size).toBe(0);
    expect(resetVoiceConnection(reset)).toBe(reset);
  });
});
