import { StrictMode } from "react";
import { act, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import { useResource, type ResourceControls, type ResourceState } from "./use-resource";

interface Deferred<T> {
  promise: Promise<T>;
  resolve(value: T): void;
  reject(error: unknown): void;
}

function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

type Loader = (key: string, signal: AbortSignal) => Promise<string>;

let currentState: ResourceState<string>;
let currentControls: ResourceControls<string>;

function ResourceProbe({
  resourceKey,
  load,
  keepDataOnRefetch,
  onControls,
}: {
  resourceKey: string | null;
  load: Loader;
  keepDataOnRefetch?: boolean;
  onControls?: (controls: ResourceControls<string>) => void;
}) {
  const [state, controls] = useResource({
    key: resourceKey,
    load,
    keepDataOnRefetch,
  });
  currentState = state;
  currentControls = controls;
  onControls?.(controls);

  return (
    <div>
      <span data-testid="data">{state.data ?? "none"}</span>
      <span data-testid="status">{state.status}</span>
      <span data-testid="loading">{String(state.loading)}</span>
      <span data-testid="refreshing">{String(state.refreshing)}</span>
      <span data-testid="error">
        {state.error instanceof Error
          ? state.error.message
          : state.error === null
            ? "none"
            : "error"}
      </span>
    </div>
  );
}

function expectState({
  data,
  status,
  loading = false,
  refreshing = false,
  error = "none",
}: {
  data: string;
  status: string;
  loading?: boolean;
  refreshing?: boolean;
  error?: string;
}) {
  expect(screen.getByTestId("data")).toHaveTextContent(data);
  expect(screen.getByTestId("status")).toHaveTextContent(status);
  expect(screen.getByTestId("loading")).toHaveTextContent(String(loading));
  expect(screen.getByTestId("refreshing")).toHaveTextContent(String(refreshing));
  expect(screen.getByTestId("error")).toHaveTextContent(error);
}

describe("useResource", () => {
  test("a null key is idle and does not load", () => {
    const load = vi.fn<Loader>();
    render(<ResourceProbe resourceKey={null} load={load} />);

    expectState({ data: "none", status: "idle" });
    expect(load).not.toHaveBeenCalled();
  });

  test("loads an initial key", async () => {
    const request = deferred<string>();
    const load = vi.fn<Loader>(() => request.promise);
    render(<ResourceProbe resourceKey="alpha" load={load} />);

    expectState({ data: "none", status: "loading", loading: true });
    expect(load).toHaveBeenCalledTimes(1);
    expect(load.mock.calls[0]?.[0]).toBe("alpha");

    await act(async () => request.resolve("first"));
    expectState({ data: "first", status: "ready" });
  });

  test("a key change immediately hides prior-key data", async () => {
    const requests = new Map<string, Deferred<string>>();
    const load = vi.fn<Loader>((key) => {
      const request = deferred<string>();
      requests.set(key, request);
      return request.promise;
    });
    const view = render(<ResourceProbe resourceKey="alpha" load={load} />);
    await act(async () => requests.get("alpha")?.resolve("old"));

    view.rerender(<ResourceProbe resourceKey="beta" load={load} />);
    expectState({ data: "none", status: "loading", loading: true });
    await act(async () => requests.get("beta")?.resolve("new"));
    expectState({ data: "new", status: "ready" });
  });

  test("same-key refresh can retain data while reporting refreshing", async () => {
    const requests: Deferred<string>[] = [];
    const load = vi.fn<Loader>(() => {
      const request = deferred<string>();
      requests.push(request);
      return request.promise;
    });
    render(<ResourceProbe resourceKey="alpha" load={load} keepDataOnRefetch />);
    await act(async () => requests[0]?.resolve("cached"));

    let refresh!: Promise<string | undefined>;
    act(() => {
      refresh = currentControls.refetch();
    });
    expectState({ data: "cached", status: "loading", refreshing: true });
    await act(async () => requests[1]?.resolve("fresh"));

    await expect(refresh).resolves.toBe("fresh");
    expectState({ data: "fresh", status: "ready" });
  });

  test("refetch starts exactly one request", async () => {
    const requests: Deferred<string>[] = [];
    const load = vi.fn<Loader>(() => {
      const request = deferred<string>();
      requests.push(request);
      return request.promise;
    });
    render(<ResourceProbe resourceKey="alpha" load={load} />);
    await act(async () => requests[0]?.resolve("initial"));

    act(() => {
      void currentControls.refetch();
    });
    expect(load).toHaveBeenCalledTimes(2);
    await act(async () => requests[1]?.resolve("refetched"));
    expect(load).toHaveBeenCalledTimes(2);
  });

  test("overlapping same-key successes only return and commit the latest result", async () => {
    const requests: Deferred<string>[] = [];
    const load = vi.fn<Loader>(() => {
      const request = deferred<string>();
      requests.push(request);
      return request.promise;
    });
    render(<ResourceProbe resourceKey="alpha" load={load} />);
    await act(async () => requests[0]?.resolve("initial"));

    let stalePromise!: Promise<string | undefined>;
    let latestPromise!: Promise<string | undefined>;
    act(() => {
      stalePromise = currentControls.refetch();
      latestPromise = currentControls.refetch();
    });
    await act(async () => requests[2]?.resolve("latest"));
    await act(async () => requests[1]?.resolve("stale"));

    await expect(latestPromise).resolves.toBe("latest");
    await expect(stalePromise).resolves.toBeUndefined();
    expectState({ data: "latest", status: "ready" });
  });

  test("a later same-key success wins over an earlier error", async () => {
    const requests: Deferred<string>[] = [];
    const load = vi.fn<Loader>(() => {
      const request = deferred<string>();
      requests.push(request);
      return request.promise;
    });
    render(<ResourceProbe resourceKey="alpha" load={load} />);
    await act(async () => requests[0]?.resolve("initial"));

    act(() => {
      void currentControls.refetch();
      void currentControls.refetch();
    });
    await act(async () => requests[2]?.resolve("latest"));
    await act(async () => requests[1]?.reject(new Error("stale error")));

    expectState({ data: "latest", status: "ready" });
  });

  test("a later same-key error wins over an earlier success", async () => {
    const requests: Deferred<string>[] = [];
    const load = vi.fn<Loader>(() => {
      const request = deferred<string>();
      requests.push(request);
      return request.promise;
    });
    render(<ResourceProbe resourceKey="alpha" load={load} />);
    await act(async () => requests[0]?.resolve("initial"));

    act(() => {
      void currentControls.refetch();
      void currentControls.refetch();
    });
    await act(async () => requests[1]?.resolve("stale success"));
    await act(async () => requests[2]?.reject(new Error("latest error")));

    expectState({ data: "none", status: "error", error: "latest error" });
  });

  test("update supports undefined and functional latest-value updates with stable controls", () => {
    const controls = new Set<ResourceControls<string>>();
    const load = vi.fn<Loader>(() => new Promise(() => undefined));
    render(
      <ResourceProbe
        resourceKey="alpha"
        load={load}
        onControls={(nextControls) => controls.add(nextControls)}
      />,
    );

    act(() => currentControls.update(undefined));
    act(() => currentControls.update((value) => `${value ?? "empty"}:one`));
    act(() => currentControls.update((value) => `${value}:two`));

    expect(currentState.data).toBe("empty:one:two");
    expect(controls.size).toBe(1);
  });

  test("a key change aborts work and its stale success promise returns undefined", async () => {
    const requests: Array<{ request: Deferred<string>; signal: AbortSignal }> = [];
    const load = vi.fn<Loader>((_key, signal) => {
      const request = deferred<string>();
      requests.push({ request, signal });
      return request.promise;
    });
    const view = render(<ResourceProbe resourceKey="alpha" load={load} />);
    await act(async () => requests[0]?.request.resolve("initial"));

    let staleResult!: Promise<string | undefined>;
    act(() => {
      staleResult = currentControls.refetch();
    });
    view.rerender(<ResourceProbe resourceKey="beta" load={load} />);
    expect(requests[1]?.signal.aborted).toBe(true);
    await act(async () => requests[2]?.request.resolve("new"));
    await act(async () => requests[1]?.request.resolve("old"));

    await expect(staleResult).resolves.toBeUndefined();
    expectState({ data: "new", status: "ready" });
  });

  test("transitioning to null aborts and clears the active resource", async () => {
    const request = deferred<string>();
    let signal: AbortSignal | undefined;
    const load = vi.fn<Loader>((_key, nextSignal) => {
      signal = nextSignal;
      return request.promise;
    });
    const view = render(<ResourceProbe resourceKey="alpha" load={load} />);

    view.rerender(<ResourceProbe resourceKey={null} load={load} />);
    expect(signal?.aborted).toBe(true);
    expectState({ data: "none", status: "idle" });
    await act(async () => request.resolve("stale"));
    expectState({ data: "none", status: "idle" });
  });

  test("unmount aborts work and AbortError is never exposed", async () => {
    const request = deferred<string>();
    let signal: AbortSignal | undefined;
    const load = vi.fn<Loader>((_key, nextSignal) => {
      signal = nextSignal;
      nextSignal.addEventListener("abort", () => {
        request.reject(new DOMException("Aborted", "AbortError"));
      });
      return request.promise;
    });
    const view = render(<ResourceProbe resourceKey="alpha" load={load} />);

    view.unmount();
    expect(signal?.aborted).toBe(true);
    await act(async () => request.promise.catch(() => undefined));
  });

  test("Strict Mode replay aborts stale setup and only the latest setup commits", async () => {
    const requests: Array<{ request: Deferred<string>; signal: AbortSignal }> = [];
    const load = vi.fn<Loader>((_key, signal) => {
      const request = deferred<string>();
      requests.push({ request, signal });
      return request.promise;
    });
    render(
      <StrictMode>
        <ResourceProbe resourceKey="alpha" load={load} />
      </StrictMode>,
    );

    expect(requests.length).toBeGreaterThanOrEqual(2);
    expect(requests[0]?.signal.aborted).toBe(true);
    const latest = requests.at(-1);
    await act(async () => latest?.request.resolve("current"));
    expectState({ data: "current", status: "ready" });

    await act(async () => {
      for (const entry of requests.slice(0, -1)) entry.request.resolve("stale");
    });
    await waitFor(() => expect(screen.getByTestId("data")).toHaveTextContent("current"));
  });
});
