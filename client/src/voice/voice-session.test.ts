import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("livekit-client", () => {
  class RemoteAudioTrack {
    sid = "audio";
  }
  class RemoteVideoTrack {
    sid = "video";
  }
  return {
    ParticipantEvent: { IsSpeakingChanged: "speaking" },
    RoomEvent: {
      TrackPublished: "trackPublished",
      TrackSubscribed: "trackSubscribed",
      TrackUnpublished: "trackUnpublished",
      TrackUnsubscribed: "trackUnsubscribed",
      LocalTrackUnpublished: "localTrackUnpublished",
      Disconnected: "disconnected",
      ParticipantConnected: "participantConnected",
      ParticipantDisconnected: "participantDisconnected",
    },
    Track: {
      Kind: { Audio: "audio", Video: "video" },
      Source: { Microphone: "microphone", Camera: "camera", ScreenShare: "screen_share" },
    },
    RemoteAudioTrack,
    RemoteVideoTrack,
  };
});

import { RemoteAudioTrack, RemoteVideoTrack, RoomEvent, Track } from "livekit-client";
import type { LocalTrackPublication, Room, RoomOptions } from "livekit-client";
import type { CameraStream, ScreenShareStream } from "../api";
import type { AudioRouter } from "./audio-routing";
import type { InputGainHandle } from "./livekit";
import { DEFAULT_VOICE_PREFERENCES, type VoicePreferencesSnapshot } from "./settings";
import { createVoiceSession, type VoiceSessionDependencies } from "./voice-session";

