import { StrictMode, useState } from "react";
import { act, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import type { Channel, User } from "../api";

const listChannelsMock = vi.hoisted(() => vi.fn<(signal?: AbortSignal) => Promise<Channel[]>>());
const reorderChannelsMock = vi.hoisted(() => vi.fn<(ids: number[]) => Promise<Channel[]>>());

vi.mock("../api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../api")>();
  return {
    ...actual,
    listChannels: listChannelsMock,
    reorderChannels: reorderChannelsMock,
  };
});

const authState = vi.hoisted(
  () =>
    ({
      status: "authenticated" as "authenticated" | "anonymous",
      user: {
        id: 1,
        username: "alice",
        display_name: null,
        email: null,
        email_verified: false,
        avatar_url: null,
      } as User | null,
    }) as {
      status: "authenticated" | "anonymous";
      user: User | null;
    },
);

vi.mock("./auth", () => ({
  useAuth: () => authState,
}));

type Listener<T> = (value: T) => void;
const eventBus = vi.hoisted(() => {
  const created = new Set<Listener<Channel>>();
  const reordered = new Set<Listener<Channel[]>>();
  const connected = new Set<Listener<void>>();
  const subscribe = <T,>(set: Set<Listener<T>>, listener: Listener<T>) => {
    set.add(listener);
    return () => set.delete(listener);
  };
  return {
    created,
    reordered,
    connected,
    events: {
      onChannelCreated: (listener: Listener<Channel>) => subscribe(created, listener),
      onChannelsReordered: (listener: Listener<Channel[]>) => subscribe(reordered, listener),
      onConnected: (listener: Listener<void>) => subscribe(connected, listener),
    },
    emitCreated(channel: Channel) {
      for (const listener of created) listener(channel);
    },
    emitReordered(channels: Channel[]) {
      for (const listener of reordered) listener(channels);
    },
    emitConnected() {
      for (const listener of connected) listener();
    },
    reset() {
      created.clear();
      reordered.clear();
      connected.clear();
    },
  };
});

vi.mock("./events", () => ({
  useEvents: () => eventBus.events,
}));

import { ChannelsProvider, useChannels, type ChannelsContextValue } from "./channels";

const GENERAL: Channel = { id: 10, name: "general", position: 0, type: "text" };
const RANDOM: Channel = { id: 20, name: "random", position: 1, type: "text" };
const VOICE: Channel = { id: 30, name: "voice", position: 2, type: "voice" };

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

let currentContext: ChannelsContextValue;
const snapshots: ChannelsContextValue[] = [];

function Probe() {
  const context = useChannels();
  currentContext = context;
  snapshots.push(context);
  return (
    <div>
      <p data-testid="status">{context.status}</p>
      <p data-testid="channels">{context.channels.map((channel) => channel.name).join(",")}</p>
      <p data-testid="reordering">{String(context.reordering)}</p>
    </div>
  );
}

function Harness() {
  const [, rerender] = useState(0);
  return (
    <ChannelsProvider>
      <button type="button" onClick={() => rerender((value) => value + 1)}>
        unrelated rerender
      </button>
      <Probe />
    </ChannelsProvider>
  );
}

function mount() {
  return render(
    <StrictMode>
      <Harness />
    </StrictMode>,
  );
}

async function resolveLatestInitial(
  requests: Array<ReturnType<typeof deferred<Channel[]>>>,
  value: Channel[],
) {
  await waitFor(() => expect(requests.length).toBeGreaterThanOrEqual(2));
  await act(async () => requests.at(-1)?.resolve(value));
  await waitFor(() => expect(screen.getByTestId("status")).toHaveTextContent("ready"));
}

