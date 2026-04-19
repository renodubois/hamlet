import { describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import type {
  Channel,
  VoiceParticipant,
  VoiceParticipantLeft,
  VoiceParticipantSpeaking,
} from "../api";
import { mswState, resetMswState } from "../test/msw/server";
import VoiceChannel from "./voice_channel";

type JoinedListener = (p: VoiceParticipant) => void;
type LeftListener = (p: VoiceParticipantLeft) => void;
type SpeakingListener = (p: VoiceParticipantSpeaking) => void;

interface MockVoiceChatApi {
  activeChannelId: () => number | null;
  setActiveChannelId: (id: number | null) => void;
  isConnecting: () => boolean;
  isMuted: () => boolean;
  setIsMuted: (v: boolean) => void;
  isDeafened: () => boolean;
  setIsDeafened: (v: boolean) => void;
  lastError: () => string | null;
  speakingUserIds: () => ReadonlySet<number>;
  setSpeakingUserIds: (ids: ReadonlySet<number>) => void;
  join: ReturnType<typeof vi.fn>;
  leave: ReturnType<typeof vi.fn>;
  toggleMuted: ReturnType<typeof vi.fn>;
  toggleDeafened: ReturnType<typeof vi.fn>;
}

let mockVoice: MockVoiceChatApi;
const joinedListeners = new Set<JoinedListener>();
const leftListeners = new Set<LeftListener>();
const speakingListeners = new Set<SpeakingListener>();
const [showEverywhere, setShowEverywhere] = createSignal(false);

vi.mock("../voice_chat_context", () => ({
  useVoiceChat: () => mockVoice,
}));

vi.mock("../events_context", () => ({
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
  }),
}));

vi.mock("./voice_settings", () => ({
  showSpeakingIndicatorsEverywhere: () => showEverywhere(),
}));

function makeVoiceMock(overrides?: Partial<MockVoiceChatApi>): MockVoiceChatApi {
  const [activeChannelId, setActiveChannelId] = createSignal<number | null>(null);
  const [isMuted, setIsMuted] = createSignal(false);
  const [isDeafened, setIsDeafened] = createSignal(false);
  const [speakingUserIds, setSpeakingUserIds] = createSignal<ReadonlySet<number>>(new Set());
  return {
    activeChannelId,
    setActiveChannelId,
    isConnecting: () => false,
    isMuted,
    setIsMuted,
    isDeafened,
    setIsDeafened,
    lastError: () => null,
    speakingUserIds,
    setSpeakingUserIds,
    join: vi.fn<(id: number) => Promise<void>>().mockResolvedValue(),
    leave: vi.fn<() => Promise<void>>().mockResolvedValue(),
    toggleMuted: vi.fn<() => Promise<void>>().mockResolvedValue(),
    toggleDeafened: vi.fn<() => void>().mockImplementation(() => {}),
    ...overrides,
  };
}

const CHANNEL: Channel = { id: 42, name: "lobby", position: 0, type: "voice" };

function setup(initial: VoiceParticipant[] = []) {
  const state = resetMswState();
  state.me = {
    id: 1,
    username: "alice",
    email: null,
    email_verified: false,
    avatar_url: null,
  };
  state.voiceParticipants[String(CHANNEL.id)] = initial;
  joinedListeners.clear();
  leftListeners.clear();
  speakingListeners.clear();
  setShowEverywhere(false);
  mockVoice = makeVoiceMock();
  return state;
}

