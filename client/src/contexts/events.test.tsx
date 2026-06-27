import { render, screen, waitFor } from "@solidjs/testing-library";
import { createSignal, onCleanup, onMount } from "solid-js";
import { afterEach, describe, expect, test, vi } from "vitest";
import { EventsProvider, useEvents } from "./events";
import { makeCameraStream, makeScreenShareStream } from "../test/fixtures";
import { FakeEventSource, latestFakeEventSource, resetFakeEventSources } from "../test/msw/sse";

function ReadStateEventProbe() {
  const events = useEvents();
  const [readState, setReadState] = createSignal("none");
  const [connectedCount, setConnectedCount] = createSignal(0);

  onMount(() => {
    const unsubscribeReadState = events.onReadStateUpdated((summary) => {
      setReadState(`${summary.channel_id}:${summary.last_read_message_id}:${summary.has_unread}`);
    });
    const unsubscribeConnected = events.onConnected(() => {
      setConnectedCount((count) => count + 1);
    });
    onCleanup(() => {
      unsubscribeReadState();
      unsubscribeConnected();
    });
  });

  return (
    <div>
      <p>read state {readState()}</p>
      <p>connected {connectedCount()}</p>
    </div>
  );
}

function MediaEventProbe() {
  const events = useEvents();
  const [screenStarted, setScreenStarted] = createSignal("none");
  const [screenStopped, setScreenStopped] = createSignal("none");
  const [cameraStarted, setCameraStarted] = createSignal("none");
  const [cameraStopped, setCameraStopped] = createSignal("none");

  onMount(() => {
    const unsubscribeScreenStarted = events.onScreenShareStarted((stream) => {
      setScreenStarted(`${stream.channel_id}:${stream.sharer_user_id}:${stream.track_sid}`);
    });
    const unsubscribeScreenStopped = events.onScreenShareStopped((stream) => {
      setScreenStopped(`${stream.channel_id}:${stream.sharer_user_id}:${stream.track_sid}`);
    });
    const unsubscribeCameraStarted = events.onCameraVideoStarted((stream) => {
      setCameraStarted(`${stream.channel_id}:${stream.sharer_user_id}:${stream.track_sid}`);
    });
    const unsubscribeCameraStopped = events.onCameraVideoStopped((stream) => {
      setCameraStopped(`${stream.channel_id}:${stream.sharer_user_id}:${stream.track_sid}`);
    });
    onCleanup(() => {
      unsubscribeScreenStarted();
      unsubscribeScreenStopped();
      unsubscribeCameraStarted();
      unsubscribeCameraStopped();
    });
  });

  return (
    <div>
      <p>screen started {screenStarted()}</p>
      <p>screen stopped {screenStopped()}</p>
      <p>camera started {cameraStarted()}</p>
      <p>camera stopped {cameraStopped()}</p>
    </div>
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
  resetFakeEventSources();
});

describe("EventsProvider read-state events", () => {
  test("dispatches read-state updates and connection notifications", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    render(() => (
      <EventsProvider>
        <ReadStateEventProbe />
      </EventsProvider>
    ));

    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());
    latestFakeEventSource()?.pushConnected();
    expect(await screen.findByText("connected 1")).toBeInTheDocument();
    latestFakeEventSource()?.open();
    expect(await screen.findByText("connected 2")).toBeInTheDocument();

    latestFakeEventSource()?.pushReadStateUpdated({
      channel_id: 10,
      has_unread: false,
      mention_count: 0,
      last_read_created_at: 100,
      last_read_message_id: 20,
      updated_at: 200,
    });
    expect(await screen.findByText("read state 10:20:false")).toBeInTheDocument();
  });
});

describe("EventsProvider media stream events", () => {
  test("dispatches typed screen-share and camera start and stop payloads", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    render(() => (
      <EventsProvider>
        <MediaEventProbe />
      </EventsProvider>
    ));

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
});
