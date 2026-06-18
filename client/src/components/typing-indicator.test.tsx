import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@solidjs/testing-library";
import TypingIndicator, { formatTypingMessage } from "./typing-indicator";
import { TYPING_EXPIRY_MS } from "../constants";
import type { EventsContextValue } from "../contexts/events";
import type { Message, UserTyping } from "../api";
import { expectNoA11yViolations } from "../test/a11y";

function fakeEvents(): {
  events: EventsContextValue;
  emit: (t: UserTyping) => void;
  emitMessage: (m: Message) => void;
} {
  let emit: (t: UserTyping) => void = () => {};
  let emitMessage: (m: Message) => void = () => {};
  const events: EventsContextValue = {
    onMessage: (cb) => {
      emitMessage = cb;
      return () => {};
    },
    onMessageUpdated: () => () => {},
    onMessageDeleted: () => () => {},
    onMessageEmbedsUpdated: () => () => {},
    onMessageReactionsUpdated: () => () => {},
    onChannelCreated: () => () => {},
    onChannelsReordered: () => () => {},
    onEmojiCreated: () => () => {},
    onEmojiUpdated: () => () => {},
    onEmojiDeleted: () => () => {},
    onVoiceParticipantJoined: () => () => {},
    onVoiceParticipantLeft: () => () => {},
    onVoiceParticipantSpeakingChanged: () => () => {},
    onVoiceParticipantStatusChanged: () => () => {},
    onScreenShareStarted: () => () => {},
    onScreenShareStopped: () => () => {},
    onCameraVideoStarted: () => () => {},
    onCameraVideoStopped: () => () => {},
    onUserTyping: (cb) => {
      emit = cb;
      return () => {};
    },
    onThreadReplyCreated: () => () => {},
    onThreadReplyDeleted: () => () => {},
  };
  return { events, emit: (t) => emit(t), emitMessage: (m) => emitMessage(m) };
}

function fakeMessage(
  overrides: Partial<Message> & Pick<Message, "user_id" | "channel_id">,
): Message {
  return {
    id: 1,
    text: "hi",
    username: "someone",
    display_name: null,
    avatar_url: null,
    suppress_embeds: false,
    mentions: [],
    attachments: [],
    embeds: [],
    ...overrides,
  };
}

describe("formatTypingMessage", () => {
  test("returns null when no one is typing", () => {
    expect(formatTypingMessage([])).toBeNull();
  });

  test("lists a single user", () => {
    expect(formatTypingMessage(["alice"])).toBe("alice is typing…");
  });

  test("lists two users joined by 'and'", () => {
    expect(formatTypingMessage(["alice", "bob"])).toBe("alice and bob are typing…");
  });

  test("lists three users with a serial 'and'", () => {
    expect(formatTypingMessage(["alice", "bob", "carol"])).toBe(
      "alice, bob, and carol are typing…",
    );
  });

  test("collapses to 'Several people' at four users", () => {
    expect(formatTypingMessage(["alice", "bob", "carol", "dave"])).toBe(
      "Several people are typing…",
    );
  });

  test("collapses to 'Several people' at many users", () => {
    expect(formatTypingMessage(["a", "b", "c", "d", "e"])).toBe("Several people are typing…");
  });
});