describe("<VoiceChannel>", () => {
  test("fetches and renders the initial participant list", async () => {
    setup([
      { user_id: 2, channel_id: 42, username: "bob", avatar_url: null },
      { user_id: 3, channel_id: 42, username: "carol", avatar_url: null },
    ]);

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

    joinedListeners.forEach((cb) =>
      cb({ user_id: 7, channel_id: 42, username: "dave", avatar_url: null }),
    );

    await waitFor(() => expect(screen.getByText("dave")).toBeInTheDocument());
  });

  test("ignores SSE events for other channels", async () => {
    setup();
    render(() => <VoiceChannel channel={CHANNEL} />);
    await waitFor(() => expect(mswState().voiceParticipants["42"]).toBeDefined());

    joinedListeners.forEach((cb) =>
      cb({ user_id: 9, channel_id: 999, username: "ghost", avatar_url: null }),
    );

    // Give the signal an event loop turn to propagate if it were going to.
    await Promise.resolve();
    expect(screen.queryByText("ghost")).toBeNull();
  });

  test("removes a participant on a left SSE event", async () => {
    setup([{ user_id: 2, channel_id: 42, username: "bob", avatar_url: null }]);
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

  test("shows mute/deafen/disconnect controls only when connected to this channel", () => {
    setup();
    const { unmount } = render(() => <VoiceChannel channel={CHANNEL} />);
    expect(screen.queryByRole("button", { name: /Mute microphone/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /Disconnect from voice/ })).toBeNull();

    unmount();
    mockVoice = makeVoiceMock();
    mockVoice.setActiveChannelId(42);
    render(() => <VoiceChannel channel={CHANNEL} />);

    expect(screen.getByRole("button", { name: /Mute microphone/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Deafen/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Disconnect from voice/ })).toBeInTheDocument();
  });

  test("renders a speaking ring on the avatar for in-channel speakers", async () => {
    setup([{ user_id: 2, channel_id: 42, username: "bob", avatar_url: null }]);
    mockVoice.setActiveChannelId(42);
    render(() => <VoiceChannel channel={CHANNEL} />);

    await waitFor(() => expect(screen.getByText("bob")).toBeInTheDocument());

    const avatar = screen.getByRole("img", { name: /bob's avatar/i });
    expect(avatar.className).not.toMatch(/ring-green-500/);

    mockVoice.setSpeakingUserIds(new Set([2]));
    await waitFor(() => expect(avatar.className).toMatch(/ring-green-500/));
  });

  test("does not show ring from SSE speaking events when not connected and setting off", async () => {
    setup([{ user_id: 2, channel_id: 42, username: "bob", avatar_url: null }]);
    // Not active — we're not connected to this channel.
    render(() => <VoiceChannel channel={CHANNEL} />);
    await waitFor(() => expect(screen.getByText("bob")).toBeInTheDocument());

    speakingListeners.forEach((cb) => cb({ channel_id: 42, user_id: 2, speaking: true }));
    await Promise.resolve();

    const avatar = screen.getByRole("img", { name: /bob's avatar/i });
    expect(avatar.className).not.toMatch(/ring-green-500/);
  });

  test("shows ring from SSE speaking events when setting is on and not connected", async () => {
    setup([{ user_id: 2, channel_id: 42, username: "bob", avatar_url: null }]);
    setShowEverywhere(true);
    render(() => <VoiceChannel channel={CHANNEL} />);
    await waitFor(() => expect(screen.getByText("bob")).toBeInTheDocument());

    speakingListeners.forEach((cb) => cb({ channel_id: 42, user_id: 2, speaking: true }));

    const avatar = screen.getByRole("img", { name: /bob's avatar/i });
    await waitFor(() => expect(avatar.className).toMatch(/ring-green-500/));

    speakingListeners.forEach((cb) => cb({ channel_id: 42, user_id: 2, speaking: false }));
    await waitFor(() => expect(avatar.className).not.toMatch(/ring-green-500/));
  });

  test("mute/deafen/disconnect buttons call the voice context", () => {
    setup();
    mockVoice.setActiveChannelId(42);
    render(() => <VoiceChannel channel={CHANNEL} />);

    fireEvent.click(screen.getByRole("button", { name: /Mute microphone/ }));
    expect(mockVoice.toggleMuted).toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: /Deafen/ }));
    expect(mockVoice.toggleDeafened).toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: /Disconnect from voice/ }));
    expect(mockVoice.leave).toHaveBeenCalled();
  });
});
