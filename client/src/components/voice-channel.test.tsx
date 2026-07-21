import { describe, expect, test, vi } from "vitest";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { renderNative } from "../test/render";
import { cloneElement, useReducer, type ReactElement } from "react";
import { flushSync } from "react-dom";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import type {
  CameraStream,
  CameraVideoStopped,
  Channel,
  ScreenShareStopped,
  ScreenShareStream,
  VoiceParticipant,
  VoiceParticipantLeft,
  VoiceParticipantSpeaking,
  VoiceParticipantStatus,
} from "../api";
import { mswState, resetMswState, server } from "../test/msw/server";
import VoiceChannel from "./voice-channel";

type JoinedListener = (p: VoiceParticipant) => void;
type LeftListener = (p: VoiceParticipantLeft) => void;
type SpeakingListener = (p: VoiceParticipantSpeaking) => void;
type StatusListener = (p: VoiceParticipantStatus) => void;
type ScreenShareStartedListener = (p: ScreenShareStream) => void;
type ScreenShareStoppedListener = (p: ScreenShareStopped) => void;
type CameraStartedListener = (p: CameraStream) => void;
type CameraStoppedListener = (p: CameraVideoStopped) => void;
type ConnectedListener = () => void;

interface MockVoiceChatApi {
  activeChannelId: number | null;
  setActiveChannelId: (id: number | null) => void;
  isConnecting: boolean;
  isMuted: boolean;
  setIsMuted: (value: boolean) => void;
  isDeafened: boolean;
  setIsDeafened: (value: boolean) => void;
  isScreenSharing: boolean;
  setIsScreenSharing: (value: boolean) => void;
  screenShareStatus: "off" | "starting" | "on" | "stopping";
  isScreenShareBusy: boolean;
  setScreenShareStatus: (value: "off" | "starting" | "on" | "stopping") => void;
  isCameraEnabled: boolean;
  setIsCameraEnabled: (value: boolean) => void;
  cameraStatus: "off" | "starting" | "on" | "stopping";
  isCameraBusy: boolean;
  setCameraStatus: (value: "off" | "starting" | "on" | "stopping") => void;
  localCameraTrack: null;
  remoteCameraTiles: readonly [];
  watchingScreenShare: ScreenShareStream | null;
  setWatchingScreenShare: (stream: ScreenShareStream | null) => void;
  watchingScreenShareTrack: null;
  lastError: string | null;
  speakingUserIds: ReadonlySet<number>;
  setSpeakingUserIds: (ids: ReadonlySet<number>) => void;
  join: ReturnType<typeof vi.fn>;
  leave: ReturnType<typeof vi.fn>;
  toggleMuted: ReturnType<typeof vi.fn>;
  toggleDeafened: ReturnType<typeof vi.fn>;
  startScreenShare: ReturnType<typeof vi.fn>;
  stopScreenShare: ReturnType<typeof vi.fn>;
  startCamera: ReturnType<typeof vi.fn>;
  stopCamera: ReturnType<typeof vi.fn>;
  syncRemoteCameraStreams: ReturnType<typeof vi.fn>;
  watchScreenShare: ReturnType<typeof vi.fn>;
  stopWatchingScreenShare: ReturnType<typeof vi.fn>;
}

let mockVoice: MockVoiceChatApi;
const preferenceMock = vi.hoisted(() => ({ showSpeakingEverywhere: false }));
let notify: () => void = () => undefined;
const joinedListeners = new Set<JoinedListener>();
const leftListeners = new Set<LeftListener>();
const speakingListeners = new Set<SpeakingListener>();
const statusListeners = new Set<StatusListener>();
const screenShareStartedListeners = new Set<ScreenShareStartedListener>();
const screenShareStoppedListeners = new Set<ScreenShareStoppedListener>();
const cameraStartedListeners = new Set<CameraStartedListener>();
const cameraStoppedListeners = new Set<CameraStoppedListener>();
const connectedListeners = new Set<ConnectedListener>();

vi.mock("../contexts/voice-chat", () => ({ useVoiceChat: () => mockVoice }));
vi.mock("../contexts/voice-preferences", () => ({
  useVoicePreferences: () => preferenceMock,
}));

