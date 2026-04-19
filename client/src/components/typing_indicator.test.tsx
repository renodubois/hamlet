import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@solidjs/testing-library";
import TypingIndicator, { TYPING_EXPIRY_MS, formatTypingMessage } from "./typing_indicator";
import type { EventsContextValue } from "../events_context";
import type { UserTyping } from "../api";
import { expectNoA11yViolations } from "../test/a11y";

function fakeEvents(): {
  events: EventsContextValue;
  emit: (t: UserTyping) => void;
} {
  let emit: (t: UserTyping) => void = () => {};
  const events: EventsContextValue = {
    onMessage: () => () => {},
    onMessageUpdated: () => () => {},
    onMessageDeleted: () => () => {},
    onChannelCreated: () => () => {},
    onChannelsReordered: () => () => {},
    onVoiceParticipantJoined: () => () => {},
    onVoiceParticipantLeft: () => () => {},
    onVoiceParticipantSpeakingChanged: () => () => {},
    onUserTyping: (cb) => {
      emit = cb;
      return () => {};
    },
  };
  return { events, emit: (t) => emit(t) };
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
      onChannelCreated: () => () => {},
      onChannelsReordered: () => () => {},
      onVoiceParticipantJoined: () => () => {},
      onVoiceParticipantLeft: () => () => {},
      onVoiceParticipantSpeakingChanged: () => () => {},
      onUserTyping: () => unsub,
    };
    const { unmount } = render(() => (
      <TypingIndicator channelId={100} currentUserId={1} events={events} />
    ));
    unmount();
    expect(unsub).toHaveBeenCalled();
  });
});
