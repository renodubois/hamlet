import { act, render, screen, waitFor } from "@testing-library/react";
import { useEffect, useState, type ReactNode } from "react";
import { afterEach, describe, expect, test, vi } from "vitest";
import { makeCameraStream, makeScreenShareStream } from "../test/fixtures";
import { FakeEventSource, latestFakeEventSource, resetFakeEventSources } from "../test/msw/sse";
import { EventsProvider, useEvents, type EventsContextValue } from "./events";

const readStateSummary = {
  channel_id: 10,
  has_unread: false,
  mention_count: 0,
  last_read_created_at: 100,
  last_read_message_id: 20,
  updated_at: 200,
};

function ProviderHarness(props: { children: ReactNode }) {
  return <EventsProvider>{props.children}</EventsProvider>;
}

function ReadStateEventProbe() {
  const events = useEvents();
  const [readState, setReadState] = useState("none");
  const [connectedCount, setConnectedCount] = useState(0);

  useEffect(() => {
    const unsubscribeReadState = events.onReadStateUpdated((summary) => {
      setReadState(`${summary.channel_id}:${summary.last_read_message_id}:${summary.has_unread}`);
    });
    const unsubscribeConnected = events.onConnected(() => {
      setConnectedCount((count) => count + 1);
    });
    return () => {
      unsubscribeReadState();
      unsubscribeConnected();
    };
  }, [events]);

  return (
    <div>
      <p>read state {readState}</p>
      <p>connected {connectedCount}</p>
    </div>
  );
}

function MediaEventProbe() {
  const events = useEvents();
  const [screenStarted, setScreenStarted] = useState("none");
  const [screenStopped, setScreenStopped] = useState("none");
  const [cameraStarted, setCameraStarted] = useState("none");
  const [cameraStopped, setCameraStopped] = useState("none");

  useEffect(() => {
    const unsubscribes = [
      events.onScreenShareStarted((stream) => {
        setScreenStarted(`${stream.channel_id}:${stream.sharer_user_id}:${stream.track_sid}`);
      }),
      events.onScreenShareStopped((stream) => {
        setScreenStopped(`${stream.channel_id}:${stream.sharer_user_id}:${stream.track_sid}`);
      }),
      events.onCameraVideoStarted((stream) => {
        setCameraStarted(`${stream.channel_id}:${stream.sharer_user_id}:${stream.track_sid}`);
      }),
      events.onCameraVideoStopped((stream) => {
        setCameraStopped(`${stream.channel_id}:${stream.sharer_user_id}:${stream.track_sid}`);
      }),
    ];
    return () => {
      unsubscribes.forEach((unsubscribe) => unsubscribe());
    };
  }, [events]);

  return (
    <div>
      <p>screen started {screenStarted}</p>
      <p>screen stopped {screenStopped}</p>
      <p>camera started {cameraStarted}</p>
      <p>camera stopped {cameraStopped}</p>
    </div>
  );
}

function SubscriptionProbe(props: {
  subscribe: (events: EventsContextValue) => () => void;
  onUnsubscribe?: (unsubscribe: () => void) => void;
}) {
  const events = useEvents();
  useEffect(() => {
    const unsubscribe = props.subscribe(events);
    props.onUnsubscribe?.(unsubscribe);
    return unsubscribe;
  }, [events, props]);
  return null;
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  resetFakeEventSources();
});

describe("EventsProvider lifecycle", () => {
  test("keeps one EventSource and a stable context value across provider rerenders", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    const values: EventsContextValue[] = [];

    function IdentityProbe() {
      const events = useEvents();
      const [count, setCount] = useState(0);
      values.push(events);
      return <button onClick={() => setCount((value) => value + 1)}>rerender {count}</button>;
    }

    const view = render(
      <ProviderHarness>
        <IdentityProbe />
      </ProviderHarness>,
    );
    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(1));

    act(() => screen.getByRole("button", { name: "rerender 0" }).click());
    expect(screen.getByRole("button", { name: "rerender 1" })).toBeInTheDocument();
    expect(FakeEventSource.instances).toHaveLength(1);
    expect(values.length).toBeGreaterThan(1);
    expect(values.every((value) => value === values[0])).toBe(true);

    const source = FakeEventSource.instances[0];
    view.unmount();
    expect(source?.closed).toBe(true);
    expect(source?.closeCallCount).toBe(1);
    expect(source?.onmessage).toBeNull();
    expect(source?.onopen).toBeNull();
    expect(source?.onerror).toBeNull();
  });

  test("balances EventSources and listeners during Strict Mode effect replay", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    const onReadState = vi.fn();

    const view = render(
      <ProviderHarness>
        <SubscriptionProbe subscribe={(events) => events.onReadStateUpdated(onReadState)} />
      </ProviderHarness>,
      { reactStrictMode: true },
    );

    await waitFor(() => expect(FakeEventSource.instances).toHaveLength(2));
    const [replayedSource, activeSource] = FakeEventSource.instances;
    expect(replayedSource?.closed).toBe(true);
    expect(replayedSource?.closeCallCount).toBe(1);
    expect(replayedSource?.onmessage).toBeNull();
    expect(activeSource?.closed).toBe(false);

    activeSource?.pushReadStateUpdated(readStateSummary);
    expect(onReadState).toHaveBeenCalledTimes(1);

    view.unmount();
    expect(activeSource?.closed).toBe(true);
    expect(activeSource?.closeCallCount).toBe(1);
    expect(activeSource?.onmessage).toBeNull();
    expect(activeSource?.onopen).toBeNull();
    expect(activeSource?.onerror).toBeNull();

    activeSource?.pushReadStateUpdated(readStateSummary);
    expect(onReadState).toHaveBeenCalledTimes(1);
  });

  test("unsubscribe is idempotent", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    const listener = vi.fn();
    let unsubscribe: (() => void) | undefined;

    render(
      <ProviderHarness>
        <SubscriptionProbe
          subscribe={(events) => events.onReadStateUpdated(listener)}
          onUnsubscribe={(next) => {
            unsubscribe = next;
          }}
        />
      </ProviderHarness>,
    );
    await waitFor(() => expect(unsubscribe).toBeDefined());

    unsubscribe?.();
    unsubscribe?.();
    latestFakeEventSource()?.pushReadStateUpdated(readStateSummary);
    expect(listener).not.toHaveBeenCalled();
  });
});