vi.mock("../contexts/events", () => {
  let events: object | undefined;
  return {
    useEvents: () =>
      (events ??= {
        onVoiceParticipantJoined: (cb: JoinedListener) => {
          joinedListeners.add(cb);
          return () => joinedListeners.delete(cb);
        },
        onVoiceParticipantLeft: (cb: LeftListener) => {
          leftListeners.add(cb);
          return () => leftListeners.delete(cb);
        },
        onVoiceParticipantSpeakingChanged: (cb: SpeakingListener) => {
          speakingListeners.add(cb);
          return () => speakingListeners.delete(cb);
        },
        onVoiceParticipantStatusChanged: (cb: StatusListener) => {
          statusListeners.add(cb);
          return () => statusListeners.delete(cb);
        },
        onScreenShareStarted: (cb: ScreenShareStartedListener) => {
          screenShareStartedListeners.add(cb);
          return () => screenShareStartedListeners.delete(cb);
        },
        onScreenShareStopped: (cb: ScreenShareStoppedListener) => {
          screenShareStoppedListeners.add(cb);
          return () => screenShareStoppedListeners.delete(cb);
        },
        onCameraVideoStarted: (cb: CameraStartedListener) => {
          cameraStartedListeners.add(cb);
          return () => cameraStartedListeners.delete(cb);
        },
        onCameraVideoStopped: (cb: CameraStoppedListener) => {
          cameraStoppedListeners.add(cb);
          return () => cameraStoppedListeners.delete(cb);
        },
        onConnected: (cb: ConnectedListener) => {
          connectedListeners.add(cb);
          return () => connectedListeners.delete(cb);
        },
      }),
  };
});

function updateMock<K extends keyof MockVoiceChatApi>(key: K, value: MockVoiceChatApi[K]) {
  mockVoice[key] = value;
  notify();
}

function makeVoiceMock(overrides?: Partial<MockVoiceChatApi>): MockVoiceChatApi {
  const voice: MockVoiceChatApi = {
    activeChannelId: null,
    setActiveChannelId: (value) => updateMock("activeChannelId", value),
    isConnecting: false,
    isMuted: false,
    setIsMuted: (value) => updateMock("isMuted", value),
    isDeafened: false,
    setIsDeafened: (value) => updateMock("isDeafened", value),
    isScreenSharing: false,
    setIsScreenSharing: (value) => updateMock("isScreenSharing", value),
    screenShareStatus: "off",
    isScreenShareBusy: false,
    setScreenShareStatus: (value) => {
      mockVoice.screenShareStatus = value;
      mockVoice.isScreenShareBusy = value === "starting" || value === "stopping";
      notify();
    },
    isCameraEnabled: false,
    setIsCameraEnabled: (value) => updateMock("isCameraEnabled", value),
    cameraStatus: "off",
    isCameraBusy: false,
    setCameraStatus: (value) => {
      mockVoice.cameraStatus = value;
      mockVoice.isCameraBusy = value === "starting" || value === "stopping";
      notify();
    },
    localCameraTrack: null,
    remoteCameraTiles: [],
    watchingScreenShare: null,
    setWatchingScreenShare: (value) => updateMock("watchingScreenShare", value),
    watchingScreenShareTrack: null,
    lastError: null,
    speakingUserIds: new Set(),
    setSpeakingUserIds: (value) => updateMock("speakingUserIds", value),
    join: vi.fn<(id: number) => Promise<void>>().mockResolvedValue(),
    leave: vi.fn<() => Promise<void>>().mockResolvedValue(),
    toggleMuted: vi.fn<() => Promise<void>>().mockResolvedValue(),
    toggleDeafened: vi.fn<() => Promise<void>>().mockResolvedValue(),
    startScreenShare: vi.fn<() => Promise<void>>().mockResolvedValue(),
    stopScreenShare: vi.fn<() => Promise<void>>().mockResolvedValue(),
    startCamera: vi.fn<() => Promise<void>>().mockResolvedValue(),
    stopCamera: vi.fn<() => Promise<void>>().mockResolvedValue(),
    syncRemoteCameraStreams: vi.fn<(channelId: number, streams: readonly CameraStream[]) => void>(),
    watchScreenShare: vi.fn<(stream: ScreenShareStream) => Promise<void>>().mockResolvedValue(),
    stopWatchingScreenShare: vi.fn<() => Promise<void>>().mockResolvedValue(),
    ...overrides,
  };
  return voice;
}

function setShowEverywhere(value: boolean) {
  preferenceMock.showSpeakingEverywhere = value;
  notify();
}

function renderVoice(element: ReactElement) {
  function Harness() {
    const [, rerender] = useReducer((value: number) => value + 1, 0);
    notify = () => flushSync(() => rerender());
    return cloneElement(element);
  }
  return renderNative(<Harness />);
}

const CHANNEL: Channel = { id: 42, name: "lobby", position: 0, type: "voice" };

function makeParticipant(overrides: Partial<VoiceParticipant> = {}): VoiceParticipant {
  return {
    user_id: 2,
    channel_id: 42,
    username: "bob",
    avatar_url: null,
    muted: false,
    deafened: false,
    ...overrides,
  };
}

