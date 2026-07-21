import { useState } from "react";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Message, Thread } from "../api";
import { makeMessage } from "../test/fixtures";
import { renderNative } from "../test/render";
import ThreadPanel from "./thread-panel";

const api = vi.hoisted(() => ({
  getThread: vi.fn(),
  sendThreadReply: vi.fn(),
}));

vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  getThread: api.getThread,
  sendThreadReply: api.sendThreadReply,
}));

type Listener = (value: never) => void;
const eventListeners = vi.hoisted(() => ({
  replyCreated: new Set<Listener>(),
  replyDeleted: new Set<Listener>(),
  messageUpdated: new Set<Listener>(),
  messageDeleted: new Set<Listener>(),
  embedsUpdated: new Set<Listener>(),
  reactionsUpdated: new Set<Listener>(),
  connected: new Set<Listener>(),
}));

function subscribe(set: Set<Listener>, listener: Listener) {
  set.add(listener);
  return () => set.delete(listener);
}

vi.mock("../contexts/events", () => ({
  useEvents: () => ({
    onThreadReplyCreated: (listener: Listener) => subscribe(eventListeners.replyCreated, listener),
    onThreadReplyDeleted: (listener: Listener) => subscribe(eventListeners.replyDeleted, listener),
    onMessageUpdated: (listener: Listener) => subscribe(eventListeners.messageUpdated, listener),
    onMessageDeleted: (listener: Listener) => subscribe(eventListeners.messageDeleted, listener),
    onMessageEmbedsUpdated: (listener: Listener) =>
      subscribe(eventListeners.embedsUpdated, listener),
    onMessageReactionsUpdated: (listener: Listener) =>
      subscribe(eventListeners.reactionsUpdated, listener),
    onConnected: (listener: Listener) => subscribe(eventListeners.connected, listener),
  }),
}));

const channelId = 10;
const rootId = 100;

function message(id: number, overrides: Partial<Message> = {}): Message {
  return makeMessage({
    id,
    user_id: 1,
    channel_id: channelId,
    parent_id: id === rootId ? null : rootId,
    created_at: id,
    text: `message ${id}`,
    username: "alice",
    ...overrides,
  });
}

function thread(replies: Message[] = []): Thread {
  return { root: message(rootId), replies, has_more_replies: false };
}

function renderPanel(onClose = vi.fn()) {
  renderNative(
    <ThreadPanel
      channelId={channelId}
      rootMessageId={rootId}
      currentUserId={1}
      onClose={onClose}
    />,
  );
  return onClose;
}

function emit(set: Set<Listener>, value: unknown) {
  act(() => {
    for (const listener of set) listener(value as never);
  });
}

beforeEach(() => {
  api.getThread.mockReset();
  api.sendThreadReply.mockReset();
  for (const listeners of Object.values(eventListeners)) listeners.clear();
});

