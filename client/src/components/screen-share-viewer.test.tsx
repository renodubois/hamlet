import { useState } from "react";
import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vitest";
import type { ScreenShareStream } from "../api";
import { expectNoA11yViolations } from "../test/a11y";
import { renderNative } from "../test/render";

class FakeRemoteVideoTrack {
  attach = vi.fn((element: HTMLMediaElement) => element);
  detach = vi.fn((element: HTMLMediaElement) => [element]);
}

const mockVoiceState = vi.hoisted(() => ({
  value: null as {
    watchingScreenShare: ScreenShareStream | null;
    watchingScreenShareTrack: FakeRemoteVideoTrack | null;
    stopWatchingScreenShare: ReturnType<typeof vi.fn>;
  } | null,
}));

vi.mock("../contexts/voice-chat", () => ({
  useOptionalVoiceChat: () => mockVoiceState.value,
}));

import ScreenShareViewer from "./screen-share-viewer";

function makeScreenShare(overrides: Partial<ScreenShareStream> = {}): ScreenShareStream {
  return {
    channel_id: 42,
    sharer_user_id: 2,
    username: "bob",
    display_name: "Bobby",
    avatar_url: null,
    participant_identity: "2",
    track_sid: "TR_bob_screen",
    track_name: "screen",
    source: "screen_share",
    started_at: 1,
    ...overrides,
  };
}

function renderViewer(
  initialStream = makeScreenShare(),
  initialTrack = new FakeRemoteVideoTrack(),
) {
  let setStream: (stream: ScreenShareStream | null) => void = () => undefined;
  let setTrack: (track: FakeRemoteVideoTrack | null) => void = () => undefined;
  const stopWatchingScreenShare = vi.fn(async () => {
    setStream(null);
    setTrack(null);
  });

  function Harness() {
    const [stream, setStreamState] = useState<ScreenShareStream | null>(initialStream);
    const [track, setTrackState] = useState<FakeRemoteVideoTrack | null>(initialTrack);
    setStream = setStreamState;
    setTrack = setTrackState;
    mockVoiceState.value = {
      watchingScreenShare: stream,
      watchingScreenShareTrack: track,
      stopWatchingScreenShare,
    };
    return <ScreenShareViewer />;
  }

  return {
    ...renderNative(<Harness />),
    setStream,
    setTrack,
    stopWatchingScreenShare,
    track: initialTrack,
  };
}

describe("<ScreenShareViewer>", () => {
  test("renders a named viewer region, attaches video, and has no accessibility violations", async () => {
    const { container, track } = renderViewer();

    const region = await screen.findByRole("region", { name: /screen share viewer for Bobby/i });
    expect(region).toHaveTextContent("Bobby's screen");
    const video = screen.getByLabelText("Bobby's screen share video") as HTMLVideoElement;
    expect(track.attach).toHaveBeenCalledWith(video);

    await expectNoA11yViolations(container, "screen share viewer");
  });

  test("Stop watching is keyboard reachable and detaches the video without leaving voice", async () => {
    const user = userEvent.setup();
    const { stopWatchingScreenShare, track } = renderViewer();

    const stop = await screen.findByRole("button", {
      name: /stop watching Bobby's screen share/i,
    });
    const video = screen.getByLabelText("Bobby's screen share video") as HTMLVideoElement;

    await user.tab();
    expect(stop).toHaveFocus();
    await user.keyboard("{Enter}");

    await waitFor(() => expect(screen.queryByRole("region")).toBeNull());
    expect(stopWatchingScreenShare).toHaveBeenCalled();
    expect(track.detach).toHaveBeenCalledWith(video);
  });

  test("detaches the previous video track when the subscribed track changes", async () => {
    const firstTrack = new FakeRemoteVideoTrack();
    const secondTrack = new FakeRemoteVideoTrack();
    const { setTrack } = renderViewer(makeScreenShare(), firstTrack);

    const video = (await screen.findByLabelText("Bobby's screen share video")) as HTMLVideoElement;
    expect(firstTrack.attach).toHaveBeenCalledWith(video);

    act(() => setTrack(secondTrack));

    await waitFor(() => expect(secondTrack.attach).toHaveBeenCalledWith(video));
    expect(firstTrack.detach).toHaveBeenCalledWith(video);
  });

  test("uses a new video element when switching watched screen shares", async () => {
    const firstTrack = new FakeRemoteVideoTrack();
    const secondTrack = new FakeRemoteVideoTrack();
    const { setStream, setTrack } = renderViewer(makeScreenShare(), firstTrack);
    const firstVideo = (await screen.findByLabelText(
      "Bobby's screen share video",
    )) as HTMLVideoElement;

    act(() => {
      setStream(
        makeScreenShare({
          sharer_user_id: 3,
          username: "carol",
          display_name: null,
          participant_identity: "3",
          track_sid: "TR_carol_screen",
        }),
      );
      setTrack(secondTrack);
    });

    const secondVideo = (await screen.findByLabelText(
      "carol's screen share video",
    )) as HTMLVideoElement;
    expect(secondVideo).not.toBe(firstVideo);
    expect(firstTrack.detach).toHaveBeenLastCalledWith(firstVideo);
    expect(secondTrack.attach).toHaveBeenCalledWith(secondVideo);
  });

  test("detaches a hidden previous video before rendering a switched stream", async () => {
    const firstTrack = new FakeRemoteVideoTrack();
    const secondTrack = new FakeRemoteVideoTrack();
    const { setStream, setTrack } = renderViewer(makeScreenShare(), firstTrack);

    const firstVideo = (await screen.findByLabelText(
      "Bobby's screen share video",
    )) as HTMLVideoElement;
    act(() => setTrack(null));

    await waitFor(() => {
      expect(screen.queryByLabelText("Bobby's screen share video")).toBeNull();
    });
    expect(firstTrack.detach).toHaveBeenCalledWith(firstVideo);

    act(() => {
      setStream(
        makeScreenShare({
          sharer_user_id: 3,
          username: "carol",
          display_name: null,
          participant_identity: "3",
          track_sid: "TR_carol_screen",
        }),
      );
      setTrack(secondTrack);
    });

    const secondVideo = (await screen.findByLabelText(
      "carol's screen share video",
    )) as HTMLVideoElement;
    expect(secondTrack.attach).toHaveBeenCalledWith(secondVideo);
  });
});
