import { useState } from "react";
import { act, screen, waitFor } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import type { CameraStream } from "../api";
import type { RemoteCameraTile } from "../contexts/voice-chat";
import { expectNoA11yViolations } from "../test/a11y";
import { renderNative } from "../test/render";

class FakeRemoteVideoTrack {
  attach = vi.fn((element: HTMLMediaElement) => element);
  detach = vi.fn((element: HTMLMediaElement) => [element]);
}

const mockVoiceState = vi.hoisted(() => ({
  value: null as {
    activeChannelId: number | null;
    remoteCameraTiles: readonly RemoteCameraTile[];
  } | null,
}));

vi.mock("../contexts/voice-chat", () => ({
  useOptionalVoiceChat: () => mockVoiceState.value,
}));

import RemoteCameraTiles from "./remote-camera-tiles";

function makeCamera(overrides: Partial<CameraStream> = {}): CameraStream {
  return {
    channel_id: 42,
    sharer_user_id: 2,
    username: "bob",
    display_name: "Bobby",
    avatar_url: null,
    participant_identity: "2",
    track_sid: "TR_bob_camera",
    track_name: "camera",
    source: "camera",
    started_at: 1,
    ...overrides,
  };
}

function tile(stream: CameraStream, track: FakeRemoteVideoTrack | null): RemoteCameraTile {
  return { stream, track: track as RemoteCameraTile["track"] };
}

function renderTiles(
  initialTiles: readonly RemoteCameraTile[],
  activeChannelId: number | null = 42,
) {
  let setTiles: (tiles: readonly RemoteCameraTile[]) => void = () => undefined;
  let setActive: (channelId: number | null) => void = () => undefined;

  function Harness() {
    const [tiles, setTileState] = useState(initialTiles);
    const [active, setActiveState] = useState(activeChannelId);
    setTiles = setTileState;
    setActive = setActiveState;
    mockVoiceState.value = {
      activeChannelId: active,
      remoteCameraTiles: tiles,
    };
    return <RemoteCameraTiles />;
  }

  const view = renderNative(<Harness />);
  return { ...view, setActive, setTiles };
}

describe("<RemoteCameraTiles>", () => {
  test("renders accessible remote camera tiles, attaches video, and bounds the layout", async () => {
    const track = new FakeRemoteVideoTrack();
    const stream = makeCamera();
    const { container } = renderTiles([
      tile(stream, track),
      tile(
        makeCamera({
          sharer_user_id: 3,
          username: "carol",
          display_name: null,
          participant_identity: "3",
          track_sid: "TR_carol_camera",
          started_at: 2,
        }),
        null,
      ),
    ]);

    const region = await screen.findByRole("region", { name: /remote camera tiles/i });
    expect(region).toHaveTextContent("2 cameras live");
    expect(region.querySelector(".max-h-72.overflow-y-auto")).not.toBeNull();
    const video = screen.getByLabelText("Bobby's camera video") as HTMLVideoElement;
    expect(track.attach).toHaveBeenCalledWith(video);
    expect(screen.getByText("Connecting to carol's camera…")).toHaveAttribute("role", "status");

    await expectNoA11yViolations(container, "remote camera tiles");
  });

  test("hides instead of rendering video for non-participants", async () => {
    const track = new FakeRemoteVideoTrack();
    renderTiles([tile(makeCamera(), track)], null);

    await Promise.resolve();
    expect(screen.queryByRole("region", { name: /remote camera tiles/i })).toBeNull();
    expect(track.attach).not.toHaveBeenCalled();
  });

  test("detaches video when the remote track disappears or the tile is removed", async () => {
    const track = new FakeRemoteVideoTrack();
    const stream = makeCamera();
    const { setTiles } = renderTiles([tile(stream, track)]);

    const video = (await screen.findByLabelText("Bobby's camera video")) as HTMLVideoElement;
    act(() => setTiles([tile(stream, null)]));

    await waitFor(() => expect(screen.queryByLabelText("Bobby's camera video")).toBeNull());
    expect(track.detach).toHaveBeenCalledWith(video);
    expect(screen.getByText("Connecting to Bobby's camera…")).toBeInTheDocument();

    act(() => setTiles([]));

    await waitFor(() =>
      expect(screen.queryByRole("region", { name: /remote camera tiles/i })).toBeNull(),
    );
  });

  test("retains each video element with its stream when camera tiles reorder", async () => {
    const bobTrack = new FakeRemoteVideoTrack();
    const carolTrack = new FakeRemoteVideoTrack();
    const bobStream = makeCamera();
    const carolStream = makeCamera({
      sharer_user_id: 3,
      username: "carol",
      display_name: null,
      participant_identity: "3",
      track_sid: "TR_carol_camera",
      started_at: 2,
    });
    const { setTiles } = renderTiles([tile(bobStream, bobTrack), tile(carolStream, carolTrack)]);

    const bobVideo = (await screen.findByLabelText("Bobby's camera video")) as HTMLVideoElement;
    const carolVideo = screen.getByLabelText("carol's camera video") as HTMLVideoElement;
    act(() => setTiles([tile(carolStream, carolTrack), tile(bobStream, bobTrack)]));

    await waitFor(() => {
      expect(screen.getByLabelText("Bobby's camera video")).toBe(bobVideo);
      expect(screen.getByLabelText("carol's camera video")).toBe(carolVideo);
    });
    expect(bobTrack.attach.mock.calls.every(([element]) => element === bobVideo)).toBe(true);
    expect(carolTrack.attach.mock.calls.every(([element]) => element === carolVideo)).toBe(true);
  });

  test("detaches the previous video when a camera publication resubscribes", async () => {
    const firstTrack = new FakeRemoteVideoTrack();
    const secondTrack = new FakeRemoteVideoTrack();
    const stream = makeCamera();
    const { setTiles } = renderTiles([tile(stream, firstTrack)]);

    const video = (await screen.findByLabelText("Bobby's camera video")) as HTMLVideoElement;
    expect(firstTrack.attach).toHaveBeenCalledWith(video);

    act(() => setTiles([tile(stream, secondTrack)]));

    await waitFor(() => expect(secondTrack.attach).toHaveBeenCalled());
    expect(firstTrack.detach).toHaveBeenCalledWith(video);
  });
});