function makeScreenShare(overrides: Partial<ScreenShareStream> = {}): ScreenShareStream {
  return {
    channel_id: 42,
    sharer_user_id: 2,
    username: "bob",
    display_name: null,
    avatar_url: null,
    participant_identity: "2",
    track_sid: "TR_bob_screen",
    track_name: "screen",
    source: "screen_share",
    started_at: 1_700_000_000,
    ...overrides,
  };
}

function makeCamera(overrides: Partial<CameraStream> = {}): CameraStream {
  return {
    channel_id: 42,
    sharer_user_id: 2,
    username: "bob",
    display_name: null,
    avatar_url: null,
    participant_identity: "2",
    track_sid: "TR_bob_camera",
    track_name: "camera",
    source: "camera",
    started_at: 1_700_000_000,
    ...overrides,
  };
}

function stoppedFrom(stream: ScreenShareStream): ScreenShareStopped {
  return {
    channel_id: stream.channel_id,
    sharer_user_id: stream.sharer_user_id,
    participant_identity: stream.participant_identity,
    track_sid: stream.track_sid,
  };
}

function cameraStoppedFrom(stream: CameraStream): CameraVideoStopped {
  return {
    channel_id: stream.channel_id,
    sharer_user_id: stream.sharer_user_id,
    participant_identity: stream.participant_identity,
    track_sid: stream.track_sid,
  };
}

function setup(
  initial: VoiceParticipant[] = [],
  screenShares: ScreenShareStream[] = [],
  cameras: CameraStream[] = [],
) {
  const state = resetMswState();
  state.me = {
    id: 1,
    username: "alice",
    display_name: null,
    email: null,
    email_verified: false,
    avatar_url: null,
  };
  state.voiceParticipants[String(CHANNEL.id)] = initial;
  state.screenShareStreams = screenShares;
  state.cameraStreams = cameras;
  joinedListeners.clear();
  leftListeners.clear();
  speakingListeners.clear();
  statusListeners.clear();
  screenShareStartedListeners.clear();
  screenShareStoppedListeners.clear();
  cameraStartedListeners.clear();
  cameraStoppedListeners.clear();
  connectedListeners.clear();
  notify = () => undefined;
  preferenceMock.showSpeakingEverywhere = false;
  mockVoice = makeVoiceMock();
  return state;
}