describe("ThreadPanel reducer ownership", () => {
  it("rejects a thread payload whose root does not belong to the requested channel", async () => {
    api.getThread.mockResolvedValue({
      ...thread(),
      root: message(rootId, { channel_id: 999 }),
    });

    renderPanel();

    expect(await screen.findByRole("alert")).toHaveTextContent("Invalid thread response");
    expect(screen.queryByText(`message ${rootId}`)).not.toBeInTheDocument();
  });

  it("journals a live reply while the initial snapshot is pending and does not duplicate it", async () => {
    const requests: Array<(value: Thread) => void> = [];
    api.getThread.mockImplementation(
      () => new Promise<Thread>((resolve) => requests.push(resolve)),
    );
    renderPanel();
    await waitFor(() => expect(requests.length).toBeGreaterThan(0));

    const reply = message(101, { text: "live reply" });
    emit(eventListeners.replyCreated, {
      channel_id: channelId,
      root_message_id: rootId,
      reply,
      thread_summary: { reply_count: 1, last_reply_created_at: 101 },
    });

    act(() => requests.at(-1)?.(thread([message(101, { text: "snapshot copy" })])));
    await waitFor(() => expect(screen.getAllByText("live reply")).toHaveLength(1));
    expect(screen.queryByText("snapshot copy")).not.toBeInTheDocument();
  });

  it("treats reconnect as a freshness barrier and ignores the older snapshot", async () => {
    const requests: Array<(value: Thread) => void> = [];
    api.getThread.mockImplementation(
      () => new Promise<Thread>((resolve) => requests.push(resolve)),
    );
    renderPanel();
    await waitFor(() => expect(requests.length).toBeGreaterThan(0));

    emit(eventListeners.connected, undefined);
    await waitFor(() => expect(requests.length).toBeGreaterThan(1));
    const newestRequest = requests.at(-1);
    act(() => requests.at(-2)?.(thread([message(101, { text: "stale reply" })])));
    expect(screen.queryByText("stale reply")).not.toBeInTheDocument();

    act(() => newestRequest?.(thread([message(102, { text: "fresh reply" })])));
    expect(await screen.findByText("fresh reply")).toBeInTheDocument();
  });

  it("renders retained thread data with a nonblocking reconnect error", async () => {
    api.getThread.mockResolvedValue(thread([message(101, { text: "retained reply" })]));
    renderPanel();

    expect(await screen.findByText("retained reply")).toBeInTheDocument();
    api.getThread.mockRejectedValue(new Error("refresh unavailable"));
    emit(eventListeners.connected, undefined);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Error refreshing thread: Error: refresh unavailable",
    );
    expect(screen.getByText(`message ${rootId}`)).toBeInTheDocument();
    expect(screen.getByText("retained reply")).toBeInTheDocument();
  });

  it("invalidates a pending older page on reconnect and commits only the fresh snapshot", async () => {
    let resolvePage: (value: Thread) => void = () => undefined;
    let resolveRefresh: (value: Thread) => void = () => undefined;
    api.getThread.mockImplementation((_rootId: number, options: { beforeId?: number }) => {
      if (options.beforeId !== undefined) {
        return new Promise<Thread>((resolve) => {
          resolvePage = resolve;
        });
      }
      if (api.getThread.mock.calls.length > 2) {
        return new Promise<Thread>((resolve) => {
          resolveRefresh = resolve;
        });
      }
      return Promise.resolve({ ...thread([message(102)]), has_more_replies: true });
    });
    renderPanel();
    const loadOlder = await screen.findByRole("button", { name: "Load older replies" });

    fireEvent.click(loadOlder);
    await waitFor(() =>
      expect(api.getThread.mock.calls.some(([, options]) => options.beforeId === 102)).toBe(true),
    );
    emit(eventListeners.connected, undefined);
    await waitFor(() => expect(api.getThread.mock.calls.length).toBeGreaterThan(2));

    act(() => resolvePage(thread([message(101, { text: "stale older reply" })])));
    expect(screen.queryByText("stale older reply")).not.toBeInTheDocument();

    act(() => resolveRefresh(thread([message(103, { text: "fresh reply" })])));
    expect(await screen.findByText("fresh reply")).toBeInTheDocument();
    expect(screen.queryByText(`message 102`)).not.toBeInTheDocument();
  });

  it("preserves pagination scroll position after older rows commit", async () => {
    let resolvePage: (value: Thread) => void = () => undefined;
    api.getThread.mockImplementation((_rootId: number, options: { beforeId?: number }) => {
      if (options.beforeId !== undefined) {
        return new Promise<Thread>((resolve) => {
          resolvePage = resolve;
        });
      }
      return Promise.resolve({ ...thread([message(102)]), has_more_replies: true });
    });
    renderPanel();
    const panel = await screen.findByRole("complementary", { name: "Thread panel" });
    const scroller = panel.querySelector(".overflow-y-auto") as HTMLDivElement;
    let height = 100;
    let top = 20;
    Object.defineProperties(scroller, {
      scrollHeight: { configurable: true, get: () => height },
      scrollTop: {
        configurable: true,
        get: () => top,
        set: (value: number) => {
          top = value;
        },
      },
    });

    fireEvent.click(await screen.findByRole("button", { name: "Load older replies" }));
    await waitFor(() =>
      expect(api.getThread.mock.calls.some(([, options]) => options.beforeId === 102)).toBe(true),
    );
    act(() => {
      height = 180;
      resolvePage(thread([message(101, { text: "older reply" })]));
    });

    expect(await screen.findByText("older reply")).toBeInTheDocument();
    expect(top).toBe(100);
  });

  it("closes on a hard root delete while leaving a tombstone update renderable", async () => {
    api.getThread.mockResolvedValue(thread());
    const onClose = renderPanel();
    await screen.findByText(`message ${rootId}`);

    emit(eventListeners.messageUpdated, message(rootId, { deleted_at: 1 }));
    expect(screen.getByLabelText("Original message deleted")).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();

    emit(eventListeners.messageDeleted, { id: rootId, channel_id: channelId });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("isolates a rapid root switch from delayed initial and send completions", async () => {
    const initialRequests = new Map<number, Array<(value: Thread) => void>>();
    let resolveSend: (value: Message) => void = () => undefined;
    api.getThread.mockImplementation(
      (requestedRootId: number) =>
        new Promise<Thread>((resolve) => {
          const requests = initialRequests.get(requestedRootId) ?? [];
          requests.push(resolve);
          initialRequests.set(requestedRootId, requests);
        }),
    );
    api.sendThreadReply.mockImplementation(
      () => new Promise<Message>((resolve) => (resolveSend = resolve)),
    );
    function Harness() {
      const [activeRootId, setActiveRootId] = useState(rootId);
      return (
        <>
          <button type="button" onClick={() => setActiveRootId(200)}>
            Open thread B
          </button>
          <ThreadPanel
            key={`${channelId}:${activeRootId}`}
            channelId={channelId}
            rootMessageId={activeRootId}
            currentUserId={1}
            onClose={() => undefined}
          />
        </>
      );
    }
    renderNative(<Harness />);
    await waitFor(() => expect(initialRequests.get(rootId)?.length).toBeGreaterThan(0));
    act(() => initialRequests.get(rootId)?.at(-1)?.(thread()));

    const oldPanel = await screen.findByRole("complementary", { name: "Thread panel" });
    const oldInput = within(oldPanel).getByLabelText("Thread reply") as HTMLInputElement;
    fireEvent.input(oldInput, { target: { value: "reply for A" } });
    fireEvent.click(within(oldPanel).getByRole("button", { name: "Send response to thread" }));
    await waitFor(() =>
      expect(api.sendThreadReply.mock.calls.at(-1)?.slice(0, 3)).toEqual([
        rootId,
        "reply for A",
        [],
      ]),
    );

    fireEvent.click(screen.getByRole("button", { name: "Open thread B" }));
    await waitFor(() => expect(initialRequests.get(200)?.length).toBeGreaterThan(0));
    const rootB = message(200, { parent_id: null, text: "root B" });
    act(() =>
      initialRequests.get(200)?.at(-1)?.({ root: rootB, replies: [], has_more_replies: false }),
    );
    expect(await screen.findByText("root B")).toBeInTheDocument();

    act(() => resolveSend(message(101, { text: "late reply for A" })));
    await waitFor(() => expect(screen.queryByText("late reply for A")).not.toBeInTheDocument());
    expect(screen.getByLabelText("Thread reply")).toHaveTextContent("");
  });

  it("scrolls an initially loaded long thread to the bottom after commit", async () => {
    let resolveInitial: (value: Thread) => void = () => undefined;
    api.getThread.mockImplementation(
      () => new Promise<Thread>((resolve) => (resolveInitial = resolve)),
    );
    renderPanel();
    const panel = await screen.findByRole("complementary", { name: "Thread panel" });
    const scroller = panel.querySelector(".overflow-y-auto") as HTMLDivElement;
    let top = 0;
    Object.defineProperties(scroller, {
      scrollHeight: { configurable: true, get: () => 900 },
      scrollTop: {
        configurable: true,
        get: () => top,
        set: (value: number) => {
          top = value;
        },
      },
    });

    act(() => resolveInitial(thread([message(101), message(102)])));

    expect(await screen.findByText("message 102")).toBeInTheDocument();
    expect(top).toBe(900);
  });

  it("preserves the composer and alerts on a malformed successful reply", async () => {
    api.getThread.mockResolvedValue(thread());
    api.sendThreadReply.mockResolvedValue(message(101, { channel_id: 999 }));
    renderPanel();
    const panel = await screen.findByRole("complementary", { name: "Thread panel" });
    await within(panel).findByText(`message ${rootId}`);
    const input = within(panel).getByLabelText("Thread reply") as HTMLInputElement;
    fireEvent.input(input, { target: { value: "keep this" } });
    fireEvent.click(within(panel).getByRole("button", { name: "Send response to thread" }));

    expect(await within(panel).findByRole("alert")).toHaveTextContent(
      "Invalid thread reply response",
    );
    expect(input.value).toBe("keep this");
    expect(within(panel).queryByText("message 101")).not.toBeInTheDocument();
  });

  it("aborts a pending send on reconnect and keeps the composer usable", async () => {
    api.getThread.mockResolvedValue(thread());
    let sendSignal: AbortSignal | undefined;
    api.sendThreadReply.mockImplementation(
      (_rootId: number, _text: string, _photos: File[], signal?: AbortSignal) => {
        sendSignal = signal;
        return new Promise<Message>(() => undefined);
      },
    );
    renderPanel();
    const panel = await screen.findByRole("complementary", { name: "Thread panel" });
    await within(panel).findByText(`message ${rootId}`);
    const input = within(panel).getByLabelText("Thread reply") as HTMLInputElement;
    fireEvent.input(input, { target: { value: "still here" } });
    fireEvent.click(within(panel).getByRole("button", { name: "Send response to thread" }));
    await waitFor(() => expect(sendSignal).toBeDefined());

    emit(eventListeners.connected, undefined);

    expect(sendSignal?.aborted).toBe(true);
    await waitFor(() =>
      expect(within(panel).getByRole("button", { name: "Send response to thread" })).toBeEnabled(),
    );
    expect(input.value).toBe("still here");
  });

  it("closes only once for duplicate hard-root deletions", async () => {
    api.getThread.mockResolvedValue(thread());
    const onClose = renderPanel();
    await screen.findByText(`message ${rootId}`);

    emit(eventListeners.messageDeleted, { id: rootId, channel_id: channelId });
    emit(eventListeners.messageDeleted, { id: rootId, channel_id: channelId });

    expect(onClose).toHaveBeenCalledOnce();
  });

  it("does not replace text entered after a failed send started", async () => {
    api.getThread.mockResolvedValue(thread());
    let rejectSend: (error: Error) => void = () => undefined;
    api.sendThreadReply.mockImplementation(
      () => new Promise<Message>((_resolve, reject) => (rejectSend = reject)),
    );
    renderPanel();
    const panel = await screen.findByRole("complementary", { name: "Thread panel" });
    await within(panel).findByText(`message ${rootId}`);
    const input = within(panel).getByLabelText("Thread reply") as HTMLInputElement;

    fireEvent.input(input, { target: { value: "submitted" } });
    fireEvent.click(within(panel).getByRole("button", { name: "Send response to thread" }));
    await waitFor(() =>
      expect(api.sendThreadReply.mock.calls.at(-1)?.slice(0, 3)).toEqual([rootId, "submitted", []]),
    );
    fireEvent.input(input, { target: { value: "replacement" } });
    act(() => rejectSend(new Error("send failed")));

    await waitFor(() => expect(input.value).toBe("replacement"));
    expect(within(panel).queryByText("send failed")).not.toBeInTheDocument();
  });
});
