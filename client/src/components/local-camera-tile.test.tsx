import { useState } from "react";
import { act, screen, waitFor } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import { expectNoA11yViolations } from "../test/a11y";
import { renderNative } from "../test/render";

class FakeLocalVideoTrack {
  attach = vi.fn((element: HTMLMediaElement) => element);
  detach = vi.fn((element: HTMLMediaElement) => element);
}

const mockVoiceState = vi.hoisted(() => ({
  value: null as {
    localCameraTrack: FakeLocalVideoTrack | null;
  } | null,
}));

vi.mock("../contexts/voice-chat", () => ({
  useOptionalVoiceChat: () => mockVoiceState.value,
}));

import LocalCameraTile from "./local-camera-tile";

function renderTile(initialTrack = new FakeLocalVideoTrack()) {
  let setTrack: (track: FakeLocalVideoTrack | null) => void = () => undefined;

  function Harness() {
    const [track, setTrackState] = useState<FakeLocalVideoTrack | null>(initialTrack);
    setTrack = setTrackState;
    mockVoiceState.value = {
      localCameraTrack: track,
    };
    return <LocalCameraTile />;
  }

  return { ...renderNative(<Harness />), setTrack, track: initialTrack };
}

describe("<LocalCameraTile>", () => {
  test("renders an accessible local camera tile and attaches video", async () => {
    const { container, track } = renderTile();

    const region = await screen.findByRole("region", { name: /local camera preview/i });
    expect(region).toHaveTextContent("Your camera");
    const video = screen.getByLabelText("Your camera video") as HTMLVideoElement;
    expect(video).toHaveAttribute("muted");
    expect(track.attach).toHaveBeenCalledWith(video);

    await expectNoA11yViolations(container, "local camera tile");
  });

  test("detaches and hides the local video when camera stops", async () => {
    const { setTrack, track } = renderTile();

    const video = (await screen.findByLabelText("Your camera video")) as HTMLVideoElement;
    act(() => setTrack(null));

    await waitFor(() => expect(screen.queryByRole("region")).toBeNull());
    expect(track.detach).toHaveBeenCalledWith(video);
  });

  test("detaches the previous local camera track when it changes", async () => {
    const firstTrack = new FakeLocalVideoTrack();
    const secondTrack = new FakeLocalVideoTrack();
    const { setTrack } = renderTile(firstTrack);

    const video = (await screen.findByLabelText("Your camera video")) as HTMLVideoElement;
    expect(firstTrack.attach).toHaveBeenCalledWith(video);

    act(() => setTrack(secondTrack));

    await waitFor(() => expect(secondTrack.attach).toHaveBeenCalledWith(video));
    expect(firstTrack.detach).toHaveBeenCalledWith(video);
  });

  test("detaches the local video on unmount", async () => {
    const { track, unmount } = renderTile();
    const video = (await screen.findByLabelText("Your camera video")) as HTMLVideoElement;

    unmount();

    expect(track.detach).toHaveBeenLastCalledWith(video);
    expect(track.detach).toHaveBeenCalledTimes(track.attach.mock.calls.length);
  });
});
