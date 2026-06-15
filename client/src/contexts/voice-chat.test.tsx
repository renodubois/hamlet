import { describe, expect, beforeEach, test, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { Show } from "solid-js";

const apiMock = vi.hoisted(() => ({
  getVoiceToken: vi.fn(),
  postVoiceSpeaking: vi.fn(),
}));

const audioMock = vi.hoisted(() => ({
  router: {
    attach: vi.fn(),
    detach: vi.fn(),
    detachAll: vi.fn(),
    setDeafened: vi.fn(),
  },
}));

const livekitMock = vi.hoisted(() => {
  const RoomEvent = {
    TrackPublished: "trackPublished",
    TrackSubscribed: "trackSubscribed",
    TrackUnpublished: "trackUnpublished",
    TrackUnsubscribed: "trackUnsubscribed",
    LocalTrackUnpublished: "localTrackUnpublished",
    Disconnected: "disconnected",
    ParticipantConnected: "participantConnected",
    ParticipantDisconnected: "participantDisconnected",
  } as const;
  const ParticipantEvent = {
    IsSpeakingChanged: "isSpeakingChanged",
  } as const;
  const Track = {
    Kind: {
      Audio: "audio",
      Video: "video",
    },
    Source: {
      Microphone: "microphone",
      ScreenShare: "screen_share",
      ScreenShareAudio: "screen_share_audio",
    },
  } as const;

  type Listener = (...args: unknown[]) => void;

  class FakeEmitter {
    private listeners = new Map<string, Set<Listener>>();

    on(event: string, listener: Listener) {
      const listeners = this.listeners.get(event) ?? new Set<Listener>();
      listeners.add(listener);
      this.listeners.set(event, listeners);
      return this;
    }

    emit(event: string, ...args: unknown[]) {
      this.listeners.get(event)?.forEach((listener) => listener(...args));
    }
  }

  class FakeMediaStreamTrack {
    private endedListeners = new Set<() => void>();

    addEventListener = vi.fn((type: string, listener: () => void) => {
      if (type === "ended") this.endedListeners.add(listener);
    });

    removeEventListener = vi.fn((type: string, listener: () => void) => {
      if (type === "ended") this.endedListeners.delete(listener);
    });

    dispatchEnded() {
      [...this.endedListeners].forEach((listener) => listener());
      this.endedListeners.clear();
    }
  }

  interface FakePublication {
    source: string;
    track: { source: string; mediaStreamTrack?: FakeMediaStreamTrack };
  }

  class FakeRemoteAudioTrack {
    kind = Track.Kind.Audio;
  }

  class FakeRemoteVideoTrack {
    kind = Track.Kind.Video;
    attach = vi.fn((element?: HTMLMediaElement) => element ?? document.createElement("video"));
    detach = vi.fn((element?: HTMLMediaElement) => (element ? [element] : []));
  }

  class FakeRemotePublication {
    kind: string;
    source: string;
    trackSid: string;
    trackName: string;
    track?: FakeRemoteAudioTrack | FakeRemoteVideoTrack;
    isSubscribed = false;
    isEnabled = false;
    setSubscribed = vi.fn((subscribed: boolean) => {
      this.isSubscribed = subscribed;
    });
    setEnabled = vi.fn((enabled: boolean) => {
      this.isEnabled = enabled;
    });

    constructor(args: {
      sid: string;
      kind: string;
      source: string;
      track?: FakeRemoteAudioTrack | FakeRemoteVideoTrack;
      name?: string;
    }) {
      this.kind = args.kind;
      this.source = args.source;
      this.trackSid = args.sid;
      this.trackName = args.name ?? args.sid;
      this.track = args.track;
    }
  }

  class FakeRemoteParticipant extends FakeEmitter {
    trackPublications = new Map<string, FakeRemotePublication>();

    constructor(
      readonly identity: string,
      publications: FakeRemotePublication[] = [],
    ) {
      super();
      publications.forEach((publication) => this.addPublication(publication));
    }

    addPublication(publication: FakeRemotePublication) {
      this.trackPublications.set(publication.trackSid, publication);
    }

    getTrackPublicationBySid = vi.fn((sid: string) => this.trackPublications.get(sid));
  }

  let nextRemoteParticipants: Array<{
    identity: string;
    publications: FakeRemotePublication[];
  }> = [];

  class FakeLocalParticipant extends FakeEmitter {
    identity = "1";
    screenPublication: FakePublication | undefined;
    private nextScreenShareError: unknown;

    constructor(private owner: FakeRoom) {
      super();
    }

    setMicrophoneEnabled = vi.fn(async (_enabled: boolean) => {});
    setCameraEnabled = vi.fn(async (_enabled: boolean) => {});
    getTrackPublication = vi.fn((source: string) => {
      if (source === Track.Source.ScreenShare) return this.screenPublication;
      return undefined;
    });
    setScreenShareEnabled = vi.fn(async (enabled: boolean, _options?: unknown) => {
      if (this.nextScreenShareError) {
        const error = this.nextScreenShareError;
        this.nextScreenShareError = undefined;
        throw error;
      }
      if (enabled) {
        this.screenPublication = {
          source: Track.Source.ScreenShare,
          track: {
            source: Track.Source.ScreenShare,
            mediaStreamTrack: new FakeMediaStreamTrack(),
          },
        };
        return this.screenPublication;
      }
      const publication = this.screenPublication;
      this.screenPublication = undefined;
      if (publication) this.owner.emit(RoomEvent.LocalTrackUnpublished, publication);
      return undefined;
    });

    failNextScreenShare(error: unknown) {
      this.nextScreenShareError = error;
    }
  }

  class FakeRoom extends FakeEmitter {
    localParticipant: FakeLocalParticipant;
    remoteParticipants = new Map<string, FakeRemoteParticipant>();
    connect = vi.fn(async (_url: string, _token: string, _options?: unknown) => {});
    disconnect = vi.fn(async () => {});

    constructor(readonly options: unknown) {
      super();
      this.localParticipant = new FakeLocalParticipant(this);
      nextRemoteParticipants.forEach((participant) => {
        this.remoteParticipants.set(
          participant.identity,
          new FakeRemoteParticipant(participant.identity, participant.publications),
        );
      });
    }
  }

  const rooms: FakeRoom[] = [];
  const Room = vi.fn(function Room(options: unknown) {
    const room = new FakeRoom(options);
    rooms.push(room);
    return room;
  });

  return {
    FakeMediaStreamTrack,
    FakeRemoteAudioTrack,
    FakeRemoteParticipant,
    FakeRemotePublication,
    FakeRemoteVideoTrack,
    ParticipantEvent,
    RemoteAudioTrack: FakeRemoteAudioTrack,
    RemoteVideoTrack: FakeRemoteVideoTrack,
    Room,
    RoomEvent,
    Track,
    rooms,
    seedNextRemoteParticipants(
      participants: Array<{ identity: string; publications: FakeRemotePublication[] }>,
    ) {
      nextRemoteParticipants = participants;
    },
    reset() {
      rooms.splice(0, rooms.length);
      nextRemoteParticipants = [];
      Room.mockClear();
    },
  };
});

vi.mock("livekit-client", () => livekitMock);

vi.mock("../api", () => ({
  getVoiceToken: apiMock.getVoiceToken,
  postVoiceSpeaking: apiMock.postVoiceSpeaking,
}));

vi.mock("../voice/audio-routing", () => ({
  createAudioRouter: () => audioMock.router,
}));

vi.mock("../voice/settings", () => ({
  VOICE_INPUT_STORAGE_KEY: "hamlet.voice.inputDeviceId",
  getInputGain: () => 1,
  getNoiseSuppressionEnabled: () => true,
}));

vi.mock("../voice/livekit", () => ({
  applyInputGain: vi.fn(async () => {}),
}));

import { VoiceChatProvider, useVoiceChat } from "./voice-chat";

function VoiceHarness() {
  const voice = useVoiceChat();
  return (
    <>
      <div data-testid="active-channel">{voice.activeChannelId() ?? "none"}</div>
      <div data-testid="muted">{voice.isMuted() ? "muted" : "unmuted"}</div>
      <div data-testid="deafened">{voice.isDeafened() ? "deafened" : "undeafened"}</div>
      <div data-testid="sharing">{voice.isScreenSharing() ? "sharing" : "not-sharing"}</div>
      <div data-testid="starting">{voice.isScreenShareStarting() ? "starting" : "idle"}</div>
      <div data-testid="watching">{voice.watchingScreenShare()?.track_sid ?? "none"}</div>
      <div data-testid="watch-track">{voice.watchingScreenShareTrack() ? "video" : "none"}</div>
      <Show when={voice.lastError()}>{(message) => <div role="alert">{message()}</div>}</Show>
      <button type="button" onClick={() => void voice.join(42)}>
        Join 42
      </button>
      <button type="button" onClick={() => void voice.join(99)}>
        Join 99
      </button>
      <button type="button" onClick={() => void voice.leave()}>
        Leave
      </button>
      <button type="button" onClick={() => void voice.toggleMuted()}>
        Toggle mute
      </button>
      <button type="button" onClick={voice.toggleDeafened}>
        Toggle deafen
      </button>
      <button type="button" onClick={() => void voice.startScreenShare().catch(() => {})}>
        Start screen share
      </button>
      <button type="button" onClick={() => void voice.stopScreenShare().catch(() => {})}>
        Stop screen share
      </button>
      <button
        type="button"
        onClick={() =>
          void voice
            .watchScreenShare({
              channel_id: 42,
              sharer_user_id: 2,
              username: "bob",
              display_name: "Bobby",
              avatar_url: null,
              participant_identity: "2",
              track_sid: "TR_bob_screen",
              track_name: "screen",
              source: "screen_share",
              started_at: 1,
            })
            .catch(() => {})
        }
      >
        Watch Bob
      </button>
      <button
        type="button"
        onClick={() =>
          void voice
            .watchScreenShare({
              channel_id: 42,
              sharer_user_id: 3,
              username: "carol",
              display_name: null,
              avatar_url: null,
              participant_identity: "3",
              track_sid: "TR_carol_screen",
              track_name: "screen",
              source: "screen_share",
              started_at: 2,
            })
            .catch(() => {})
        }
      >
        Watch Carol
      </button>
      <button type="button" onClick={() => void voice.stopWatchingScreenShare()}>
        Stop watching
      </button>
    </>
  );
}

function renderVoiceHarness() {
  render(() => (
    <VoiceChatProvider>
      <VoiceHarness />
    </VoiceChatProvider>
  ));
}

async function joinVoiceChannel(buttonName = "Join 42", expectedChannelId = "42") {
  fireEvent.click(screen.getByRole("button", { name: buttonName }));
  await waitFor(() => {
    expect(screen.getByTestId("active-channel")).toHaveTextContent(expectedChannelId);
  });
  return livekitMock.rooms[livekitMock.rooms.length - 1];
}

async function startScreenShare() {
  fireEvent.click(screen.getByRole("button", { name: "Start screen share" }));
  await waitFor(() => {
    expect(screen.getByTestId("sharing")).toHaveTextContent("sharing");
  });
}

function firstCallOrderAfter(
  calls: readonly unknown[][],
  invocationCallOrder: readonly number[],
  startIndex: number,
  predicate: (args: unknown[]) => boolean,
): number {
  const index = calls.findIndex((args, callIndex) => {
    return callIndex >= startIndex && predicate(args);
  });
  if (index < 0) throw new Error("matching call not found");
  return invocationCallOrder[index] ?? Number.POSITIVE_INFINITY;
}

beforeEach(() => {
  livekitMock.reset();
  apiMock.getVoiceToken.mockReset();
  apiMock.postVoiceSpeaking.mockReset();
  audioMock.router.attach.mockClear();
  audioMock.router.detach.mockClear();
  audioMock.router.detachAll.mockClear();
  audioMock.router.setDeafened.mockClear();
  apiMock.getVoiceToken.mockImplementation(async (channelId: number) => ({
    url: "ws://livekit.test",
    token: `token-${channelId}`,
    room: `channel-${channelId}`,
  }));
  apiMock.postVoiceSpeaking.mockResolvedValue(undefined);
});

describe("VoiceChatProvider screen sharing", () => {
  test("joins voice with manual remote subscriptions for microphone audio only", async () => {
    const micTrack = new livekitMock.FakeRemoteAudioTrack();
    const screenTrack = new livekitMock.FakeRemoteVideoTrack();
    const micPublication = new livekitMock.FakeRemotePublication({
      sid: "TR_bob_mic",
      kind: livekitMock.Track.Kind.Audio,
      source: livekitMock.Track.Source.Microphone,
      track: micTrack,
    });
    const screenPublication = new livekitMock.FakeRemotePublication({
      sid: "TR_bob_screen",
      kind: livekitMock.Track.Kind.Video,
      source: livekitMock.Track.Source.ScreenShare,
      track: screenTrack,
    });
    livekitMock.seedNextRemoteParticipants([
      { identity: "2", publications: [micPublication, screenPublication] },
    ]);

    renderVoiceHarness();
    const room = await joinVoiceChannel();
    const participant = room.remoteParticipants.get("2");
    if (!participant) throw new Error("expected seeded remote participant");

    expect(room.connect).toHaveBeenCalledWith("ws://livekit.test", "token-42", {
      autoSubscribe: false,
    });
    expect(micPublication.setSubscribed).toHaveBeenCalledWith(true);
    expect(screenPublication.setEnabled).toHaveBeenLastCalledWith(false);
    expect(screenPublication.setSubscribed).toHaveBeenLastCalledWith(false);
    expect(screenPublication.setSubscribed).not.toHaveBeenCalledWith(true);

    room.emit(livekitMock.RoomEvent.TrackSubscribed, micTrack, micPublication, participant);

    expect(audioMock.router.attach).toHaveBeenCalledWith(micTrack);
    expect(screen.getByTestId("watch-track")).toHaveTextContent("none");
  });

  test("watching one screen share subscribes only that publication and stop preserves audio", async () => {
    const micTrack = new livekitMock.FakeRemoteAudioTrack();
    const selectedTrack = new livekitMock.FakeRemoteVideoTrack();
    const otherTrack = new livekitMock.FakeRemoteVideoTrack();
    const micPublication = new livekitMock.FakeRemotePublication({
      sid: "TR_bob_mic",
      kind: livekitMock.Track.Kind.Audio,
      source: livekitMock.Track.Source.Microphone,
      track: micTrack,
    });
    const selectedPublication = new livekitMock.FakeRemotePublication({
      sid: "TR_bob_screen",
      kind: livekitMock.Track.Kind.Video,
      source: livekitMock.Track.Source.ScreenShare,
      track: selectedTrack,
    });
    const otherPublication = new livekitMock.FakeRemotePublication({
      sid: "TR_carol_screen",
      kind: livekitMock.Track.Kind.Video,
      source: livekitMock.Track.Source.ScreenShare,
      track: otherTrack,
    });
    livekitMock.seedNextRemoteParticipants([
      { identity: "2", publications: [micPublication, selectedPublication] },
      { identity: "3", publications: [otherPublication] },
    ]);

    renderVoiceHarness();
    const room = await joinVoiceChannel();
    const participant = room.remoteParticipants.get("2");
    if (!participant) throw new Error("expected seeded remote participant");
    room.emit(livekitMock.RoomEvent.TrackSubscribed, micTrack, micPublication, participant);

    fireEvent.click(screen.getByRole("button", { name: "Watch Bob" }));

    await waitFor(() => {
      expect(screen.getByTestId("watching")).toHaveTextContent("TR_bob_screen");
    });
    expect(selectedPublication.setEnabled).toHaveBeenLastCalledWith(true);
    expect(selectedPublication.setSubscribed).toHaveBeenLastCalledWith(true);
    expect(otherPublication.setEnabled).toHaveBeenLastCalledWith(false);
    expect(otherPublication.setSubscribed).toHaveBeenLastCalledWith(false);
    expect(otherPublication.setSubscribed).not.toHaveBeenCalledWith(true);

    room.emit(
      livekitMock.RoomEvent.TrackSubscribed,
      selectedTrack,
      selectedPublication,
      participant,
    );
    await waitFor(() => expect(screen.getByTestId("watch-track")).toHaveTextContent("video"));

    fireEvent.click(screen.getByRole("button", { name: "Stop watching" }));

    await waitFor(() => expect(screen.getByTestId("watching")).toHaveTextContent("none"));
    expect(selectedPublication.setEnabled).toHaveBeenLastCalledWith(false);
    expect(selectedPublication.setSubscribed).toHaveBeenLastCalledWith(false);
    expect(screen.getByTestId("active-channel")).toHaveTextContent("42");
    expect(audioMock.router.attach).toHaveBeenCalledWith(micTrack);
    expect(audioMock.router.detach).not.toHaveBeenCalled();
    expect(audioMock.router.detachAll).not.toHaveBeenCalled();
  });

  test("switching watched screen shares disables the previous stream before receiving the next", async () => {
    const bobTrack = new livekitMock.FakeRemoteVideoTrack();
    const carolTrack = new livekitMock.FakeRemoteVideoTrack();
    const carolPublication = new livekitMock.FakeRemotePublication({
      sid: "TR_carol_screen",
      kind: livekitMock.Track.Kind.Video,
      source: livekitMock.Track.Source.ScreenShare,
      track: carolTrack,
    });
    const bobPublication = new livekitMock.FakeRemotePublication({
      sid: "TR_bob_screen",
      kind: livekitMock.Track.Kind.Video,
      source: livekitMock.Track.Source.ScreenShare,
      track: bobTrack,
    });
    livekitMock.seedNextRemoteParticipants([
      { identity: "3", publications: [carolPublication] },
      { identity: "2", publications: [bobPublication] },
    ]);

    renderVoiceHarness();
    const room = await joinVoiceChannel();
    const bob = room.remoteParticipants.get("2");
    if (!bob) throw new Error("expected seeded Bob participant");

    fireEvent.click(screen.getByRole("button", { name: "Watch Bob" }));
    await waitFor(() => {
      expect(screen.getByTestId("watching")).toHaveTextContent("TR_bob_screen");
    });
    room.emit(livekitMock.RoomEvent.TrackSubscribed, bobTrack, bobPublication, bob);
    await waitFor(() => expect(screen.getByTestId("watch-track")).toHaveTextContent("video"));
    expect(bobPublication.isSubscribed).toBe(true);
    expect(carolPublication.isSubscribed).toBe(false);

    const bobSubscribedCallsBeforeSwitch = bobPublication.setSubscribed.mock.calls.length;
    const carolSubscribedCallsBeforeSwitch = carolPublication.setSubscribed.mock.calls.length;

    fireEvent.click(screen.getByRole("button", { name: "Watch Carol" }));

    await waitFor(() => {
      expect(screen.getByTestId("watching")).toHaveTextContent("TR_carol_screen");
    });
    const bobUnsubscribedOrder = firstCallOrderAfter(
      bobPublication.setSubscribed.mock.calls,
      bobPublication.setSubscribed.mock.invocationCallOrder,
      bobSubscribedCallsBeforeSwitch,
      ([subscribed]) => subscribed === false,
    );
    const carolSubscribedOrder = firstCallOrderAfter(
      carolPublication.setSubscribed.mock.calls,
      carolPublication.setSubscribed.mock.invocationCallOrder,
      carolSubscribedCallsBeforeSwitch,
      ([subscribed]) => subscribed === true,
    );
    expect(bobUnsubscribedOrder).toBeLessThan(carolSubscribedOrder);
    expect(bobPublication.isEnabled).toBe(false);
    expect(bobPublication.isSubscribed).toBe(false);
    expect(carolPublication.isEnabled).toBe(true);
    expect(carolPublication.isSubscribed).toBe(true);
  });

  test("publishes one display screen-share video track without requesting camera", async () => {
    renderVoiceHarness();
    const room = await joinVoiceChannel();

    await startScreenShare();

    expect(room.localParticipant.setScreenShareEnabled).toHaveBeenCalledWith(
      true,
      expect.objectContaining({
        audio: false,
        video: true,
        systemAudio: "exclude",
      }),
    );
    expect(room.localParticipant.setCameraEnabled).not.toHaveBeenCalled();
    expect(room.localParticipant.setMicrophoneEnabled).toHaveBeenCalledTimes(1);
    expect(room.localParticipant.setMicrophoneEnabled).toHaveBeenLastCalledWith(true);
    expect(screen.getByTestId("active-channel")).toHaveTextContent("42");
    expect(screen.getByTestId("starting")).toHaveTextContent("idle");
  });

  test("rejects starting another local screen share while one is active", async () => {
    renderVoiceHarness();
    const room = await joinVoiceChannel();
    await startScreenShare();

    fireEvent.click(screen.getByRole("button", { name: "Start screen share" }));

    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent(
        "Stop your current screen share before starting another.",
      );
    });
    expect(room.localParticipant.setScreenShareEnabled).toHaveBeenCalledTimes(1);
    expect(room.localParticipant.setScreenShareEnabled).toHaveBeenCalledWith(
      true,
      expect.objectContaining({ video: true }),
    );
    expect(screen.getByTestId("sharing")).toHaveTextContent("sharing");
  });

  test("stops sharing by unpublishing the local screen-share track", async () => {
    renderVoiceHarness();
    const room = await joinVoiceChannel();
    await startScreenShare();

    fireEvent.click(screen.getByRole("button", { name: "Stop screen share" }));

    await waitFor(() => {
      expect(room.localParticipant.setScreenShareEnabled).toHaveBeenLastCalledWith(false);
      expect(screen.getByTestId("sharing")).toHaveTextContent("not-sharing");
    });
    expect(screen.getByTestId("active-channel")).toHaveTextContent("42");
  });

  test("capture failure leaves voice joined and preserves mute/deafen state", async () => {
    renderVoiceHarness();
    const room = await joinVoiceChannel();
    fireEvent.click(screen.getByRole("button", { name: "Toggle mute" }));
    fireEvent.click(screen.getByRole("button", { name: "Toggle deafen" }));
    await waitFor(() => {
      expect(screen.getByTestId("muted")).toHaveTextContent("muted");
      expect(screen.getByTestId("deafened")).toHaveTextContent("deafened");
    });

    const denied = new Error("Permission denied");
    denied.name = "NotAllowedError";
    room.localParticipant.failNextScreenShare(denied);
    fireEvent.click(screen.getByRole("button", { name: "Start screen share" }));

    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent("Screen share was canceled or denied.");
      expect(screen.getByTestId("sharing")).toHaveTextContent("not-sharing");
    });
    expect(screen.getByTestId("active-channel")).toHaveTextContent("42");
    expect(screen.getByTestId("muted")).toHaveTextContent("muted");
    expect(screen.getByTestId("deafened")).toHaveTextContent("deafened");
    expect(room.disconnect).not.toHaveBeenCalled();
    expect(room.localParticipant.setCameraEnabled).not.toHaveBeenCalled();
    expect(room.localParticipant.setMicrophoneEnabled).toHaveBeenCalledTimes(2);
    expect(room.localParticipant.setMicrophoneEnabled).toHaveBeenNthCalledWith(1, true);
    expect(room.localParticipant.setMicrophoneEnabled).toHaveBeenNthCalledWith(2, false);
  });

  test("switching voice channels stops the previous local screen share", async () => {
    renderVoiceHarness();
    const firstRoom = await joinVoiceChannel();
    await startScreenShare();

    const secondRoom = await joinVoiceChannel("Join 99", "99");

    expect(firstRoom.localParticipant.setScreenShareEnabled).toHaveBeenLastCalledWith(false);
    expect(firstRoom.disconnect).toHaveBeenCalled();
    expect(secondRoom.connect).toHaveBeenCalledWith("ws://livekit.test", "token-99", {
      autoSubscribe: false,
    });
    expect(screen.getByTestId("sharing")).toHaveTextContent("not-sharing");
  });

  test("browser or OS ended events clear the local sharing indicator", async () => {
    renderVoiceHarness();
    const room = await joinVoiceChannel();
    await startScreenShare();

    room.emit(livekitMock.RoomEvent.LocalTrackUnpublished, room.localParticipant.screenPublication);

    await waitFor(() => {
      expect(screen.getByTestId("sharing")).toHaveTextContent("not-sharing");
    });
    expect(screen.getByTestId("active-channel")).toHaveTextContent("42");
    expect(room.disconnect).not.toHaveBeenCalled();
  });

  test("local screen-share media track ended events unpublish and allow cleanup", async () => {
    renderVoiceHarness();
    const room = await joinVoiceChannel();
    await startScreenShare();
    const mediaTrack = room.localParticipant.screenPublication?.track.mediaStreamTrack;
    if (!mediaTrack) throw new Error("expected a fake screen-share media track");
    const callsBeforeEnd = room.localParticipant.setScreenShareEnabled.mock.calls.length;

    mediaTrack.dispatchEnded();

    await waitFor(() => {
      expect(screen.getByTestId("sharing")).toHaveTextContent("not-sharing");
    });
    expect(room.localParticipant.setScreenShareEnabled).toHaveBeenCalledTimes(callsBeforeEnd + 1);
    expect(room.localParticipant.setScreenShareEnabled).toHaveBeenLastCalledWith(false);
    expect(screen.getByTestId("active-channel")).toHaveTextContent("42");
    expect(room.disconnect).not.toHaveBeenCalled();
  });

  test("stale local unpublish events do not clear a newer screen share", async () => {
    renderVoiceHarness();
    const room = await joinVoiceChannel();
    await startScreenShare();
    const firstPublication = room.localParticipant.screenPublication;
    if (!firstPublication) throw new Error("expected first screen publication");

    fireEvent.click(screen.getByRole("button", { name: "Stop screen share" }));
    await waitFor(() => {
      expect(screen.getByTestId("sharing")).toHaveTextContent("not-sharing");
    });

    await startScreenShare();
    const secondPublication = room.localParticipant.screenPublication;
    if (!secondPublication) throw new Error("expected second screen publication");
    expect(secondPublication).not.toBe(firstPublication);

    room.emit(livekitMock.RoomEvent.LocalTrackUnpublished, firstPublication);
    await Promise.resolve();

    expect(screen.getByTestId("sharing")).toHaveTextContent("sharing");
    expect(room.localParticipant.screenPublication).toBe(secondPublication);
  });

  test("remote track unpublished events close the viewer and unsubscribe the publication", async () => {
    const videoTrack = new livekitMock.FakeRemoteVideoTrack();
    const publication = new livekitMock.FakeRemotePublication({
      sid: "TR_bob_screen",
      kind: livekitMock.Track.Kind.Video,
      source: livekitMock.Track.Source.ScreenShare,
      track: videoTrack,
    });
    livekitMock.seedNextRemoteParticipants([{ identity: "2", publications: [publication] }]);

    renderVoiceHarness();
    const room = await joinVoiceChannel();
    const participant = room.remoteParticipants.get("2");
    if (!participant) throw new Error("expected seeded remote participant");

    fireEvent.click(screen.getByRole("button", { name: "Watch Bob" }));
    await waitFor(() => {
      expect(screen.getByTestId("watching")).toHaveTextContent("TR_bob_screen");
    });
    expect(publication.isSubscribed).toBe(true);

    room.emit(livekitMock.RoomEvent.TrackUnpublished, publication, participant);

    await waitFor(() => expect(screen.getByTestId("watching")).toHaveTextContent("none"));
    expect(screen.getByTestId("watch-track")).toHaveTextContent("none");
    expect(publication.setEnabled).toHaveBeenLastCalledWith(false);
    expect(publication.setSubscribed).toHaveBeenLastCalledWith(false);
  });

  test("remote participant disconnect closes the viewer and disables screen video", async () => {
    const videoTrack = new livekitMock.FakeRemoteVideoTrack();
    const publication = new livekitMock.FakeRemotePublication({
      sid: "TR_bob_screen",
      kind: livekitMock.Track.Kind.Video,
      source: livekitMock.Track.Source.ScreenShare,
      track: videoTrack,
    });
    livekitMock.seedNextRemoteParticipants([{ identity: "2", publications: [publication] }]);

    renderVoiceHarness();
    const room = await joinVoiceChannel();
    const participant = room.remoteParticipants.get("2");
    if (!participant) throw new Error("expected seeded remote participant");

    fireEvent.click(screen.getByRole("button", { name: "Watch Bob" }));
    await waitFor(() => {
      expect(screen.getByTestId("watching")).toHaveTextContent("TR_bob_screen");
    });

    room.emit(livekitMock.RoomEvent.ParticipantDisconnected, participant);

    await waitFor(() => expect(screen.getByTestId("watching")).toHaveTextContent("none"));
    expect(publication.setEnabled).toHaveBeenLastCalledWith(false);
    expect(publication.setSubscribed).toHaveBeenLastCalledWith(false);
  });

  test("switching voice channels stops watching and releases previous screen video", async () => {
    const videoTrack = new livekitMock.FakeRemoteVideoTrack();
    const publication = new livekitMock.FakeRemotePublication({
      sid: "TR_bob_screen",
      kind: livekitMock.Track.Kind.Video,
      source: livekitMock.Track.Source.ScreenShare,
      track: videoTrack,
    });
    livekitMock.seedNextRemoteParticipants([{ identity: "2", publications: [publication] }]);

    renderVoiceHarness();
    const firstRoom = await joinVoiceChannel();
    fireEvent.click(screen.getByRole("button", { name: "Watch Bob" }));
    await waitFor(() => {
      expect(screen.getByTestId("watching")).toHaveTextContent("TR_bob_screen");
    });
    expect(publication.isSubscribed).toBe(true);

    const secondRoom = await joinVoiceChannel("Join 99", "99");

    expect(publication.setEnabled).toHaveBeenLastCalledWith(false);
    expect(publication.setSubscribed).toHaveBeenLastCalledWith(false);
    expect(firstRoom.disconnect).toHaveBeenCalled();
    expect(secondRoom.connect).toHaveBeenCalledWith("ws://livekit.test", "token-99", {
      autoSubscribe: false,
    });
    expect(screen.getByTestId("watching")).toHaveTextContent("none");
  });
});
