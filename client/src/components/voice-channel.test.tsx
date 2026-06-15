import { describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
import { createSignal } from "solid-js";
import type {
  Channel,
  ScreenShareStopped,
  ScreenShareStream,
  VoiceParticipant,
  VoiceParticipantLeft,
  VoiceParticipantSpeaking,
  VoiceParticipantStatus,
} from "../api";
import { mswState, resetMswState } from "../test/msw/server";
import VoiceChannel from "./voice-channel";

type JoinedListener = (p: VoiceParticipant) => void;
type LeftListener = (p: VoiceParticipantLeft) => void;
type SpeakingListener = (p: VoiceParticipantSpeaking) => void;
type StatusListener = (p: VoiceParticipantStatus) => void;
type ScreenShareStartedListener = (p: ScreenShareStream) => void;
type ScreenShareStoppedListener = (p: ScreenShareStopped) => void;

interface MockVoiceChatApi {
  activeChannelId: () => number | null;
  setActiveChannelId: (id: number | null) => void;
  isConnecting: () => boolean;
  isMuted: () => boolean;
  setIsMuted: (v: boolean) => void;
  isDeafened: () => boolean;
  setIsDeafened: (v: boolean) => void;
  isScreenSharing: () => boolean;
  setIsScreenSharing: (v: boolean) => void;
  isScreenShareStarting: () => boolean;
  setIsScreenShareStarting: (v: boolean) => void;
  watchingScreenShare: () => ScreenShareStream | null;
  setWatchingScreenShare: (stream: ScreenShareStream | null) => void;
  watchingScreenShareTrack: () => null;
  lastError: () => string | null;
  speakingUserIds: () => ReadonlySet<number>;
  setSpeakingUserIds: (ids: ReadonlySet<number>) => void;
  join: ReturnType<typeof vi.fn>;
  leave: ReturnType<typeof vi.fn>;
  toggleMuted: ReturnType<typeof vi.fn>;
  toggleDeafened: ReturnType<typeof vi.fn>;
  startScreenShare: ReturnType<typeof vi.fn>;
  stopScreenShare: ReturnType<typeof vi.fn>;
  watchScreenShare: ReturnType<typeof vi.fn>;
  stopWatchingScreenShare: ReturnType<typeof vi.fn>;
}

let mockVoice: MockVoiceChatApi;
const joinedListeners = new Set<JoinedListener>();
const leftListeners = new Set<LeftListener>();
const speakingListeners = new Set<SpeakingListener>();
const statusListeners = new Set<StatusListener>();
const screenShareStartedListeners = new Set<ScreenShareStartedListener>();
const screenShareStoppedListeners = new Set<ScreenShareStoppedListener>();
const [showEverywhere, setShowEverywhere] = createSignal(false);

vi.mock("../contexts/voice-chat", () => ({
  useVoiceChat: () => mockVoice,
}));

vi.mock("../contexts/events", () => ({
  useEvents: () => ({
    onMessage: () => () => {},
    onMessageUpdated: () => () => {},
    onMessageDeleted: () => () => {},
    onChannelCreated: () => () => {},
    onChannelsReordered: () => () => {},
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
  }),
}));

vi.mock("../voice/settings", () => ({
  showSpeakingIndicatorsEverywhere: () => showEverywhere(),
}));

function makeVoiceMock(overrides?: Partial<MockVoiceChatApi>): MockVoiceChatApi {
  const [activeChannelId, setActiveChannelId] = createSignal<number | null>(null);
  const [isMuted, setIsMuted] = createSignal(false);
  const [isDeafened, setIsDeafened] = createSignal(false);
  const [isScreenSharing, setIsScreenSharing] = createSignal(false);
  const [isScreenShareStarting, setIsScreenShareStarting] = createSignal(false);
  const [watchingScreenShare, setWatchingScreenShare] = createSignal<ScreenShareStream | null>(
    null,
  );
  const [speakingUserIds, setSpeakingUserIds] = createSignal<ReadonlySet<number>>(new Set());
  return {
    activeChannelId,
    setActiveChannelId,
    isConnecting: () => false,
    isMuted,
    setIsMuted,
    isDeafened,
    setIsDeafened,
    isScreenSharing,
    setIsScreenSharing,
    isScreenShareStarting,
    setIsScreenShareStarting,
    watchingScreenShare,
    setWatchingScreenShare,
    watchingScreenShareTrack: () => null,
    lastError: () => null,
    speakingUserIds,
    setSpeakingUserIds,
    join: vi.fn<(id: number) => Promise<void>>().mockResolvedValue(),
    leave: vi.fn<() => Promise<void>>().mockResolvedValue(),
    toggleMuted: vi.fn<() => Promise<void>>().mockResolvedValue(),
    toggleDeafened: vi.fn<() => void>().mockImplementation(() => {}),
    startScreenShare: vi.fn<() => Promise<void>>().mockResolvedValue(),
    stopScreenShare: vi.fn<() => Promise<void>>().mockResolvedValue(),
    watchScreenShare: vi.fn<(stream: ScreenShareStream) => Promise<void>>().mockResolvedValue(),
    stopWatchingScreenShare: vi.fn<() => Promise<void>>().mockResolvedValue(),
    ...overrides,
  };
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

function stoppedFrom(stream: ScreenShareStream): ScreenShareStopped {
  return {
    channel_id: stream.channel_id,
    sharer_user_id: stream.sharer_user_id,
    participant_identity: stream.participant_identity,
    track_sid: stream.track_sid,
  };
}

function setup(initial: VoiceParticipant[] = [], screenShares: ScreenShareStream[] = []) {
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
  joinedListeners.clear();
  leftListeners.clear();
  speakingListeners.clear();
  statusListeners.clear();
  screenShareStartedListeners.clear();
  screenShareStoppedListeners.clear();
  setShowEverywhere(false);
  mockVoice = makeVoiceMock();
  return state;
}

describe("<VoiceChannel>", () => {
  test("fetches and renders the initial participant list", async () => {
    setup([makeParticipant(), makeParticipant({ user_id: 3, username: "carol" })]);

    render(() => <VoiceChannel channel={CHANNEL} />);

    await waitFor(() => {
      expect(screen.getByText("bob")).toBeInTheDocument();
      expect(screen.getByText("carol")).toBeInTheDocument();
    });
  });

  test("adds a participant when a join SSE event arrives", async () => {
    setup();
    render(() => <VoiceChannel channel={CHANNEL} />);

    // Wait for the initial empty-list fetch to complete before pushing events,
    // so the SSE-driven append isn't clobbered by the resource resolving.
    await waitFor(() => expect(mswState().voiceParticipants["42"]).toBeDefined());

    joinedListeners.forEach((cb) => cb(makeParticipant({ user_id: 7, username: "dave" })));

    await waitFor(() => expect(screen.getByText("dave")).toBeInTheDocument());
  });

  test("renders participant mute/deafen status from fetch and SSE", async () => {
    setup([makeParticipant({ muted: true })]);
    render(() => <VoiceChannel channel={CHANNEL} />);

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
    render(() => <VoiceChannel channel={CHANNEL} />);
    await waitFor(() => expect(mswState().voiceParticipants["42"]).toBeDefined());

    joinedListeners.forEach((cb) =>
      cb(makeParticipant({ user_id: 9, channel_id: 999, username: "ghost" })),
    );

    // Give the signal an event loop turn to propagate if it were going to.
    await Promise.resolve();
    expect(screen.queryByText("ghost")).toBeNull();
  });

  test("removes a participant on a left SSE event", async () => {
    setup([makeParticipant()]);
    render(() => <VoiceChannel channel={CHANNEL} />);

    await waitFor(() => expect(screen.getByText("bob")).toBeInTheDocument());

    leftListeners.forEach((cb) => cb({ channel_id: 42, user_id: 2 }));
    await waitFor(() => expect(screen.queryByText("bob")).toBeNull());
  });

  test("clicking the row when disconnected calls join(channelId)", async () => {
    setup();
    render(() => <VoiceChannel channel={CHANNEL} />);

    fireEvent.click(screen.getByRole("button", { name: /Join voice channel lobby/ }));

    await waitFor(() => expect(mockVoice.join).toHaveBeenCalledWith(42));
    expect(mockVoice.leave).not.toHaveBeenCalled();
  });

  test("clicking the row when active calls leave()", async () => {
    setup();
    render(() => <VoiceChannel channel={CHANNEL} />);
    mockVoice.setActiveChannelId(42);

    fireEvent.click(screen.getByRole("button", { name: /Leave voice channel lobby/ }));

    await waitFor(() => expect(mockVoice.leave).toHaveBeenCalled());
    expect(mockVoice.join).not.toHaveBeenCalled();
  });

  test("shows call controls only when connected to this channel", () => {
    setup();
    const { unmount } = render(() => <VoiceChannel channel={CHANNEL} />);
    expect(screen.queryByRole("button", { name: /Share screen/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /Disconnect from voice/ })).toBeNull();

    unmount();
    mockVoice = makeVoiceMock();
    mockVoice.setActiveChannelId(42);
    render(() => <VoiceChannel channel={CHANNEL} />);

    expect(screen.getByRole("button", { name: /Share screen/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Disconnect from voice/ })).toBeInTheDocument();
  });

  test("renders a speaking ring on the avatar for in-channel speakers", async () => {
    setup([makeParticipant()]);
    mockVoice.setActiveChannelId(42);
    render(() => <VoiceChannel channel={CHANNEL} />);

    await waitFor(() => expect(screen.getByText("bob")).toBeInTheDocument());

    const avatar = screen.getByRole("img", { name: /bob's avatar/i });
    expect(avatar.className).not.toMatch(/ring-green-500/);

    mockVoice.setSpeakingUserIds(new Set([2]));
    await waitFor(() => expect(avatar.className).toMatch(/ring-green-500/));
  });

  test("does not show ring from SSE speaking events when not connected and setting off", async () => {
    setup([makeParticipant()]);
    // Not active — we're not connected to this channel.
    render(() => <VoiceChannel channel={CHANNEL} />);
    await waitFor(() => expect(screen.getByText("bob")).toBeInTheDocument());

    speakingListeners.forEach((cb) => cb({ channel_id: 42, user_id: 2, speaking: true }));
    await Promise.resolve();

    const avatar = screen.getByRole("img", { name: /bob's avatar/i });
    expect(avatar.className).not.toMatch(/ring-green-500/);
  });

  test("shows ring from SSE speaking events when setting is on and not connected", async () => {
    setup([makeParticipant()]);
    setShowEverywhere(true);
    render(() => <VoiceChannel channel={CHANNEL} />);
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

    render(() => <VoiceChannel channel={CHANNEL} />);

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

    render(() => <VoiceChannel channel={CHANNEL} />);

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

    render(() => <VoiceChannel channel={CHANNEL} />);

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
    render(() => <VoiceChannel channel={CHANNEL} />);

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
    render(() => <VoiceChannel channel={CHANNEL} />);

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
    render(() => <VoiceChannel channel={CHANNEL} />);

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
    render(() => <VoiceChannel channel={CHANNEL} />);

    await screen.findByText("bob");
    leftListeners.forEach((cb) => cb({ channel_id: 42, user_id: 2 }));

    await waitFor(() => expect(mockVoice.stopWatchingScreenShare).toHaveBeenCalled());
    expect(screen.queryByText("bob")).toBeNull();
  });

  test("screen-share SSE updates fan out to multiple mounted channel views", async () => {
    const stream = makeScreenShare({ display_name: "Bobby" });
    setup([makeParticipant()]);
    render(() => (
      <>
        <VoiceChannel channel={CHANNEL} />
        <VoiceChannel channel={CHANNEL} />
      </>
    ));

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
    render(() => <VoiceChannel channel={CHANNEL} />);

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

  test("share/disconnect buttons call the voice context", () => {
    setup();
    mockVoice.setActiveChannelId(42);
    render(() => <VoiceChannel channel={CHANNEL} />);

    fireEvent.click(screen.getByRole("button", { name: /Share screen/ }));
    expect(mockVoice.startScreenShare).toHaveBeenCalled();

    mockVoice.setIsScreenSharing(true);
    fireEvent.click(screen.getByRole("button", { name: /Stop sharing screen/ }));
    expect(mockVoice.stopScreenShare).toHaveBeenCalled();
    expect(screen.getByRole("status")).toHaveTextContent("Sharing screen");

    fireEvent.click(screen.getByRole("button", { name: /Disconnect from voice/ }));
    expect(mockVoice.leave).toHaveBeenCalled();
  });
});
