import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, describe, expect, test, vi } from "vitest";
import type { ReadStateSummary, User } from "../api";

const authUser = vi.hoisted(() => vi.fn<() => User | null | undefined>());
const listReadStatesMock = vi.hoisted(() => vi.fn<() => Promise<ReadStateSummary[]>>());
const markChannelReadMock = vi.hoisted(() =>
  vi.fn<(channelId: number, lastVisibleMessageId: number) => Promise<ReadStateSummary>>(),
);

vi.mock("./auth", () => ({
  useAuth: () => ({ user: authUser }),
}));

vi.mock("../api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../api")>();
  return {
    ...actual,
    listReadStates: listReadStatesMock,
    markChannelRead: markChannelReadMock,
  };
});

import { EventsProvider } from "./events";
import { ReadStatesProvider, useReadStates } from "./read-states";
import { FakeEventSource, latestFakeEventSource, resetFakeEventSources } from "../test/msw/sse";

const USER: User = {
  id: 1,
  username: "alice",
  display_name: null,
  email: null,
  email_verified: false,
  avatar_url: null,
};

function Probe() {
  const readStates = useReadStates();
  return (
    <div>
      <p>unread {readStates.hasUnread(10) ? "yes" : "no"}</p>
      <p>mentions {readStates.mentionCount(10)}</p>
      <p>missing {readStates.hasUnread(99) ? "yes" : "no"}</p>
      <button type="button" onClick={() => void readStates.markRead(10, 20)}>
        mark read
      </button>
    </div>
  );
}

afterEach(() => {
  vi.clearAllMocks();
  vi.unstubAllGlobals();
  resetFakeEventSources();
});

