import { render, screen, waitFor } from "@solidjs/testing-library";
import { createSignal, onCleanup, onMount } from "solid-js";
import { afterEach, describe, expect, test, vi } from "vitest";
import { EventsProvider, useEvents } from "./events";
import { makeScreenShareStream } from "../test/fixtures";
import { FakeEventSource, latestFakeEventSource, resetFakeEventSources } from "../test/msw/sse";

function ScreenShareProbe() {
  const events = useEvents();
  const [started, setStarted] = createSignal("none");
  const [stopped, setStopped] = createSignal("none");

  onMount(() => {
    const unsubscribeStarted = events.onScreenShareStarted((stream) => {
      setStarted(`${stream.channel_id}:${stream.sharer_user_id}:${stream.track_sid}`);
    });
    const unsubscribeStopped = events.onScreenShareStopped((stream) => {
      setStopped(`${stream.channel_id}:${stream.sharer_user_id}:${stream.track_sid}`);
    });
    onCleanup(() => {
      unsubscribeStarted();
      unsubscribeStopped();
    });
  });

  return (
    <div>
      <p>started {started()}</p>
      <p>stopped {stopped()}</p>
    </div>
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
  resetFakeEventSources();
});

describe("EventsProvider screen-share events", () => {
  test("dispatches typed start and stop payloads", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    render(() => (
      <EventsProvider>
        <ScreenShareProbe />
      </EventsProvider>
    ));

    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());
    const stream = makeScreenShareStream({
      channel_id: 42,
      sharer_user_id: 7,
      track_sid: "TR_screen",
    });
    latestFakeEventSource()?.pushScreenShareStarted(stream);
    expect(await screen.findByText("started 42:7:TR_screen")).toBeInTheDocument();

    latestFakeEventSource()?.pushScreenShareStopped({
      channel_id: 42,
      sharer_user_id: 7,
      participant_identity: "7",
      track_sid: "TR_screen",
    });
    expect(await screen.findByText("stopped 42:7:TR_screen")).toBeInTheDocument();
  });
});