describe("EventsProvider dispatch", () => {
  test("dispatches typed read-state updates", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    render(
      <ProviderHarness>
        <ReadStateEventProbe />
      </ProviderHarness>,
    );

    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());
    latestFakeEventSource()?.pushReadStateUpdated(readStateSummary);
    expect(await screen.findByText("read state 10:20:false")).toBeInTheDocument();
  });

  test("dispatches typed screen-share and camera start and stop payloads", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    render(
      <ProviderHarness>
        <MediaEventProbe />
      </ProviderHarness>,
    );

    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());
    const stream = makeScreenShareStream({
      channel_id: 42,
      sharer_user_id: 7,
      track_sid: "TR_screen",
    });
    latestFakeEventSource()?.pushScreenShareStarted(stream);
    expect(await screen.findByText("screen started 42:7:TR_screen")).toBeInTheDocument();

    latestFakeEventSource()?.pushScreenShareStopped({
      channel_id: 42,
      sharer_user_id: 7,
      participant_identity: "7",
      track_sid: "TR_screen",
    });
    expect(await screen.findByText("screen stopped 42:7:TR_screen")).toBeInTheDocument();

    const camera = makeCameraStream({
      channel_id: 42,
      sharer_user_id: 8,
      track_sid: "TR_camera",
    });
    latestFakeEventSource()?.pushCameraVideoStarted(camera);
    expect(await screen.findByText("camera started 42:8:TR_camera")).toBeInTheDocument();

    latestFakeEventSource()?.pushCameraVideoStopped({
      channel_id: 42,
      sharer_user_id: 8,
      participant_identity: "8",
      track_sid: "TR_camera",
    });
    expect(await screen.findByText("camera stopped 42:8:TR_camera")).toBeInTheDocument();
  });

  test("isolates malformed payloads and continues dispatching", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    const listener = vi.fn();

    render(
      <ProviderHarness>
        <SubscriptionProbe subscribe={(events) => events.onReadStateUpdated(listener)} />
      </ProviderHarness>,
    );
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    latestFakeEventSource()?.pushRaw("not json");
    latestFakeEventSource()?.pushRaw("null");
    latestFakeEventSource()?.pushRaw(JSON.stringify({ kind: "read_state_updated" }));
    latestFakeEventSource()?.pushReadStateUpdated(readStateSummary);

    expect(warn).toHaveBeenCalledTimes(3);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(listener).toHaveBeenCalledWith(readStateSummary);
  });

  test("continues to later listeners when one listener throws", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const laterListener = vi.fn();

    function ThrowingListeners() {
      const events = useEvents();
      useEffect(() => {
        const unsubscribeThrowing = events.onReadStateUpdated(() => {
          throw new Error("listener failed");
        });
        const unsubscribeLater = events.onReadStateUpdated(laterListener);
        return () => {
          unsubscribeThrowing();
          unsubscribeLater();
        };
      }, [events]);
      return null;
    }

    render(
      <ProviderHarness>
        <ThrowingListeners />
      </ProviderHarness>,
    );
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    latestFakeEventSource()?.pushReadStateUpdated(readStateSummary);
    expect(error).toHaveBeenCalledOnce();
    expect(laterListener).toHaveBeenCalledWith(readStateSummary);
  });
});

describe("EventsProvider connection notifications", () => {
  test("dedupes onopen and sentinel evidence within each connection and reports reconnects", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    const connected = vi.fn();

    render(
      <ProviderHarness>
        <SubscriptionProbe subscribe={(events) => events.onConnected(connected)} />
      </ProviderHarness>,
    );
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());
    const source = latestFakeEventSource();

    source?.open();
    source?.pushConnected();
    expect(connected).toHaveBeenCalledTimes(1);

    source?.failConnection();
    source?.pushConnected();
    source?.open();
    expect(connected).toHaveBeenCalledTimes(2);

    source?.failConnection();
    source?.open();
    source?.pushConnected();
    expect(connected).toHaveBeenCalledTimes(3);
  });
});