describe("ReadStatesProvider", () => {
  test("loads a snapshot for an authenticated user and exposes selectors", async () => {
    authUser.mockReturnValue(USER);
    listReadStatesMock.mockResolvedValue([
      {
        channel_id: 10,
        has_unread: true,
        mention_count: 3,
        last_read_created_at: 100,
        last_read_message_id: 20,
        updated_at: 200,
      },
    ]);

    render(() => (
      <ReadStatesProvider>
        <Probe />
      </ReadStatesProvider>
    ));

    await waitFor(() => expect(screen.getByText("unread yes")).toBeInTheDocument());
    expect(screen.getByText("mentions 3")).toBeInTheDocument();
    expect(screen.getByText("missing no")).toBeInTheDocument();
    expect(listReadStatesMock).toHaveBeenCalledTimes(1);
  });

  test("markRead applies the returned summary and swallows failures", async () => {
    authUser.mockReturnValue(USER);
    listReadStatesMock.mockResolvedValue([
      {
        channel_id: 10,
        has_unread: true,
        mention_count: 3,
        last_read_created_at: 100,
        last_read_message_id: 20,
        updated_at: 200,
      },
    ]);
    markChannelReadMock.mockResolvedValue({
      channel_id: 10,
      has_unread: false,
      mention_count: 0,
      last_read_created_at: 200,
      last_read_message_id: 30,
      updated_at: 300,
    });

    render(() => (
      <ReadStatesProvider>
        <Probe />
      </ReadStatesProvider>
    ));

    await waitFor(() => expect(screen.getByText("unread yes")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: /mark read/i }));

    await waitFor(() => expect(screen.getByText("unread no")).toBeInTheDocument());
    expect(screen.getByText("mentions 0")).toBeInTheDocument();
    expect(markChannelReadMock).toHaveBeenCalledWith(10, 20);

    markChannelReadMock.mockRejectedValueOnce(new Error("network"));
    fireEvent.click(screen.getByRole("button", { name: /mark read/i }));
    await waitFor(() => expect(markChannelReadMock).toHaveBeenCalledTimes(2));
  });

  test("applies read-state SSE updates and refetches on stream and focus recovery", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    authUser.mockReturnValue(USER);
    listReadStatesMock.mockResolvedValue([
      {
        channel_id: 10,
        has_unread: true,
        mention_count: 1,
        last_read_created_at: 100,
        last_read_message_id: 20,
        updated_at: 200,
      },
    ]);

    render(() => (
      <EventsProvider>
        <ReadStatesProvider>
          <Probe />
        </ReadStatesProvider>
      </EventsProvider>
    ));

    await waitFor(() => expect(screen.getByText("unread yes")).toBeInTheDocument());
    latestFakeEventSource()?.pushReadStateUpdated({
      channel_id: 10,
      has_unread: false,
      mention_count: 0,
      last_read_created_at: 200,
      last_read_message_id: 30,
      updated_at: 300,
    });

    await waitFor(() => expect(screen.getByText("unread no")).toBeInTheDocument());
    latestFakeEventSource()?.pushConnected();
    await waitFor(() => expect(listReadStatesMock).toHaveBeenCalledTimes(2));

    window.dispatchEvent(new Event("focus"));
    await waitFor(() => expect(listReadStatesMock).toHaveBeenCalledTimes(3));
  });

  test("applies incoming top-level message events across fake SSE clients", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    authUser.mockReturnValue(USER);
    listReadStatesMock.mockResolvedValue([
      {
        channel_id: 10,
        has_unread: false,
        mention_count: 0,
        last_read_created_at: 100,
        last_read_message_id: 20,
        updated_at: 200,
      },
    ]);

    render(() => (
      <div>
        <EventsProvider>
          <ReadStatesProvider>
            <section aria-label="client one">
              <Probe />
            </section>
          </ReadStatesProvider>
        </EventsProvider>
        <EventsProvider>
          <ReadStatesProvider>
            <section aria-label="client two">
              <Probe />
            </section>
          </ReadStatesProvider>
        </EventsProvider>
      </div>
    ));

    await waitFor(() => expect(listReadStatesMock).toHaveBeenCalledTimes(2));
    const incoming = {
      id: 21,
      user_id: 2,
      channel_id: 10,
      parent_id: null,
      created_at: 101,
      text: "ping",
      username: "bob",
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [{ id: 1, username: "alice", display_name: null, avatar_url: null }],
      attachments: [],
      embeds: [],
    };
    FakeEventSource.instances[0]?.pushMessage(incoming);
    FakeEventSource.instances[1]?.pushMessage(incoming);

    await waitFor(() => {
      const clientOne = screen.getByLabelText("client one");
      const clientTwo = screen.getByLabelText("client two");
      expect(within(clientOne).getByText("unread yes")).toBeInTheDocument();
      expect(within(clientOne).getByText("mentions 1")).toBeInTheDocument();
      expect(within(clientTwo).getByText("unread yes")).toBeInTheDocument();
      expect(within(clientTwo).getByText("mentions 1")).toBeInTheDocument();
    });
  });

  test("refetches snapshots after message lifecycle events can affect badges", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    authUser.mockReturnValue(USER);
    listReadStatesMock
      .mockResolvedValueOnce([
        {
          channel_id: 10,
          has_unread: true,
          mention_count: 1,
          last_read_created_at: 100,
          last_read_message_id: 20,
          updated_at: 200,
        },
      ])
      .mockResolvedValue([
        {
          channel_id: 10,
          has_unread: false,
          mention_count: 0,
          last_read_created_at: 100,
          last_read_message_id: 20,
          updated_at: 300,
        },
      ]);

    render(() => (
      <EventsProvider>
        <ReadStatesProvider>
          <Probe />
        </ReadStatesProvider>
      </EventsProvider>
    ));

    await waitFor(() => expect(screen.getByText("unread yes")).toBeInTheDocument());
    latestFakeEventSource()?.pushMessageUpdated({
      id: 20,
      user_id: 2,
      channel_id: 10,
      parent_id: null,
      text: "edited away",
      username: "bob",
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [],
      attachments: [],
      embeds: [],
    });

    await waitFor(() => expect(screen.getByText("unread no")).toBeInTheDocument());
    expect(screen.getByText("mentions 0")).toBeInTheDocument();

    latestFakeEventSource()?.pushMessageDeleted({ id: 20, channel_id: 10 });
    await waitFor(() => expect(listReadStatesMock).toHaveBeenCalledTimes(3));
  });

  test("clears local state when unauthenticated", async () => {
    authUser.mockReturnValue(null);

    render(() => (
      <ReadStatesProvider>
        <Probe />
      </ReadStatesProvider>
    ));

    expect(screen.getByText("unread no")).toBeInTheDocument();
    expect(screen.getByText("mentions 0")).toBeInTheDocument();
    expect(listReadStatesMock).not.toHaveBeenCalled();
  });
});