function required<T>(value: T | undefined): T {
  if (value === undefined) throw new Error("Expected test fixture value");
  return value;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

class FakeParticipant {
  identity: string;
  trackPublications = new Map<string, FakeRemotePublication>();
  private listeners = new Map<string, Set<(...args: unknown[]) => void>>();

  constructor(identity: string) {
    this.identity = identity;
  }

  on(event: string, listener: (...args: never[]) => void) {
    const listeners = this.listeners.get(event) ?? new Set();
    listeners.add(listener as (...args: unknown[]) => void);
    this.listeners.set(event, listeners);
    return this;
  }

  off(event: string, listener: (...args: never[]) => void) {
    this.listeners.get(event)?.delete(listener as (...args: unknown[]) => void);
    return this;
  }

  emit(event: string, ...args: unknown[]) {
    this.listeners.get(event)?.forEach((listener) => listener(...args));
  }

  listenerCount(event: string) {
    return this.listeners.get(event)?.size ?? 0;
  }
}

class FakeRemotePublication {
  kind: string;
  source: string;
  trackSid: string;
  track: RemoteVideoTrack | null;
  setEnabled = vi.fn();
  setSubscribed = vi.fn();

  constructor(kind: string, source: string, sid: string, track: RemoteVideoTrack | null = null) {
    this.kind = kind;
    this.source = source;
    this.trackSid = sid;
    this.track = track;
  }
}

function remoteVideoTrack(): RemoteVideoTrack {
  const MockRemoteVideoTrack = RemoteVideoTrack as unknown as new () => RemoteVideoTrack;
  return new MockRemoteVideoTrack();
}

function mediaTrack() {
  const listeners = new Set<() => void>();
  return {
    readyState: "live",
    stop: vi.fn(),
    addEventListener: vi.fn((_event: string, listener: () => void) => listeners.add(listener)),
    removeEventListener: vi.fn((_event: string, listener: () => void) =>
      listeners.delete(listener),
    ),
    end: () => [...listeners].forEach((listener) => listener()),
  };
}

function localPublication(source: string) {
  const media = mediaTrack();
  const track = {
    source,
    kind: Track.Kind.Video,
    mediaStreamTrack: media,
    stop: vi.fn(),
  };
  return { source, track, videoTrack: track, media };
}

class FakeRoom {
  readonly localParticipant = new FakeParticipant("1") as FakeParticipant & {
    setMicrophoneEnabled: ReturnType<typeof vi.fn>;
    setCameraEnabled: ReturnType<typeof vi.fn>;
    setScreenShareEnabled: ReturnType<typeof vi.fn>;
    getTrackPublication: ReturnType<typeof vi.fn>;
    unpublishTrack: ReturnType<typeof vi.fn>;
  };
  readonly remoteParticipants = new Map<string, FakeParticipant>();
  readonly connect = vi.fn(async (): Promise<void> => undefined);
  readonly disconnect = vi.fn(async (): Promise<void> => undefined);
  private listeners = new Map<string, Set<(...args: unknown[]) => void>>();

  constructor() {
    this.localParticipant.setMicrophoneEnabled = vi.fn(async () => undefined);
    this.localParticipant.setCameraEnabled = vi.fn(async () => undefined);
    this.localParticipant.setScreenShareEnabled = vi.fn(async () => undefined);
    this.localParticipant.getTrackPublication = vi.fn(() => undefined);
    this.localParticipant.unpublishTrack = vi.fn(async () => undefined);
  }

  on(event: string, listener: (...args: never[]) => void) {
    const listeners = this.listeners.get(event) ?? new Set();
    listeners.add(listener as (...args: unknown[]) => void);
    this.listeners.set(event, listeners);
    return this;
  }

  off(event: string, listener: (...args: never[]) => void) {
    this.listeners.get(event)?.delete(listener as (...args: unknown[]) => void);
    return this;
  }

  emit(event: string, ...args: unknown[]) {
    this.listeners.get(event)?.forEach((listener) => listener(...args));
  }

  listenerCount() {
    return [...this.listeners.values()].reduce((count, listeners) => count + listeners.size, 0);
  }
}

const cameraStream = (overrides: Partial<CameraStream> = {}): CameraStream => ({
  channel_id: 10,
  sharer_user_id: 2,
  username: "two",
  display_name: null,
  avatar_url: null,
  participant_identity: "2",
  track_sid: "camera-2",
  track_name: "camera",
  source: "camera",
  started_at: 1,
  ...overrides,
});

const shareStream = (overrides: Partial<ScreenShareStream> = {}): ScreenShareStream => ({
  ...cameraStream(),
  track_sid: "share-2",
  track_name: "screen",
  source: "screen_share",
  ...overrides,
});

function setup(
  options: {
    token?: VoiceSessionDependencies["getToken"];
    gain?: VoiceSessionDependencies["applyInputGain"];
    preferences?: VoicePreferencesSnapshot;
  } = {},
) {
  const rooms: FakeRoom[] = [];
  const audioAttach = vi.fn();
  const audioDetachAll = vi.fn();
  const audioSetOutputDevice = vi.fn();
  const audio: AudioRouter = {
    attach: audioAttach,
    detach: vi.fn(),
    detachAll: audioDetachAll,
    setDeafened: vi.fn(),
    setOutputDevice: audioSetOutputDevice,
  };
  const postStatus = vi.fn(async () => undefined);
  const postSpeaking = vi.fn(async () => undefined);
  const dependencies: VoiceSessionDependencies = {
    getToken:
      options.token ??
      vi.fn(async (channelId) => ({
        url: "ws://voice",
        token: `token-${channelId}`,
        room: String(channelId),
      })),
    postStatus,
    postSpeaking,
    createRoom: vi.fn((_options: RoomOptions) => {
      const room = new FakeRoom();
      rooms.push(room);
      return room as unknown as Room;
    }),
    createAudioRouter: vi.fn(() => audio),
    applyInputGain: options.gain ?? vi.fn(async () => null),
    getPreferences: () => options.preferences ?? DEFAULT_VOICE_PREFERENCES,
  };
  const session = createVoiceSession(dependencies);
  return {
    session,
    dependencies,
    rooms,
    audio,
    audioAttach,
    audioDetachAll,
    audioSetOutputDevice,
    postStatus,
    postSpeaking,
  };
}

async function joined(value = setup(), channelId = 10) {
  value.session.activate();
  await value.session.join(channelId);
  expect(value.session.getSnapshot().activeChannelId).toBe(channelId);
  return value;
}

beforeEach(() => vi.clearAllMocks());

describe("VoiceSession external store", () => {
  it("is render-pure and publishes only changed immutable snapshots", () => {
    const value = setup();
    expect(value.rooms).toHaveLength(0);
    const initial = value.session.getSnapshot();
    const listener = vi.fn();
    const unsubscribe = value.session.subscribe(listener);
    value.session.activate();
    value.session.activate();
    value.session.applyPreferences(DEFAULT_VOICE_PREFERENCES);
    expect(value.session.getSnapshot()).toBe(initial);
    expect(listener).not.toHaveBeenCalled();
    unsubscribe();
    unsubscribe();
  });

  it("joins with autoSubscribe false, capture preferences, microphone, and gain", async () => {
    const preferences = {
      ...DEFAULT_VOICE_PREFERENCES,
      inputDeviceId: "mic",
      noiseSuppression: false,
      inputGain: 1.5,
    };
    const gain = vi.fn(async () => null);
    const value = setup({ preferences, gain });
    value.session.activate();
    await value.session.join(10);
    const room = required(value.rooms[0]);
    expect(vi.mocked(value.dependencies.createRoom)).toHaveBeenCalledWith(
      expect.objectContaining({
        audioCaptureDefaults: expect.objectContaining({ deviceId: "mic", noiseSuppression: false }),
      }),
    );
    expect(room.connect).toHaveBeenCalledWith("ws://voice", "token-10", { autoSubscribe: false });
    expect(room.localParticipant.setMicrophoneEnabled).toHaveBeenCalledWith(true);
    expect(gain).toHaveBeenCalledWith(room, {
      gain: 1.5,
      inputDeviceId: "mic",
      noiseSuppression: false,
    });
    expect(value.session.getSnapshot().connectionStatus).toBe("connected");
  });

  it.each(["token", "connect", "microphone"] as const)(
    "cleans an owned room after %s failure",
    async (failure) => {
      const value = setup({
        token:
          failure === "token"
            ? vi.fn(async () => {
                throw new Error("token failed");
              })
            : undefined,
      });
      value.session.activate();
      if (failure !== "token") {
        const create = value.dependencies.createRoom as ReturnType<typeof vi.fn>;
        create.mockImplementation(() => {
          const room = new FakeRoom();
          if (failure === "connect")
            room.connect.mockRejectedValueOnce(new Error("connect failed"));
          else
            room.localParticipant.setMicrophoneEnabled.mockRejectedValueOnce(
              new Error("mic failed"),
            );
          value.rooms.push(room);
          return room as unknown as Room;
        });
      }
      await value.session.join(10);
      expect(value.session.getSnapshot().connectionStatus).toBe("idle");
      expect(value.session.getSnapshot().error).toContain("failed");
      if (value.rooms[0]) expect(value.rooms[0].disconnect).toHaveBeenCalled();
    },
  );

  it("reapplies a muted join after input gain replaces the microphone", async () => {
    const dispose = vi.fn(async () => undefined);
    const value = setup({
      preferences: { ...DEFAULT_VOICE_PREFERENCES, inputGain: 1.5 },
      gain: vi.fn(async () => ({ dispose }) as unknown as InputGainHandle),
    });
    value.session.activate();
    await value.session.toggleMuted();
    await value.session.join(10);

    expect(required(value.rooms[0]).localParticipant.setMicrophoneEnabled.mock.calls).toEqual([
      [false],
      [false],
    ]);
    expect(value.session.getSnapshot()).toMatchObject({
      muted: true,
      connectionStatus: "connected",
    });
    expect(dispose).not.toHaveBeenCalled();
  });

  it("applies a mute changed while input gain is replacing the microphone", async () => {
    const pendingGain = deferred<InputGainHandle | null>();
    const gain = vi.fn(() => pendingGain.promise);
    const value = setup({
      preferences: { ...DEFAULT_VOICE_PREFERENCES, inputGain: 1.5 },
      gain,
    });
    value.session.activate();

    const joining = value.session.join(10);
    await vi.waitFor(() => expect(gain).toHaveBeenCalledOnce());
    await value.session.toggleMuted();
    pendingGain.resolve({ dispose: vi.fn(async () => undefined) } as unknown as InputGainHandle);
    await joining;

    const microphoneCalls = required(value.rooms[0]).localParticipant.setMicrophoneEnabled.mock
      .calls;
    expect(microphoneCalls).toEqual([[true], [false], [false]]);
    expect(value.session.getSnapshot()).toMatchObject({
      muted: true,
      connectionStatus: "connected",
    });
  });

  it("keeps the default microphone when input gain fails", async () => {
    const value = await joined(
      setup({
        gain: vi.fn(async () => {
          throw new Error("gain");
        }),
      }),
    );
    expect(value.session.getSnapshot().connectionStatus).toBe("connected");
    expect(value.session.getSnapshot().error).toBeNull();
  });

  it("reports camera/share capture failures without disturbing the other medium", async () => {
    const value = await joined();
    const room = required(value.rooms[0]);
    room.localParticipant.setCameraEnabled.mockRejectedValueOnce(
      Object.assign(new Error("denied"), { name: "NotAllowedError" }),
    );
    await expect(value.session.startCamera()).rejects.toThrow("Camera permission was denied");
    expect(value.session.getSnapshot()).toMatchObject({
      cameraStatus: "off",
      screenShareStatus: "off",
    });

    room.localParticipant.setScreenShareEnabled.mockRejectedValueOnce(
      Object.assign(new Error("canceled"), { name: "AbortError" }),
    );
    await expect(value.session.startScreenShare()).rejects.toThrow("canceled or denied");
    expect(value.session.getSnapshot()).toMatchObject({
      cameraStatus: "off",
      screenShareStatus: "off",
    });
  });

  it("switches channels and prevents an older token from becoming current", async () => {
    const first = deferred<{ url: string; token: string; room: string }>();
    const getToken = vi.fn((channelId: number) =>
      channelId === 1 ? first.promise : Promise.resolve({ url: "ws://b", token: "b", room: "2" }),
    );
    const value = setup({ token: getToken });
    value.session.activate();
    const joiningA = value.session.join(1);
    await Promise.resolve();
    await value.session.join(2);
    first.resolve({ url: "ws://a", token: "a", room: "1" });
    await joiningA;
    expect(value.session.getSnapshot().activeChannelId).toBe(2);
    expect(value.rooms).toHaveLength(1);
  });

  it("a stale connecting room cannot remove the replacement room's listeners", async () => {
    const value = setup();
    value.session.activate();
    const pending = deferred<void>();
    const create = value.dependencies.createRoom as ReturnType<typeof vi.fn>;
    create.mockImplementation(() => {
      const room = new FakeRoom();
      if (value.rooms.length === 0) room.connect.mockImplementationOnce(() => pending.promise);
      value.rooms.push(room);
      return room as unknown as Room;
    });
    const joiningA = value.session.join(1);
    await vi.waitFor(() => expect(value.rooms).toHaveLength(1));
    await value.session.join(2);
    const replacement = required(value.rooms[1]);
    pending.resolve();
    await joiningA;
    expect(value.session.getSnapshot().activeChannelId).toBe(2);
    expect(replacement.listenerCount()).toBeGreaterThan(0);
  });

  it("disconnects a joining room when leave or deactivate wins connect", async () => {
    for (const deactivate of [false, true]) {
      const value = setup();
      value.session.activate();
      const pending = deferred<void>();
      const create = value.dependencies.createRoom as ReturnType<typeof vi.fn>;
      create.mockImplementation(() => {
        const room = new FakeRoom();
        room.connect.mockImplementationOnce(() => pending.promise);
        value.rooms.push(room);
        return room as unknown as Room;
      });
      const joining = value.session.join(10);
      await vi.waitFor(() => expect(value.rooms).toHaveLength(1));
      const leaving = deactivate ? value.session.deactivate() : value.session.leave();
      pending.resolve();
      await Promise.all([joining, leaving]);
      expect(required(value.rooms[0]).disconnect).toHaveBeenCalled();
      expect(value.session.getSnapshot().activeChannelId).toBeNull();
    }
  });

  it("disposes a stale gain handle produced after deactivation", async () => {
    const pending = deferred<InputGainHandle | null>();
    const value = setup({ gain: () => pending.promise });
    value.session.activate();
    const joining = value.session.join(10);
    await vi.waitFor(() =>
      expect(value.rooms[0]?.localParticipant.setMicrophoneEnabled).toHaveBeenCalled(),
    );
    const deactivating = value.session.deactivate();
    const dispose = vi.fn(async () => undefined);
    pending.resolve({ dispose } as unknown as InputGainHandle);
    await Promise.all([joining, deactivating]);
    expect(dispose).toHaveBeenCalledOnce();
  });

  it("disposes a gain handle when deactivated during microphone correction", async () => {
    const correction = deferred<void>();
    const dispose = vi.fn(async () => undefined);
    const value = setup({
      gain: vi.fn(async () => ({ dispose }) as unknown as InputGainHandle),
    });
    const create = value.dependencies.createRoom as ReturnType<typeof vi.fn>;
    create.mockImplementation(() => {
      const room = new FakeRoom();
      room.localParticipant.setMicrophoneEnabled
        .mockResolvedValue(undefined)
        .mockImplementationOnce(async () => undefined)
        .mockImplementationOnce(() => correction.promise);
      value.rooms.push(room);
      return room as unknown as Room;
    });
    value.session.activate();

    const joining = value.session.join(10);
    await vi.waitFor(() =>
      expect(value.rooms[0]?.localParticipant.setMicrophoneEnabled).toHaveBeenCalledTimes(2),
    );
    const deactivating = value.session.deactivate();
    correction.resolve();
    await Promise.all([joining, deactivating]);

    expect(dispose).toHaveBeenCalledOnce();
    expect(value.session.getSnapshot().connectionStatus).toBe("idle");
  });

  it("serializes mute/deafen and restores the mute state from before deafen", async () => {
    const value = await joined();
    const room = required(value.rooms[0]);
    const gate = deferred<void>();
    room.localParticipant.setMicrophoneEnabled.mockImplementationOnce(() => gate.promise);
    const deafen = value.session.toggleDeafened();
    const undeafen = value.session.toggleDeafened();
    await vi.waitFor(() =>
      expect(room.localParticipant.setMicrophoneEnabled).toHaveBeenCalledTimes(2),
    ); // join + first control
    gate.resolve();
    await Promise.all([deafen, undeafen]);
    expect(value.session.getSnapshot()).toMatchObject({ muted: false, deafened: false });
    expect(room.localParticipant.setMicrophoneEnabled.mock.calls.slice(-2)).toEqual([
      [false],
      [true],
    ]);
    expect(value.postStatus.mock.calls.slice(-2)).toEqual([
      [true, true],
      [false, false],
    ]);
  });

  it("unmuting while deafened undeafens and microphone failure rolls controls back", async () => {
    const value = await joined();
    await value.session.toggleDeafened();
    await value.session.toggleMuted();
    expect(value.session.getSnapshot()).toMatchObject({ muted: false, deafened: false });
    required(value.rooms[0]).localParticipant.setMicrophoneEnabled.mockRejectedValueOnce(
      new Error("media"),
    );
    await expect(value.session.toggleMuted()).rejects.toThrow("media");
    expect(value.session.getSnapshot()).toMatchObject({ muted: false, deafened: false });
  });

  it("posts speaking transitions once with the channel captured at join", async () => {
    const value = await joined();
    required(value.rooms[0]).localParticipant.emit("speaking", true);
    required(value.rooms[0]).localParticipant.emit("speaking", true);
    required(value.rooms[0]).localParticipant.emit("speaking", false);
    expect(value.postSpeaking.mock.calls).toEqual([
      [10, true],
      [10, false],
    ]);
  });

  it("posts captured-channel speaking false once on disconnect races", async () => {
    const value = await joined();
    const room = required(value.rooms[0]);
    room.localParticipant.emit("speaking", true);
    room.emit(RoomEvent.Disconnected);
    room.emit(RoomEvent.Disconnected);
    await value.session.leave();
    expect(value.postSpeaking.mock.calls).toEqual([
      [10, true],
      [10, false],
    ]);
  });

  it("subscribes every remote microphone despite autoSubscribe false", async () => {
    const value = await joined();
    const participant = new FakeParticipant("2");
    const microphone = new FakeRemotePublication(
      Track.Kind.Audio,
      Track.Source.Microphone,
      "mic-2",
    );
    required(value.rooms[0]).emit(RoomEvent.TrackPublished, microphone, participant);
    expect(microphone.setSubscribed).toHaveBeenCalledWith(true);
  });

  it("converges remote camera discovery before or after publication", async () => {
    for (const discoveryFirst of [true, false]) {
      const value = await joined();
      const room = required(value.rooms[0]);
      const participant = new FakeParticipant("2");
      const track = remoteVideoTrack();
      const publication = new FakeRemotePublication(
        Track.Kind.Video,
        Track.Source.Camera,
        "camera-2",
        track,
      );
      participant.trackPublications.set("camera-2", publication);
      room.remoteParticipants.set("2", participant);
      if (discoveryFirst) value.session.syncRemoteCameraStreams(10, [cameraStream()]);
      room.emit(RoomEvent.TrackPublished, publication, participant);
      room.emit(RoomEvent.TrackSubscribed, track, publication, participant);
      if (!discoveryFirst) value.session.syncRemoteCameraStreams(10, [cameraStream()]);
      expect(value.session.getSnapshot().remoteCameraTiles[0]).toMatchObject({ track });
      expect(publication.setSubscribed).toHaveBeenLastCalledWith(true);
    }
  });

  it("enables and subscribes exactly the watched screen publication", async () => {
    const value = await joined();
    const room = required(value.rooms[0]);
    const participant = new FakeParticipant("2");
    const selected = new FakeRemotePublication(
      Track.Kind.Video,
      Track.Source.ScreenShare,
      "share-2",
      remoteVideoTrack(),
    );
    const other = new FakeRemotePublication(
      Track.Kind.Video,
      Track.Source.ScreenShare,
      "other",
      remoteVideoTrack(),
    );
    participant.trackPublications.set("share-2", selected);
    participant.trackPublications.set("other", other);
    room.remoteParticipants.set("2", participant);
    await value.session.watchScreenShare(shareStream());
    expect(selected.setEnabled).toHaveBeenLastCalledWith(true);
    expect(selected.setSubscribed).toHaveBeenLastCalledWith(true);
    expect(other.setEnabled).toHaveBeenLastCalledWith(false);
    expect(other.setSubscribed).toHaveBeenLastCalledWith(false);
    await value.session.stopWatchingScreenShare();
    expect(selected.setSubscribed).toHaveBeenLastCalledWith(false);
  });

  it("runs camera and share independently and track-ended stops only its owner", async () => {
    const value = await joined();
    const room = required(value.rooms[0]);
    const camera = localPublication(Track.Source.Camera);
    const share = localPublication(Track.Source.ScreenShare);
    room.localParticipant.setCameraEnabled.mockResolvedValue(
      camera as unknown as LocalTrackPublication,
    );
    room.localParticipant.setScreenShareEnabled
      .mockResolvedValueOnce(share as unknown as LocalTrackPublication)
      .mockResolvedValue(undefined);
    await value.session.startCamera();
    await value.session.startScreenShare();
    expect(value.session.getSnapshot()).toMatchObject({
      cameraStatus: "on",
      screenShareStatus: "on",
    });
    camera.media.end();
    await vi.waitFor(() => expect(value.session.getSnapshot().cameraStatus).toBe("off"));
    expect(value.session.getSnapshot().screenShareStatus).toBe("on");
    share.media.end();
    await vi.waitFor(() => expect(value.session.getSnapshot().screenShareStatus).toBe("off"));
  });

  it("coalesces duplicate camera and screen-share starts while capture is pending", async () => {
    const value = await joined();
    const room = required(value.rooms[0]);
    const cameraPending = deferred<LocalTrackPublication>();
    const sharePending = deferred<LocalTrackPublication>();
    room.localParticipant.setCameraEnabled.mockImplementationOnce(() => cameraPending.promise);
    room.localParticipant.setScreenShareEnabled.mockImplementationOnce(() => sharePending.promise);

    const cameraA = value.session.startCamera();
    const cameraB = value.session.startCamera();
    const shareA = value.session.startScreenShare();
    const shareB = value.session.startScreenShare();
    expect(cameraB).toBe(cameraA);
    expect(shareB).toBe(shareA);
    expect(room.localParticipant.setCameraEnabled).toHaveBeenCalledTimes(1);
    expect(room.localParticipant.setScreenShareEnabled).toHaveBeenCalledTimes(1);

    cameraPending.resolve(
      localPublication(Track.Source.Camera) as unknown as LocalTrackPublication,
    );
    sharePending.resolve(
      localPublication(Track.Source.ScreenShare) as unknown as LocalTrackPublication,
    );
    await Promise.all([cameraA, cameraB, shareA, shareB]);
    expect(value.session.getSnapshot()).toMatchObject({
      cameraStatus: "on",
      screenShareStatus: "on",
    });
  });

  it("keeps a share publication visible while stopping and clears it on completion", async () => {
    const value = await joined();
    const room = required(value.rooms[0]);
    const share = localPublication(Track.Source.ScreenShare);
    const stopped = deferred<void>();
    room.localParticipant.setScreenShareEnabled
      .mockResolvedValueOnce(share as unknown as LocalTrackPublication)
      .mockImplementationOnce(() => stopped.promise);
    await value.session.startScreenShare();

    const stopping = value.session.stopScreenShare();
    expect(value.session.getSnapshot()).toMatchObject({
      screenShareStatus: "stopping",
      screenSharePublicationVisible: true,
    });
    stopped.resolve();
    await stopping;
    expect(value.session.getSnapshot()).toMatchObject({
      screenShareStatus: "off",
      screenSharePublicationVisible: false,
    });
  });

  it("keeps stopping visible and serializes stop followed by restart", async () => {
    const value = await joined();
    const room = required(value.rooms[0]);
    const first = localPublication(Track.Source.Camera);
    const second = localPublication(Track.Source.Camera);
    room.localParticipant.setCameraEnabled
      .mockResolvedValueOnce(first as unknown as LocalTrackPublication)
      .mockResolvedValueOnce(second as unknown as LocalTrackPublication);
    await value.session.startCamera();
    const stopped = deferred<void>();
    room.localParticipant.unpublishTrack.mockImplementationOnce(() => stopped.promise);

    const stopping = value.session.stopCamera();
    const restarting = value.session.startCamera();
    expect(value.session.getSnapshot().cameraStatus).toBe("stopping");
    expect(room.localParticipant.setCameraEnabled).toHaveBeenCalledTimes(1);
    stopped.resolve();
    await Promise.all([stopping, restarting]);

    expect(room.localParticipant.setCameraEnabled).toHaveBeenCalledTimes(2);
    expect(value.session.getSnapshot().cameraStatus).toBe("on");
  });

  it("switching channels clears active media, watching, remotes, and speaking", async () => {
    const value = await joined();
    const firstRoom = required(value.rooms[0]);
    const camera = localPublication(Track.Source.Camera);
    const share = localPublication(Track.Source.ScreenShare);
    firstRoom.localParticipant.setCameraEnabled.mockResolvedValueOnce(
      camera as unknown as LocalTrackPublication,
    );
    firstRoom.localParticipant.setScreenShareEnabled.mockResolvedValueOnce(
      share as unknown as LocalTrackPublication,
    );
    await value.session.startCamera();
    await value.session.startScreenShare();
    await value.session.watchScreenShare(shareStream());
    value.session.syncRemoteCameraStreams(10, [cameraStream()]);
    firstRoom.localParticipant.emit("speaking", true);
    await value.session.toggleDeafened();

    await value.session.join(20);

    expect(firstRoom.localParticipant.unpublishTrack).toHaveBeenCalledWith(camera.track, true);
    expect(firstRoom.localParticipant.setScreenShareEnabled).toHaveBeenCalledWith(false);
    expect(value.postSpeaking.mock.calls).toContainEqual([10, false]);
    expect(value.session.getSnapshot()).toMatchObject({
      activeChannelId: 20,
      muted: true,
      deafened: true,
      cameraStatus: "off",
      screenShareStatus: "off",
      watchedScreenShare: null,
      remoteCameraTiles: [],
    });
    expect(value.session.getSnapshot().speakingUserIds.size).toBe(0);
  });

  it("deactivation disposes camera/share publications created by stale capture completions", async () => {
    const value = await joined();
    const room = required(value.rooms[0]);
    const cameraPending = deferred<LocalTrackPublication>();
    const sharePending = deferred<LocalTrackPublication>();
    room.localParticipant.setCameraEnabled.mockImplementationOnce(() => cameraPending.promise);
    room.localParticipant.setScreenShareEnabled.mockImplementationOnce(() => sharePending.promise);
    const camera = value.session.startCamera();
    const share = value.session.startScreenShare();
    const deactivation = value.session.deactivate();
    cameraPending.resolve(
      localPublication(Track.Source.Camera) as unknown as LocalTrackPublication,
    );
    sharePending.resolve(
      localPublication(Track.Source.ScreenShare) as unknown as LocalTrackPublication,
    );
    await Promise.all([camera, share, deactivation]);
    expect(room.localParticipant.unpublishTrack).toHaveBeenCalled();
    expect(room.localParticipant.setScreenShareEnabled).toHaveBeenLastCalledWith(false);
    expect(value.session.getSnapshot()).toMatchObject({
      activeChannelId: null,
      cameraStatus: "off",
      screenShareStatus: "off",
    });
  });

  it("cleans stale camera/share work and keeps their operations independent", async () => {
    const value = await joined();
    const room = required(value.rooms[0]);
    const cameraPending = deferred<LocalTrackPublication>();
    const sharePending = deferred<LocalTrackPublication>();
    room.localParticipant.setCameraEnabled.mockImplementationOnce(() => cameraPending.promise);
    room.localParticipant.setScreenShareEnabled.mockImplementationOnce(() => sharePending.promise);
    const startingCamera = value.session.startCamera();
    const startingShare = value.session.startScreenShare();
    const stoppingCamera = value.session.stopCamera();
    cameraPending.resolve(
      localPublication(Track.Source.Camera) as unknown as LocalTrackPublication,
    );
    sharePending.resolve(
      localPublication(Track.Source.ScreenShare) as unknown as LocalTrackPublication,
    );
    await Promise.all([startingCamera, startingShare, stoppingCamera]);
    expect(value.session.getSnapshot()).toMatchObject({
      cameraStatus: "off",
      screenShareStatus: "on",
    });
    expect(room.localParticipant.unpublishTrack).toHaveBeenCalled();
  });

  it("does not let stale room cleanup detach replacement-room audio", async () => {
    const disposal = deferred<void>();
    const value = await joined(
      setup({
        gain: vi.fn(
          async () => ({ dispose: () => disposal.promise }) as unknown as InputGainHandle,
        ),
      }),
    );
    const oldRoom = required(value.rooms[0]);
    oldRoom.emit(RoomEvent.Disconnected);
    await value.session.join(20);
    const replacement = required(value.rooms[1]);
    const audioTrack = new (RemoteAudioTrack as unknown as new () => RemoteAudioTrack)();
    replacement.emit(RoomEvent.TrackSubscribed, audioTrack, {}, new FakeParticipant("2"));
    expect(value.audioAttach).toHaveBeenCalledWith(audioTrack);
    const detachCount = value.audioDetachAll.mock.calls.length;

    disposal.resolve();
    await vi.waitFor(() => expect(oldRoom.disconnect).toHaveBeenCalled());
    expect(value.audioDetachAll).toHaveBeenCalledTimes(detachCount);
  });

  it("server disconnect, leave, switch, and Strict activation dispose listeners/audio/gain", async () => {
    const dispose = vi.fn(async () => undefined);
    const value = await joined(
      setup({ gain: vi.fn(async () => ({ dispose }) as unknown as InputGainHandle) }),
    );
    const first = required(value.rooms[0]);
    expect(first.listenerCount()).toBeGreaterThan(0);
    first.emit(RoomEvent.Disconnected);
    await vi.waitFor(() => expect(first.disconnect).toHaveBeenCalled());
    expect(first.listenerCount()).toBe(0);
    expect(dispose).toHaveBeenCalledOnce();
    expect(value.audioDetachAll).toHaveBeenCalled();
    expect(value.session.getSnapshot().activeChannelId).toBeNull();

    await value.session.deactivate();
    await value.session.deactivate();
    value.session.activate();
    value.session.activate();
    await value.session.join(20);
    await value.session.leave();
    expect(required(value.rooms[1]).listenerCount()).toBe(0);
    expect(required(value.rooms[1]).disconnect).toHaveBeenCalledOnce();
  });

  it("reroutes output immediately while capture preferences apply to the next operation", async () => {
    const value = await joined();
    value.session.applyPreferences({
      ...DEFAULT_VOICE_PREFERENCES,
      outputDeviceId: "speakers",
      cameraDeviceId: "cam-2",
    });
    expect(value.audioSetOutputDevice).toHaveBeenLastCalledWith("speakers");
    const publication = localPublication(Track.Source.Camera);
    required(value.rooms[0]).localParticipant.setCameraEnabled.mockResolvedValue(
      publication as unknown as LocalTrackPublication,
    );
    await value.session.startCamera();
    expect(required(value.rooms[0]).localParticipant.setCameraEnabled).toHaveBeenCalledWith(true, {
      deviceId: { exact: "cam-2" },
    });
  });
});
