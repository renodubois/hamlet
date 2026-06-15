import { render, screen, waitFor } from "@solidjs/testing-library";
import { createSignal, onCleanup, onMount } from "solid-js";
import { afterEach, describe, expect, test, vi } from "vitest";
import { EventsProvider, useEvents } from "./events";
import { makeCameraStream, makeScreenShareStream } from "../test/fixtures";
import { FakeEventSource, latestFakeEventSource, resetFakeEventSources } from "../test/msw/sse";

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