describe("<VoiceChannel>", () => {
  test("fetches and renders the initial participant list", async () => {
    setup([makeParticipant(), makeParticipant({ user_id: 3, username: "carol" })]);

    renderVoice(<VoiceChannel channel={CHANNEL} />);

    await waitFor(() => {
      expect(screen.getByText("bob")).toBeInTheDocument();
      expect(screen.getByText("carol")).toBeInTheDocument();
    });
  });

  test("adds a participant when a join SSE event arrives", async () => {
    setup();
    renderVoice(<VoiceChannel channel={CHANNEL} />);

    // Wait for the initial empty-list fetch to complete before pushing events,
    // so the SSE-driven append isn't clobbered by the resource resolving.
    await waitFor(() => expect(mswState().voiceParticipants["42"]).toBeDefined());

    joinedListeners.forEach((cb) => cb(makeParticipant({ user_id: 7, username: "dave" })));

    await waitFor(() => expect(screen.getByText("dave")).toBeInTheDocument());
  });

  test("renders participant mute/deafen status from fetch and SSE", async () => {
    setup([makeParticipant({ muted: true })]);
    renderVoice(<VoiceChannel channel={CHANNEL} />);

    await waitFor(() => expect(screen.getByRole("img", { name: /bob is muted/i })).toBeVisible());
    expect(screen.queryByRole("img", { name: /bob is deafened/i })).toBeNull();

    statusListeners.forEach((cb) =>
      cb({ channel_id: 42, user_id: 2, muted: false, deafened: true }),
    );

    await waitFor(() => expect(screen.queryByRole("img", { name: /bob is muted/i })).toBeNull());
    expect(screen.getByRole("img", { name: /bob is deafened/i })).toBeVisible();
  });

  test("ignores SSE events for other channels", async () => {
    setup();
    renderVoice(<VoiceChannel channel={CHANNEL} />);
    await waitFor(() => expect(mswState().voiceParticipants["42"]).toBeDefined());

    joinedListeners.forEach((cb) =>
      cb(makeParticipant({ user_id: 9, channel_id: 999, username: "ghost" })),
    );

    // Give the React update an event loop turn to propagate if it were going to.
    await Promise.resolve();
    expect(screen.queryByText("ghost")).toBeNull();
  });

  test("removes a participant on a left SSE event", async () => {
    setup([makeParticipant()]);
    renderVoice(<VoiceChannel channel={CHANNEL} />);

    await waitFor(() => expect(screen.getByText("bob")).toBeInTheDocument());

    leftListeners.forEach((cb) => cb({ channel_id: 42, user_id: 2 }));
    await waitFor(() => expect(screen.queryByText("bob")).toBeNull());
  });

  test("clicking the row when disconnected calls join(channelId)", async () => {
    setup();
    renderVoice(<VoiceChannel channel={CHANNEL} />);

    fireEvent.click(screen.getByRole("button", { name: /Join voice channel lobby/ }));

    await waitFor(() => expect(mockVoice.join).toHaveBeenCalledWith(42));
    expect(mockVoice.leave).not.toHaveBeenCalled();
  });

  test("clicking the row when active calls leave()", async () => {
    setup();
    renderVoice(<VoiceChannel channel={CHANNEL} />);
    mockVoice.setActiveChannelId(42);

    fireEvent.click(screen.getByRole("button", { name: /Leave voice channel lobby/ }));

    await waitFor(() => expect(mockVoice.leave).toHaveBeenCalled());
    expect(mockVoice.join).not.toHaveBeenCalled();
  });

  test("shows camera and screen-share controls only when connected to this channel", () => {
    setup();
    const { unmount } = renderVoice(<VoiceChannel channel={CHANNEL} />);
    expect(screen.queryByRole("button", { name: /Turn on camera/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /Share screen/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /Disconnect from voice/ })).toBeNull();

    unmount();
    mockVoice = makeVoiceMock();
    mockVoice.setActiveChannelId(42);
    renderVoice(<VoiceChannel channel={CHANNEL} />);

    expect(screen.getByRole("button", { name: /Turn on camera/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Share screen/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Disconnect from voice/ })).toBeNull();
  });

  test("renders a speaking ring on the avatar for in-channel speakers", async () => {
    setup([makeParticipant()]);
    mockVoice.setActiveChannelId(42);
    renderVoice(<VoiceChannel channel={CHANNEL} />);

    await waitFor(() => expect(screen.getByText("bob")).toBeInTheDocument());

    const avatar = screen.getByRole("img", { name: /bob's avatar/i });
    expect(avatar.className).not.toMatch(/ring-green-500/);

    mockVoice.setSpeakingUserIds(new Set([2]));
    await waitFor(() => expect(avatar.className).toMatch(/ring-green-500/));
  });

  test("does not show ring from SSE speaking events when not connected and setting off", async () => {
    setup([makeParticipant()]);
    // Not active — we're not connected to this channel.
    renderVoice(<VoiceChannel channel={CHANNEL} />);
    await waitFor(() => expect(screen.getByText("bob")).toBeInTheDocument());

    speakingListeners.forEach((cb) => cb({ channel_id: 42, user_id: 2, speaking: true }));
    await Promise.resolve();

    const avatar = screen.getByRole("img", { name: /bob's avatar/i });
    expect(avatar.className).not.toMatch(/ring-green-500/);
  });

  test("shows ring from SSE speaking events when setting is on and not connected", async () => {
    setup([makeParticipant()]);
    setShowEverywhere(true);
    renderVoice(<VoiceChannel channel={CHANNEL} />);
    await waitFor(() => expect(screen.getByText("bob")).toBeInTheDocument());

    speakingListeners.forEach((cb) => cb({ channel_id: 42, user_id: 2, speaking: true }));

    const avatar = screen.getByRole("img", { name: /bob's avatar/i });
    await waitFor(() => expect(avatar.className).toMatch(/ring-green-500/));

    speakingListeners.forEach((cb) => cb({ channel_id: 42, user_id: 2, speaking: false }));
    await waitFor(() => expect(avatar.className).not.toMatch(/ring-green-500/));
  });

  test("bootstraps active screen shares and marks sharing participants", async () => {
    setup(
      [makeParticipant(), makeParticipant({ user_id: 3, username: "carol" })],
      [
        makeScreenShare({ display_name: "Bobby" }),
        makeScreenShare({
          channel_id: 999,
          sharer_user_id: 9,
          username: "ghost",
          participant_identity: "9",
          track_sid: "TR_ghost_screen",
        }),
      ],
    );

    renderVoice(<VoiceChannel channel={CHANNEL} />);

    const shelf = await screen.findByRole("region", { name: /active screen shares in lobby/i });
    await screen.findByText("bob");
    expect(within(shelf).getByText("Bobby's screen")).toBeInTheDocument();
    expect(within(shelf).queryByText("ghost's screen")).toBeNull();
    expect(screen.getByRole("img", { name: /bob is sharing screen/i })).toBeInTheDocument();
    expect(screen.queryByRole("img", { name: /carol is sharing screen/i })).toBeNull();
    expect(within(shelf).getByText("Join voice")).toBeInTheDocument();
    expect(within(shelf).queryByRole("button", { name: /watch Bobby's screen share/i })).toBeNull();
    expect(mockVoice.join).not.toHaveBeenCalled();
  });

  test("bootstraps active cameras, marks participants, and waits for join before syncing tiles", async () => {
    const camera = makeCamera({ display_name: "Bobby" });
    setup(
      [makeParticipant(), makeParticipant({ user_id: 3, username: "carol" })],
      [],
      [
        camera,
        makeCamera({
          channel_id: 999,
          sharer_user_id: 9,
          username: "ghost",
          participant_identity: "9",
          track_sid: "TR_ghost_camera",
        }),
      ],
    );

    renderVoice(<VoiceChannel channel={CHANNEL} />);

    const cameraShelf = await screen.findByRole("region", { name: /active cameras in lobby/i });
    await screen.findByText("bob");
    expect(within(cameraShelf).getByText("1 camera live")).toBeInTheDocument();
    expect(within(cameraShelf).getByText("Join voice to view cameras.")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: /bob has camera on/i })).toBeInTheDocument();
    expect(screen.queryByRole("img", { name: /carol has camera on/i })).toBeNull();
    expect(mockVoice.syncRemoteCameraStreams).not.toHaveBeenCalledWith(42, [camera]);

    mockVoice.setActiveChannelId(42);

    await waitFor(() =>
      expect(mockVoice.syncRemoteCameraStreams).toHaveBeenLastCalledWith(42, [camera]),
    );
    expect(screen.queryByRole("region", { name: /active cameras in lobby/i })).toBeNull();
  });

  test("applies camera SSE start and stop updates with polite announcements", async () => {
    const camera = makeCamera({ display_name: "Bobby" });
    setup([makeParticipant()]);
    mockVoice.setActiveChannelId(42);
    renderVoice(<VoiceChannel channel={CHANNEL} />);

    await screen.findByText("bob");
    await waitFor(() => expect(cameraStartedListeners.size).toBe(1));

    cameraStartedListeners.forEach((cb) => cb(camera));

    await waitFor(() =>
      expect(screen.getByRole("img", { name: /bob has camera on/i })).toBeInTheDocument(),
    );
    await waitFor(() =>
      expect(mockVoice.syncRemoteCameraStreams).toHaveBeenLastCalledWith(42, [camera]),
    );
    let status = screen.getByRole("status");
    expect(status).toHaveAttribute("aria-live", "polite");
    expect(status).toHaveTextContent("Bobby turned on camera in lobby.");

    cameraStoppedListeners.forEach((cb) => cb(cameraStoppedFrom(camera)));

    await waitFor(() =>
      expect(screen.queryByRole("img", { name: /bob has camera on/i })).toBeNull(),
    );
    await waitFor(() => expect(mockVoice.syncRemoteCameraStreams).toHaveBeenLastCalledWith(42, []));
    status = screen.getByRole("status");
    expect(status).toHaveAttribute("aria-live", "polite");
    expect(status).toHaveTextContent("Bobby turned off camera in lobby.");
  });

  test("participant leave removes active cameras for that user", async () => {
    const camera = makeCamera({ display_name: "Bobby" });
    setup([makeParticipant()], [], [camera]);
    mockVoice.setActiveChannelId(42);
    renderVoice(<VoiceChannel channel={CHANNEL} />);

    await screen.findByRole("img", { name: /bob has camera on/i });
    await waitFor(() =>
      expect(mockVoice.syncRemoteCameraStreams).toHaveBeenLastCalledWith(42, [camera]),
    );

    leftListeners.forEach((cb) => cb({ channel_id: 42, user_id: 2 }));

    await waitFor(() => expect(screen.queryByText("bob")).toBeNull());
    await waitFor(() => expect(mockVoice.syncRemoteCameraStreams).toHaveBeenLastCalledWith(42, []));
  });

  test("lists multiple active screen shares compactly", async () => {
    setup(
      [],
      [
        makeScreenShare({ display_name: "Bobby" }),
        makeScreenShare({
          sharer_user_id: 3,
          username: "carol",
          participant_identity: "3",
          track_sid: "TR_carol_screen",
          started_at: 1_700_000_005,
        }),
      ],
    );

    renderVoice(<VoiceChannel channel={CHANNEL} />);

    const shelf = await screen.findByRole("region", { name: /active screen shares in lobby/i });
    expect(within(shelf).getByText("2 streams live")).toBeInTheDocument();
    expect(within(shelf).getByText("Bobby's screen")).toBeInTheDocument();
    expect(within(shelf).getByText("carol's screen")).toBeInTheDocument();
    expect(within(shelf).getAllByText("Join voice")).toHaveLength(2);
    expect(within(shelf).queryByRole("button", { name: /watch/i })).toBeNull();
  });

  test("marks only the currently watched screen share", async () => {
    const bob = makeScreenShare({ display_name: "Bobby" });
    const carol = makeScreenShare({
      sharer_user_id: 3,
      username: "carol",
      participant_identity: "3",
      track_sid: "TR_carol_screen",
      started_at: 1_700_000_005,
    });
    setup([], [bob, carol]);
    mockVoice.setActiveChannelId(42);
    mockVoice.setWatchingScreenShare(carol);

    renderVoice(<VoiceChannel channel={CHANNEL} />);

    const shelf = await screen.findByRole("region", { name: /active screen shares in lobby/i });
    const bobButton = within(shelf).getByRole("button", { name: /watch Bobby's screen share/i });
    const carolButton = within(shelf).getByRole("button", {
      name: /watching carol's screen share/i,
    });
    expect(bobButton).toHaveTextContent("Watch");
    expect(bobButton).toHaveAttribute("aria-pressed", "false");
    expect(bobButton).toBeEnabled();
    expect(carolButton).toHaveTextContent("Watching");
    expect(carolButton).toHaveAttribute("aria-pressed", "true");
    expect(carolButton).toBeDisabled();
  });

  test("applies screen-share SSE start and stop updates with polite announcements", async () => {
    const stream = makeScreenShare({ display_name: "Bobby" });
    setup([makeParticipant()]);
    renderVoice(<VoiceChannel channel={CHANNEL} />);

    await screen.findByText("bob");
    await waitFor(() => expect(screenShareStartedListeners.size).toBe(1));

    screenShareStartedListeners.forEach((cb) => cb(stream));

    await waitFor(() => expect(screen.getByText("Bobby's screen")).toBeInTheDocument());
    expect(screen.getByRole("img", { name: /bob is sharing screen/i })).toBeInTheDocument();
    mockVoice.setWatchingScreenShare(stream);
    let status = screen.getByRole("status");
    expect(status).toHaveAttribute("aria-live", "polite");
    expect(status).toHaveTextContent("Bobby started sharing screen in lobby.");

    screenShareStoppedListeners.forEach((cb) => cb(stoppedFrom(stream)));

    await waitFor(() => expect(screen.queryByText("Bobby's screen")).toBeNull());
    expect(screen.queryByRole("img", { name: /bob is sharing screen/i })).toBeNull();
    expect(mockVoice.stopWatchingScreenShare).toHaveBeenCalled();
    status = screen.getByRole("status");
    expect(status).toHaveAttribute("aria-live", "polite");
    expect(status).toHaveTextContent("Bobby stopped sharing screen in lobby.");
  });

  test("stale start events after a stop do not resurrect dead screen shares", async () => {
    const stream = makeScreenShare({ display_name: "Bobby" });
    setup([makeParticipant()]);
    renderVoice(<VoiceChannel channel={CHANNEL} />);

    await waitFor(() => expect(screenShareStartedListeners.size).toBe(1));

    screenShareStoppedListeners.forEach((cb) => cb(stoppedFrom(stream)));
    screenShareStartedListeners.forEach((cb) => cb(stream));
    await Promise.resolve();

    expect(screen.queryByText("Bobby's screen")).toBeNull();
    expect(screen.queryByRole("img", { name: /bob is sharing screen/i })).toBeNull();
  });

  test("stopped SSE closes a watched stream even when local stream state is already absent", async () => {
    const stream = makeScreenShare({ display_name: "Bobby" });
    setup();
    mockVoice.setActiveChannelId(42);
    mockVoice.setWatchingScreenShare(stream);
    renderVoice(<VoiceChannel channel={CHANNEL} />);

    await waitFor(() => expect(screenShareStoppedListeners.size).toBe(1));
    screenShareStoppedListeners.forEach((cb) => cb(stoppedFrom(stream)));

    await waitFor(() => expect(mockVoice.stopWatchingScreenShare).toHaveBeenCalled());
    expect(screen.queryByText("Bobby's screen")).toBeNull();
    expect(screen.getByRole("status")).toHaveTextContent("Bobby stopped sharing screen in lobby.");
  });

  test("participant left SSE closes a watched stream even when local stream state is already absent", async () => {
    const stream = makeScreenShare({ display_name: "Bobby" });
    setup([makeParticipant()]);
    mockVoice.setActiveChannelId(42);
    mockVoice.setWatchingScreenShare(stream);
    renderVoice(<VoiceChannel channel={CHANNEL} />);

    await screen.findByText("bob");
    leftListeners.forEach((cb) => cb({ channel_id: 42, user_id: 2 }));

    await waitFor(() => expect(mockVoice.stopWatchingScreenShare).toHaveBeenCalled());
    expect(screen.queryByText("bob")).toBeNull();
  });

  test("screen-share SSE updates fan out to multiple mounted channel views", async () => {
    const stream = makeScreenShare({ display_name: "Bobby" });
    setup([makeParticipant()]);
    renderVoice(
      <>
        <VoiceChannel channel={CHANNEL} />
        <VoiceChannel channel={CHANNEL} />
      </>,
    );

    await waitFor(() => expect(screenShareStartedListeners.size).toBe(2));
    screenShareStartedListeners.forEach((cb) => cb(stream));

    await waitFor(() => expect(screen.getAllByText("Bobby's screen")).toHaveLength(2));

    screenShareStoppedListeners.forEach((cb) => cb(stoppedFrom(stream)));
    await waitFor(() => expect(screen.queryByText("Bobby's screen")).toBeNull());
  });

  test("Watch controls require the active voice channel and are keyboard reachable", async () => {
    const user = userEvent.setup();
    const stream = makeScreenShare({ display_name: "Bobby" });
    setup([], [stream]);
    renderVoice(<VoiceChannel channel={CHANNEL} />);

    const channelButton = screen.getByRole("button", { name: /^join voice channel lobby$/i });
    await screen.findByText("Bobby's screen");
    expect(screen.queryByRole("button", { name: /watch Bobby's screen share/i })).toBeNull();

    mockVoice.setActiveChannelId(42);
    const watchButton = await screen.findByRole("button", {
      name: /watch Bobby's screen share/i,
    });
    expect(mockVoice.join).not.toHaveBeenCalled();
    expect(mockVoice.watchScreenShare).not.toHaveBeenCalled();

    await user.tab();
    expect(channelButton).toHaveFocus();
    await user.tab();
    expect(watchButton).toHaveFocus();
    await user.keyboard("{Enter}");

    await waitFor(() => expect(mockVoice.watchScreenShare).toHaveBeenCalledWith(stream));
    expect(mockVoice.join).not.toHaveBeenCalled();
  });

  test("camera button is keyboard reachable and exposes pressed and busy states", async () => {
    const user = userEvent.setup();
    setup();
    mockVoice.setActiveChannelId(42);
    renderVoice(<VoiceChannel channel={CHANNEL} />);

    const channelButton = screen.getByRole("button", { name: /^leave voice channel lobby$/i });
    const start = screen.getByRole("button", { name: /Turn on camera/ });
    expect(start).toHaveAttribute("aria-pressed", "false");
    expect(start).toHaveAttribute("aria-busy", "false");

    await user.tab();
    expect(channelButton).toHaveFocus();
    await user.tab();
    expect(start).toHaveFocus();

    fireEvent.click(start);
    expect(mockVoice.startCamera).toHaveBeenCalled();

    mockVoice.setCameraStatus("starting");
    const busy = screen.getByRole("button", { name: /Starting camera/ });
    expect(busy).toBeDisabled();
    expect(busy).toHaveAttribute("aria-busy", "true");

    mockVoice.setCameraStatus("on");
    mockVoice.setIsCameraEnabled(true);
    const stop = screen.getByRole("button", { name: /Turn off camera/ });
    expect(stop).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(stop);

    expect(mockVoice.stopCamera).toHaveBeenCalled();
    expect(screen.getByRole("status")).toHaveTextContent("Camera on");
  });

  test("a reconnect aborts a pending bootstrap and guarantees a fresh snapshot", async () => {
    setup();
    const requests: Array<{
      signal: AbortSignal;
      resolve: (response: Response) => void;
    }> = [];
    server.use(
      http.get(
        "*/voice/participants/42",
        ({ request }) =>
          new Promise<Response>((resolve) => {
            requests.push({ signal: request.signal, resolve });
          }),
      ),
    );

    renderVoice(<VoiceChannel channel={CHANNEL} />);
    await waitFor(() => expect(requests.some(({ signal }) => !signal.aborted)).toBe(true));
    const beforeReconnect = [...requests].reverse().find(({ signal }) => !signal.aborted);
    expect(beforeReconnect).toBeDefined();

    act(() => connectedListeners.forEach((listener) => listener()));
    await waitFor(() => {
      expect(beforeReconnect?.signal.aborted).toBe(true);
      expect(requests.filter(({ signal }) => !signal.aborted)).toHaveLength(1);
    });

    const fresh = [...requests].reverse().find(({ signal }) => !signal.aborted);
    fresh?.resolve(HttpResponse.json([makeParticipant({ user_id: 3, username: "carol" })]));

    expect(await screen.findByText("carol")).toBeInTheDocument();
    expect(screen.queryByText("bob")).toBeNull();
  });

  test("retains proven presence when every reconnect endpoint fails", async () => {
    const share = makeScreenShare({ display_name: "Bobby" });
    const camera = makeCamera({ display_name: "Bobby" });
    setup([makeParticipant()], [share], [camera]);
    renderVoice(<VoiceChannel channel={CHANNEL} />);

    await screen.findByText("Bobby's screen");
    await screen.findByRole("img", { name: /bob has camera on/i });
    let failedRequests = 0;
    const fail = () => {
      failedRequests += 1;
      return HttpResponse.json({ error: "offline" }, { status: 503 });
    };
    server.use(
      http.get("*/voice/participants/42", fail),
      http.get("*/voice/screen-shares", fail),
      http.get("*/voice/cameras", fail),
    );

    act(() => connectedListeners.forEach((listener) => listener()));
    await waitFor(() => expect(failedRequests).toBe(3));

    expect(screen.getByText("bob")).toBeInTheDocument();
    expect(screen.getByText("Bobby's screen")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: /bob has camera on/i })).toBeInTheDocument();
  });

  test("applies successful reconnect endpoints without clearing a failed endpoint", async () => {
    const oldShare = makeScreenShare({ display_name: "Bobby" });
    const oldCamera = makeCamera({ display_name: "Bobby" });
    setup([makeParticipant()], [oldShare], [oldCamera]);
    renderVoice(<VoiceChannel channel={CHANNEL} />);

    await screen.findByText("Bobby's screen");
    await screen.findByRole("img", { name: /bob has camera on/i });
    let completedRequests = 0;
    server.use(
      http.get("*/voice/participants/42", () => {
        completedRequests += 1;
        return HttpResponse.json({ error: "participants unavailable" }, { status: 503 });
      }),
      http.get("*/voice/screen-shares", () => {
        completedRequests += 1;
        return HttpResponse.json([]);
      }),
      http.get("*/voice/cameras", () => {
        completedRequests += 1;
        return HttpResponse.json([]);
      }),
    );

    act(() => connectedListeners.forEach((listener) => listener()));
    await waitFor(() => expect(completedRequests).toBe(3));
    await waitFor(() => expect(screen.queryByText("Bobby's screen")).toBeNull());

    expect(screen.getByText("bob")).toBeInTheDocument();
    expect(screen.queryByRole("img", { name: /bob has camera on/i })).toBeNull();
  });

  test("removes every presence listener and clears camera metadata on unmount", async () => {
    setup();
    const { unmount } = renderVoice(<VoiceChannel channel={CHANNEL} />);
    await waitFor(() => expect(joinedListeners.size).toBe(1));

    unmount();

    expect(joinedListeners.size).toBe(0);
    expect(leftListeners.size).toBe(0);
    expect(speakingListeners.size).toBe(0);
    expect(statusListeners.size).toBe(0);
    expect(screenShareStartedListeners.size).toBe(0);
    expect(screenShareStoppedListeners.size).toBe(0);
    expect(cameraStartedListeners.size).toBe(0);
    expect(cameraStoppedListeners.size).toBe(0);
    expect(connectedListeners.size).toBe(0);
    expect(mockVoice.syncRemoteCameraStreams).toHaveBeenLastCalledWith(42, []);
  });

  test("share buttons call the voice context", () => {
    setup();
    mockVoice.setActiveChannelId(42);
    renderVoice(<VoiceChannel channel={CHANNEL} />);

    fireEvent.click(screen.getByRole("button", { name: /Share screen/ }));
    expect(mockVoice.startScreenShare).toHaveBeenCalled();

    mockVoice.setScreenShareStatus("starting");
    const starting = screen.getByRole("button", { name: /Starting screen share/ });
    expect(starting).toBeDisabled();
    expect(starting).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("status")).toHaveTextContent("Starting screen share…");

    mockVoice.setScreenShareStatus("on");
    mockVoice.setIsScreenSharing(true);
    fireEvent.click(screen.getByRole("button", { name: /Stop sharing screen/ }));
    expect(mockVoice.stopScreenShare).toHaveBeenCalled();
    expect(screen.getByRole("status")).toHaveTextContent("Sharing screen");

    mockVoice.setScreenShareStatus("stopping");
    const stopping = screen.getByRole("button", { name: "Stopping screen share" });
    expect(stopping).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent("Stopping screen share…");
  });

  test("announces camera stopping from explicit media status", () => {
    setup();
    mockVoice.setActiveChannelId(42);
    mockVoice.setCameraStatus("stopping");
    renderVoice(<VoiceChannel channel={CHANNEL} />);

    const stopping = screen.getByRole("button", { name: "Stopping camera" });
    expect(stopping).toBeDisabled();
    expect(stopping).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("status")).toHaveTextContent("Stopping camera…");
  });
});