describe("<TypingIndicator>", () => {
  test("renders nothing when no one is typing", () => {
    const { events } = fakeEvents();
    render(() => <TypingIndicator channelId={100} currentUserId={1} events={events} />);
    expect(screen.queryByTestId("typing-indicator")).toBeNull();
  });

  test("shows one typer", async () => {
    const { events, emit } = fakeEvents();
    render(() => <TypingIndicator channelId={100} currentUserId={1} events={events} />);
    emit({ channel_id: 100, user_id: 2, username: "bob" });
    expect(await screen.findByText(/bob is typing/i)).toBeInTheDocument();
  });

  test("ignores typing pings from the current user", async () => {
    const { events, emit } = fakeEvents();
    render(() => <TypingIndicator channelId={100} currentUserId={1} events={events} />);
    emit({ channel_id: 100, user_id: 1, username: "me" });
    await new Promise((r) => setTimeout(r, 10));
    expect(screen.queryByTestId("typing-indicator")).toBeNull();
  });

  test("ignores typing pings for other channels", async () => {
    const { events, emit } = fakeEvents();
    render(() => <TypingIndicator channelId={100} currentUserId={1} events={events} />);
    emit({ channel_id: 999, user_id: 2, username: "bob" });
    await new Promise((r) => setTimeout(r, 10));
    expect(screen.queryByTestId("typing-indicator")).toBeNull();
  });

  test("expires entries after TYPING_EXPIRY_MS", async () => {
    vi.useFakeTimers();
    try {
      let fakeTime = 1_000_000;
      const now = () => fakeTime;
      const { events, emit } = fakeEvents();
      render(() => <TypingIndicator channelId={100} currentUserId={1} events={events} now={now} />);

      emit({ channel_id: 100, user_id: 2, username: "bob" });
      expect(screen.getByText(/bob is typing/i)).toBeInTheDocument();

      fakeTime += TYPING_EXPIRY_MS + 100;
      // Let the sweep interval fire.
      await vi.advanceTimersByTimeAsync(1000);
      expect(screen.queryByTestId("typing-indicator")).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  test("a fresh ping refreshes a user's lastSeen", async () => {
    vi.useFakeTimers();
    try {
      let fakeTime = 1_000_000;
      const now = () => fakeTime;
      const { events, emit } = fakeEvents();
      render(() => <TypingIndicator channelId={100} currentUserId={1} events={events} now={now} />);

      emit({ channel_id: 100, user_id: 2, username: "bob" });
      fakeTime += TYPING_EXPIRY_MS - 500;
      emit({ channel_id: 100, user_id: 2, username: "bob" });
      fakeTime += 500; // total is TYPING_EXPIRY_MS since the first ping, but only 500ms since the refresh.
      await vi.advanceTimersByTimeAsync(1000);
      expect(screen.getByText(/bob is typing/i)).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  test("clears a user's entry when they send a message in the channel", async () => {
    const { events, emit, emitMessage } = fakeEvents();
    render(() => <TypingIndicator channelId={100} currentUserId={1} events={events} />);
    emit({ channel_id: 100, user_id: 2, username: "bob" });
    expect(await screen.findByText(/bob is typing/i)).toBeInTheDocument();
    emitMessage(fakeMessage({ user_id: 2, channel_id: 100, username: "bob" }));
    expect(screen.queryByTestId("typing-indicator")).toBeNull();
  });

  test("ignores messages from other channels when clearing", async () => {
    const { events, emit, emitMessage } = fakeEvents();
    render(() => <TypingIndicator channelId={100} currentUserId={1} events={events} />);
    emit({ channel_id: 100, user_id: 2, username: "bob" });
    expect(await screen.findByText(/bob is typing/i)).toBeInTheDocument();
    emitMessage(fakeMessage({ user_id: 2, channel_id: 999, username: "bob" }));
    expect(screen.getByText(/bob is typing/i)).toBeInTheDocument();
  });

  test("only clears the user who sent the message", async () => {
    const { events, emit, emitMessage } = fakeEvents();
    render(() => <TypingIndicator channelId={100} currentUserId={1} events={events} />);
    emit({ channel_id: 100, user_id: 2, username: "bob" });
    emit({ channel_id: 100, user_id: 3, username: "carol" });
    expect(await screen.findByText(/bob and carol are typing/i)).toBeInTheDocument();
    emitMessage(fakeMessage({ user_id: 2, channel_id: 100, username: "bob" }));
    expect(await screen.findByText(/carol is typing/i)).toBeInTheDocument();
  });

  test("four typers collapses to 'Several people'", async () => {
    const { events, emit } = fakeEvents();
    render(() => <TypingIndicator channelId={100} currentUserId={1} events={events} />);
    emit({ channel_id: 100, user_id: 2, username: "bob" });
    emit({ channel_id: 100, user_id: 3, username: "carol" });
    emit({ channel_id: 100, user_id: 4, username: "dave" });
    emit({ channel_id: 100, user_id: 5, username: "eve" });
    expect(await screen.findByText(/several people are typing/i)).toBeInTheDocument();
  });

  test("passes axe accessibility checks when someone is typing", async () => {
    const { events, emit } = fakeEvents();
    const { container } = render(() => (
      <TypingIndicator channelId={100} currentUserId={1} events={events} />
    ));
    emit({ channel_id: 100, user_id: 2, username: "bob" });
    await screen.findByText(/bob is typing/i);
    await expectNoA11yViolations(container, "typing indicator");
  });

  test("unsubscribes from the events context on cleanup", () => {
    const unsub = vi.fn();
    const events: EventsContextValue = {
      onMessage: () => () => {},
      onMessageUpdated: () => () => {},
      onMessageDeleted: () => () => {},
      onMessageEmbedsUpdated: () => () => {},
      onMessageReactionsUpdated: () => () => {},
      onChannelCreated: () => () => {},
      onChannelsReordered: () => () => {},
      onEmojiCreated: () => () => {},
      onEmojiUpdated: () => () => {},
      onEmojiDeleted: () => () => {},
      onVoiceParticipantJoined: () => () => {},
      onVoiceParticipantLeft: () => () => {},
      onVoiceParticipantSpeakingChanged: () => () => {},
      onVoiceParticipantStatusChanged: () => () => {},
      onScreenShareStarted: () => () => {},
      onScreenShareStopped: () => () => {},
      onCameraVideoStarted: () => () => {},
      onCameraVideoStopped: () => () => {},
      onUserTyping: () => unsub,
      onThreadReplyCreated: () => () => {},
      onThreadReplyDeleted: () => () => {},
    };
    const { unmount } = render(() => (
      <TypingIndicator channelId={100} currentUserId={1} events={events} />
    ));
    unmount();
    expect(unsub).toHaveBeenCalled();
  });
});
