import { describe, expect, test, vi } from "vitest";
import { render, screen, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { expectNoA11yViolations } from "../test/a11y";

class FakeLocalVideoTrack {
  attach = vi.fn((element: HTMLMediaElement) => element);
  detach = vi.fn((element: HTMLMediaElement) => element);
}

const mockVoiceState = vi.hoisted(() => ({
  value: null as {
    localCameraTrack: () => FakeLocalVideoTrack | null;
  } | null,
}));

vi.mock("../contexts/voice-chat", () => ({
  useOptionalVoiceChat: () => mockVoiceState.value,
}));

import LocalCameraTile from "./local-camera-tile";

function setupTile(track = new FakeLocalVideoTrack()) {
  const [localCameraTrack, setLocalCameraTrack] = createSignal<FakeLocalVideoTrack | null>(track);
  mockVoiceState.value = {
    localCameraTrack,
  };
  return { setLocalCameraTrack, track };
}

describe("<LocalCameraTile>", () => {
  test("renders an accessible local camera tile and attaches video", async () => {
    const { track } = setupTile();
    const { container } = render(() => <LocalCameraTile />);

    const region = await screen.findByRole("region", { name: /local camera preview/i });
    expect(region).toHaveTextContent("Your camera");
    const video = screen.getByLabelText("Your camera video") as HTMLVideoElement;
    expect(video).toHaveAttribute("muted");
    expect(track.attach).toHaveBeenCalledWith(video);

    await expectNoA11yViolations(container, "local camera tile");
  });

  test("detaches and hides the local video when camera stops", async () => {
    const { setLocalCameraTrack, track } = setupTile();
    render(() => <LocalCameraTile />);

    const video = (await screen.findByLabelText("Your camera video")) as HTMLVideoElement;
    setLocalCameraTrack(null);

    await waitFor(() => expect(screen.queryByRole("region")).toBeNull());
    expect(track.detach).toHaveBeenCalledWith(video);
  });

  test("detaches the previous local camera track when it changes", async () => {
    const firstTrack = new FakeLocalVideoTrack();
    const secondTrack = new FakeLocalVideoTrack();
    const { setLocalCameraTrack } = setupTile(firstTrack);
    render(() => <LocalCameraTile />);

    const video = (await screen.findByLabelText("Your camera video")) as HTMLVideoElement;
    expect(firstTrack.attach).toHaveBeenCalledWith(video);

    setLocalCameraTrack(secondTrack);

    await waitFor(() => expect(secondTrack.attach).toHaveBeenCalledWith(video));
    expect(firstTrack.detach).toHaveBeenCalledWith(video);
  });
});
