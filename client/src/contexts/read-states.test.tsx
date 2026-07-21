import { StrictMode, useRef, useState } from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import type { ReadStateSummary, User } from "../api";

const authState = vi.hoisted(() => ({
  user: null as User | null,
  status: "anonymous" as "loading" | "authenticated" | "anonymous",
}));
const listReadStatesMock = vi.hoisted(() =>
  vi.fn<(signal?: AbortSignal) => Promise<ReadStateSummary[]>>(),
);
const markChannelReadMock = vi.hoisted(() =>
  vi.fn<(channelId: number, lastVisibleMessageId: number) => Promise<ReadStateSummary>>(),
);

vi.mock("./auth", () => ({
  useAuth: () => ({ ...authState, error: null }),
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
import { captureExpectedConsoleDiagnostics } from "../test/setup";

const USER: User = {
  id: 1,
  username: "alice",
  display_name: null,
  email: null,
  email_verified: false,
  avatar_url: null,
};
const OTHER_USER: User = { ...USER, id: 2, username: "bob" };

function summary(overrides: Partial<ReadStateSummary> = {}): ReadStateSummary {
  return {
    channel_id: 10,
    has_unread: true,
    mention_count: 3,
    last_read_created_at: 100,
    last_read_message_id: 20,
    updated_at: 200,
    ...overrides,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function Probe() {
  const readStates = useReadStates();
  const initialActions = useRef({
    readState: readStates.readState,
    hasUnread: readStates.hasUnread,
    mentionCount: readStates.mentionCount,
    refresh: readStates.refresh,
  });
  const [markResult, setMarkResult] = useState("none");
  const stable =
    initialActions.current.readState === readStates.readState &&
    initialActions.current.hasUnread === readStates.hasUnread &&
    initialActions.current.mentionCount === readStates.mentionCount &&
    initialActions.current.refresh === readStates.refresh;
  return (
    <div>
      <p>status {readStates.status}</p>
      <p>error {readStates.error ? "yes" : "no"}</p>
      <p>unread {readStates.hasUnread(10) ? "yes" : "no"}</p>
      <p>mentions {readStates.mentionCount(10)}</p>
      <p>channel eleven mentions {readStates.mentionCount(11)}</p>
      <p>stable {stable ? "yes" : "no"}</p>
      <p>mark result {markResult}</p>
      <button type="button" onClick={() => void readStates.refresh()}>
        refresh
      </button>
      <button
        type="button"
        onClick={() =>
          void readStates.markRead(10, 20).then((result) => setMarkResult(result ? "ok" : "null"))
        }
      >
        mark read
      </button>
    </div>
  );
}

function Harness(props: { events?: boolean }) {
  const content = (
    <ReadStatesProvider>
      <Probe />
    </ReadStatesProvider>
  );
  return props.events ? <EventsProvider>{content}</EventsProvider> : content;
}

beforeEach(() => {
  authState.user = USER;
  authState.status = "authenticated";
});

afterEach(() => {
  vi.clearAllMocks();
  vi.unstubAllGlobals();
  resetFakeEventSources();
});

describe("ReadStatesProvider", () => {
  test("exposes snapshot status, errors, refresh, and stable selectors", async () => {
    listReadStatesMock.mockResolvedValueOnce([summary()]);
    render(<Harness />);

    await waitFor(() => expect(screen.getByText("status ready")).toBeInTheDocument());
    expect(screen.getByText("unread yes")).toBeInTheDocument();
    expect(screen.getByText("mentions 3")).toBeInTheDocument();
    expect(screen.getByText("stable yes")).toBeInTheDocument();

    listReadStatesMock.mockRejectedValueOnce(new Error("network"));
    const warn = captureExpectedConsoleDiagnostics("warn");
    fireEvent.click(screen.getByRole("button", { name: "refresh" }));
    await waitFor(() => expect(screen.getByText("status error")).toBeInTheDocument());
    expect(screen.getByText("error yes")).toBeInTheDocument();
    expect(screen.getByText("unread yes")).toBeInTheDocument();
    expect(screen.getByText("stable yes")).toBeInTheDocument();
    expect(warn.diagnostics).toEqual([["failed to load read-state snapshot", expect.any(Error)]]);
    warn.stop();
  });

  test("latest-started snapshot wins", async () => {
    const first = deferred<ReadStateSummary[]>();
    const second = deferred<ReadStateSummary[]>();
    listReadStatesMock.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    render(<Harness />);

    fireEvent.click(screen.getByRole("button", { name: "refresh" }));
    second.resolve([summary({ mention_count: 8, updated_at: 400 })]);
    await waitFor(() => expect(screen.getByText("mentions 8")).toBeInTheDocument());
    first.resolve([summary({ mention_count: 1, updated_at: 100 })]);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.getByText("mentions 8")).toBeInTheDocument();
  });

  test("messages and summaries delivered during a snapshot survive completion", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    const pending = deferred<ReadStateSummary[]>();
    listReadStatesMock.mockReturnValueOnce(pending.promise);
    render(<Harness events />);
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    latestFakeEventSource()?.pushMessage({
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
    });
    latestFakeEventSource()?.pushReadStateUpdated(
      summary({ channel_id: 11, mention_count: 4, updated_at: 300 }),
    );
    pending.resolve([
      summary({ has_unread: false, mention_count: 0 }),
      summary({ channel_id: 11, mention_count: 0 }),
    ]);

    await waitFor(() => expect(screen.getByText("mentions 1")).toBeInTheDocument());
    expect(screen.getByText("channel eleven mentions 4")).toBeInTheDocument();
    expect(screen.getByText("unread yes")).toBeInTheDocument();
  });

  test("a reconnect invalidates a pending preconnection snapshot", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    const beforeConnection = deferred<ReadStateSummary[]>();
    const afterConnection = deferred<ReadStateSummary[]>();
    listReadStatesMock
      .mockReturnValueOnce(beforeConnection.promise)
      .mockReturnValueOnce(afterConnection.promise);
    render(<Harness events />);
    await waitFor(() => expect(listReadStatesMock).toHaveBeenCalledTimes(1));

    latestFakeEventSource()?.pushConnected();
    await waitFor(() => expect(listReadStatesMock).toHaveBeenCalledTimes(2));
    expect(listReadStatesMock.mock.calls[0]?.[0]?.aborted).toBe(true);
    beforeConnection.resolve([summary({ mention_count: 1 })]);
    afterConnection.resolve([summary({ mention_count: 9, updated_at: 500 })]);
    await waitFor(() => expect(screen.getByText("mentions 9")).toBeInTheDocument());
  });

  test("clears on logout/account change and rejects the old account completion", async () => {
    const oldAccount = deferred<ReadStateSummary[]>();
    listReadStatesMock
      .mockReturnValueOnce(oldAccount.promise)
      .mockResolvedValueOnce([summary({ mention_count: 7, updated_at: 700 })]);
    const view = render(<Harness />);

    authState.user = OTHER_USER;
    view.rerender(<Harness />);
    await waitFor(() => expect(screen.getByText("mentions 7")).toBeInTheDocument());
    oldAccount.resolve([summary({ mention_count: 2 })]);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.getByText("mentions 7")).toBeInTheDocument();

    authState.user = null;
    authState.status = "anonymous";
    view.rerender(<Harness />);
    await waitFor(() => expect(screen.getByText("status idle")).toBeInTheDocument());
    expect(screen.getByText("mentions 0")).toBeInTheDocument();
  });

  test("markRead returns null on failure and a stale response cannot regress a newer summary", async () => {
    listReadStatesMock.mockResolvedValueOnce([summary()]);
    const staleMark = deferred<ReadStateSummary>();
    markChannelReadMock
      .mockReturnValueOnce(staleMark.promise)
      .mockRejectedValueOnce(new Error("x"));
    const warn = captureExpectedConsoleDiagnostics("warn");
    render(<Harness />);
    await waitFor(() => expect(screen.getByText("status ready")).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: "mark read" }));
    // A newer accepted snapshot arrives while the mark request is pending.
    listReadStatesMock.mockResolvedValueOnce([
      summary({ has_unread: true, mention_count: 6, updated_at: 500 }),
    ]);
    fireEvent.click(screen.getByRole("button", { name: "refresh" }));
    await waitFor(() => expect(screen.getByText("mentions 6")).toBeInTheDocument());
    staleMark.resolve(summary({ has_unread: false, mention_count: 0, updated_at: 100 }));
    await waitFor(() => expect(screen.getByText("mark result null")).toBeInTheDocument());
    expect(screen.getByText("unread yes")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "mark read" }));
    await waitFor(() => expect(screen.getByText("mark result null")).toBeInTheDocument());
    expect(warn.diagnostics).toEqual([["failed to mark channel read", expect.any(Error)]]);
    warn.stop();
  });

  test("Strict Mode replay aborts obsolete work and commits only the current setup", async () => {
    const first = deferred<ReadStateSummary[]>();
    listReadStatesMock.mockReturnValueOnce(first.promise).mockResolvedValue([summary()]);
    const view = render(
      <StrictMode>
        <Harness />
      </StrictMode>,
    );

    await waitFor(() => expect(listReadStatesMock.mock.calls.length).toBeGreaterThanOrEqual(2));
    first.resolve([summary({ mention_count: 99 })]);
    await waitFor(() => expect(screen.getByText("mentions 3")).toBeInTheDocument());
    expect(screen.getByText("stable yes")).toBeInTheDocument();

    const pendingAtUnmount = deferred<ReadStateSummary[]>();
    listReadStatesMock.mockReturnValueOnce(pendingAtUnmount.promise);
    fireEvent.click(screen.getByRole("button", { name: "refresh" }));
    await waitFor(() => expect(screen.getByText("status loading")).toBeInTheDocument());
    const finalSignal = listReadStatesMock.mock.calls.at(-1)?.[0];
    view.unmount();
    expect(finalSignal?.aborted).toBe(true);
  });
});