beforeEach(() => {
  authState.status = "authenticated";
  authState.user = {
    id: 1,
    username: "alice",
    display_name: null,
    email: null,
    email_verified: false,
    avatar_url: null,
  };
  eventBus.reset();
  snapshots.length = 0;
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("ChannelsProvider", () => {
  test("loads only for authenticated users, sorts position then ID, and clears on account change", async () => {
    const requests: Array<ReturnType<typeof deferred<Channel[]>>> = [];
    listChannelsMock.mockImplementation(() => {
      const request = deferred<Channel[]>();
      requests.push(request);
      return request.promise;
    });
    const view = mount();

    await resolveLatestInitial(requests, [
      { ...RANDOM, position: 1 },
      { ...GENERAL, id: 11, name: "later-id", position: 0 },
      GENERAL,
    ]);
    expect(screen.getByTestId("channels")).toHaveTextContent("general,later-id,random");
    const staleAccountRequest = requests.at(-2);

    authState.status = "anonymous";
    authState.user = null;
    view.rerender(
      <StrictMode>
        <Harness />
      </StrictMode>,
    );
    expect(screen.getByTestId("channels")).toHaveTextContent("");
    expect(screen.getByTestId("status")).toHaveTextContent("idle");
    const anonymousRequestCount = listChannelsMock.mock.calls.length;
    await act(async () => staleAccountRequest?.resolve([VOICE]));
    expect(screen.getByTestId("channels")).toHaveTextContent("");

    authState.status = "authenticated";
    authState.user = { ...GENERAL_USER, id: 2, username: "bob" };
    view.rerender(
      <StrictMode>
        <Harness />
      </StrictMode>,
    );
    expect(screen.getByTestId("channels")).toHaveTextContent("");
    await waitFor(() =>
      expect(listChannelsMock.mock.calls.length).toBeGreaterThan(anonymousRequestCount),
    );
    await act(async () => requests.at(-1)?.resolve([VOICE]));
    await waitFor(() => expect(screen.getByTestId("channels")).toHaveTextContent("voice"));
  });

  test("replays idempotent create and reorder events over a delayed snapshot", async () => {
    const requests: Array<ReturnType<typeof deferred<Channel[]>>> = [];
    listChannelsMock.mockImplementation(() => {
      const request = deferred<Channel[]>();
      requests.push(request);
      return request.promise;
    });
    mount();
    await waitFor(() => expect(requests.length).toBeGreaterThanOrEqual(2));

    act(() => {
      eventBus.emitCreated(VOICE);
      eventBus.emitCreated(VOICE);
      eventBus.emitReordered([
        { ...VOICE, position: 0 },
        { ...GENERAL, position: 1 },
      ]);
      eventBus.emitReordered([
        { ...VOICE, position: 0 },
        { ...GENERAL, position: 1 },
      ]);
    });
    expect(screen.getByTestId("channels")).toHaveTextContent("voice,general");

    await act(async () => requests.at(-1)?.resolve([GENERAL]));
    await waitFor(() => expect(screen.getByTestId("status")).toHaveTextContent("ready"));
    expect(screen.getByTestId("channels")).toHaveTextContent("voice,general");
  });

  test("treats reconnect as a barrier by aborting a pending snapshot and requiring a newer one", async () => {
    const requests: Array<{
      request: ReturnType<typeof deferred<Channel[]>>;
      signal?: AbortSignal;
    }> = [];
    listChannelsMock.mockImplementation((signal) => {
      const request = deferred<Channel[]>();
      requests.push({ request, signal });
      return request.promise;
    });
    mount();
    await waitFor(() => expect(requests.length).toBeGreaterThanOrEqual(2));
    const preReconnect = requests.at(-1);
    expect(preReconnect).toBeDefined();
    if (!preReconnect) throw new Error("expected a pre-reconnect request");

    act(() => eventBus.emitConnected());
    await waitFor(() => expect(requests.length).toBeGreaterThanOrEqual(3));
    expect(preReconnect.signal?.aborted).toBe(true);

    await act(async () => preReconnect.request.resolve([GENERAL]));
    expect(screen.getByTestId("channels")).toHaveTextContent("");
    await act(async () => requests.at(-1)?.request.resolve([RANDOM]));
    await waitFor(() => expect(screen.getByTestId("channels")).toHaveTextContent("random"));
  });

  test("validates permutations and rolls back a failed optimistic reorder", async () => {
    const requests: Array<ReturnType<typeof deferred<Channel[]>>> = [];
    listChannelsMock.mockImplementation(() => {
      const request = deferred<Channel[]>();
      requests.push(request);
      return request.promise;
    });
    const failed = deferred<Channel[]>();
    reorderChannelsMock.mockImplementation(() => failed.promise);
    mount();
    await resolveLatestInitial(requests, [GENERAL, RANDOM]);

    await expect(currentContext.reorder([10])).rejects.toThrow("permutation");
    expect(reorderChannelsMock).not.toHaveBeenCalled();

    let reorderPromise!: Promise<void>;
    act(() => {
      reorderPromise = currentContext.reorder([20, 10]);
    });
    expect(screen.getByTestId("channels")).toHaveTextContent("random,general");
    expect(screen.getByTestId("reordering")).toHaveTextContent("true");

    const rejection = expect(reorderPromise).rejects.toThrow("nope");
    await act(async () => failed.reject(new Error("nope")));
    await rejection;
    expect(screen.getByTestId("channels")).toHaveTextContent("general,random");
    expect(screen.getByTestId("reordering")).toHaveTextContent("false");
  });

  test("an older overlapping reorder response cannot overwrite the newest intent", async () => {
    const requests: Array<ReturnType<typeof deferred<Channel[]>>> = [];
    listChannelsMock.mockImplementation(() => {
      const request = deferred<Channel[]>();
      requests.push(request);
      return request.promise;
    });
    const first = deferred<Channel[]>();
    const second = deferred<Channel[]>();
    reorderChannelsMock.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    mount();
    await resolveLatestInitial(requests, [GENERAL, RANDOM, VOICE]);

    let firstPromise!: Promise<void>;
    act(() => {
      firstPromise = currentContext.reorder([20, 10, 30]);
    });
    await waitFor(() =>
      expect(screen.getByTestId("channels")).toHaveTextContent("random,general,voice"),
    );
    let secondPromise!: Promise<void>;
    act(() => {
      secondPromise = currentContext.reorder([30, 20, 10]);
    });
    expect(screen.getByTestId("channels")).toHaveTextContent("voice,random,general");

    await act(async () => first.resolve([GENERAL, RANDOM, VOICE]));
    await firstPromise;
    expect(screen.getByTestId("channels")).toHaveTextContent("voice,random,general");

    await act(async () =>
      second.resolve([
        { ...VOICE, position: 0 },
        { ...RANDOM, position: 1 },
        { ...GENERAL, position: 2 },
      ]),
    );
    await secondPromise;
    expect(screen.getByTestId("channels")).toHaveTextContent("voice,random,general");
  });

  test("restores the confirmed order when both overlapping reorders fail", async () => {
    const requests: Array<ReturnType<typeof deferred<Channel[]>>> = [];
    listChannelsMock.mockImplementation(() => {
      const request = deferred<Channel[]>();
      requests.push(request);
      return request.promise;
    });
    const first = deferred<Channel[]>();
    const second = deferred<Channel[]>();
    reorderChannelsMock.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    mount();
    await resolveLatestInitial(requests, [GENERAL, RANDOM, VOICE]);

    let firstPromise!: Promise<void>;
    act(() => {
      firstPromise = currentContext.reorder([20, 10, 30]);
    });
    await waitFor(() =>
      expect(screen.getByTestId("channels")).toHaveTextContent("random,general,voice"),
    );
    let secondPromise!: Promise<void>;
    act(() => {
      secondPromise = currentContext.reorder([30, 20, 10]);
    });

    const firstRejection = expect(firstPromise).rejects.toThrow("first failed");
    await act(async () => first.reject(new Error("first failed")));
    await firstRejection;
    expect(screen.getByTestId("channels")).toHaveTextContent("voice,random,general");
    expect(screen.getByTestId("reordering")).toHaveTextContent("true");

    const secondRejection = expect(secondPromise).rejects.toThrow("second failed");
    await act(async () => second.reject(new Error("second failed")));
    await secondRejection;
    expect(screen.getByTestId("channels")).toHaveTextContent("general,random,voice");
    expect(screen.getByTestId("reordering")).toHaveTextContent("false");
  });

  test("an older failure after a newer success cannot roll back the newer order", async () => {
    const requests: Array<ReturnType<typeof deferred<Channel[]>>> = [];
    listChannelsMock.mockImplementation(() => {
      const request = deferred<Channel[]>();
      requests.push(request);
      return request.promise;
    });
    const first = deferred<Channel[]>();
    const second = deferred<Channel[]>();
    reorderChannelsMock.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    mount();
    await resolveLatestInitial(requests, [GENERAL, RANDOM, VOICE]);

    let firstPromise!: Promise<void>;
    act(() => {
      firstPromise = currentContext.reorder([20, 10, 30]);
    });
    await waitFor(() =>
      expect(screen.getByTestId("channels")).toHaveTextContent("random,general,voice"),
    );
    let secondPromise!: Promise<void>;
    act(() => {
      secondPromise = currentContext.reorder([30, 20, 10]);
    });

    await act(async () =>
      second.resolve([
        { ...VOICE, position: 0 },
        { ...RANDOM, position: 1 },
        { ...GENERAL, position: 2 },
      ]),
    );
    await secondPromise;
    expect(screen.getByTestId("channels")).toHaveTextContent("voice,random,general");

    const firstRejection = expect(firstPromise).rejects.toThrow("older failed");
    await act(async () => first.reject(new Error("older failed")));
    await firstRejection;
    expect(screen.getByTestId("channels")).toHaveTextContent("voice,random,general");
    expect(screen.getByTestId("reordering")).toHaveTextContent("false");
  });

  test("keeps the newest optimistic order over an older reorder SSE echo", async () => {
    const requests: Array<ReturnType<typeof deferred<Channel[]>>> = [];
    listChannelsMock.mockImplementation(() => {
      const request = deferred<Channel[]>();
      requests.push(request);
      return request.promise;
    });
    const first = deferred<Channel[]>();
    const second = deferred<Channel[]>();
    reorderChannelsMock.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    mount();
    await resolveLatestInitial(requests, [GENERAL, RANDOM, VOICE]);

    let firstPromise!: Promise<void>;
    act(() => {
      firstPromise = currentContext.reorder([20, 10, 30]);
    });
    await waitFor(() =>
      expect(screen.getByTestId("channels")).toHaveTextContent("random,general,voice"),
    );
    let secondPromise!: Promise<void>;
    act(() => {
      secondPromise = currentContext.reorder([30, 20, 10]);
    });

    act(() => {
      eventBus.emitReordered([
        { ...RANDOM, position: 0 },
        { ...GENERAL, position: 1 },
        { ...VOICE, position: 2 },
      ]);
    });
    expect(screen.getByTestId("channels")).toHaveTextContent("voice,random,general");

    await act(async () => first.resolve([RANDOM, GENERAL, VOICE]));
    await firstPromise;
    expect(screen.getByTestId("channels")).toHaveTextContent("voice,random,general");
    await act(async () =>
      second.resolve([
        { ...VOICE, position: 0 },
        { ...RANDOM, position: 1 },
        { ...GENERAL, position: 2 },
      ]),
    );
    await secondPromise;
    expect(screen.getByTestId("channels")).toHaveTextContent("voice,random,general");
  });

  test("memoizes the value and keeps actions stable across Strict Mode and unrelated rerenders", async () => {
    listChannelsMock.mockResolvedValue([GENERAL]);
    mount();
    await waitFor(() => expect(screen.getByTestId("status")).toHaveTextContent("ready"));
    const readyValue = currentContext;
    const refresh = currentContext.refresh;
    const reorder = currentContext.reorder;
    snapshots.length = 0;

    act(() => screen.getByRole("button", { name: "unrelated rerender" }).click());

    expect(currentContext).toBe(readyValue);
    expect(currentContext.refresh).toBe(refresh);
    expect(currentContext.reorder).toBe(reorder);
    expect(snapshots.every((snapshot) => snapshot === readyValue)).toBe(true);
  });
});

const GENERAL_USER: User = {
  id: 1,
  username: "alice",
  display_name: null,
  email: null,
  email_verified: false,
  avatar_url: null,
};
