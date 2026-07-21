import { useState } from "react";
import { act, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import type { LocalVideoTrack } from "livekit-client";
import { renderNative } from "../test/render";
import AttachedVideoTrack from "./attached-video-track";

class FakeVideoTrack {
  attach = vi.fn((element: HTMLMediaElement) => element);
  detach = vi.fn((element: HTMLMediaElement) => element);
}

function asLocalTrack(track: FakeVideoTrack): LocalVideoTrack {
  return track as unknown as LocalVideoTrack;
}

describe("<AttachedVideoTrack>", () => {
  test("preserves video props and balances Strict Mode attachment on unmount", () => {
    const track = new FakeVideoTrack();
    const view = renderNative(
      <AttachedVideoTrack
        track={asLocalTrack(track)}
        className="object-cover camera-preview"
        aria-label="Camera preview"
        autoPlay
        muted
        playsInline
      />,
    );

    const video = screen.getByLabelText("Camera preview") as HTMLVideoElement;
    expect(video).toHaveClass("object-cover", "camera-preview");
    expect(video).toHaveAttribute("autoplay");
    expect(video).toHaveAttribute("muted");
    expect(video).toHaveAttribute("playsinline");
    expect(track.attach).toHaveBeenCalledTimes(2);
    expect(track.attach.mock.calls.every(([element]) => element === video)).toBe(true);

    view.unmount();

    expect(track.detach).toHaveBeenCalledTimes(track.attach.mock.calls.length);
    expect(track.detach.mock.calls.every(([element]) => element === video)).toBe(true);
  });

  test("detaches the exact old pair before attaching a replacement track", () => {
    const firstTrack = new FakeVideoTrack();
    const secondTrack = new FakeVideoTrack();
    let replaceTrack: () => void = () => undefined;

    function Harness() {
      const [track, setTrack] = useState(firstTrack);
      replaceTrack = () => setTrack(secondTrack);
      return (
        <AttachedVideoTrack
          track={asLocalTrack(track)}
          aria-label="Replaceable video"
          autoPlay
          playsInline
        />
      );
    }

    const view = renderNative(<Harness />);
    const video = screen.getByLabelText("Replaceable video") as HTMLVideoElement;
    const events: string[] = [];
    firstTrack.detach.mockImplementation((element) => {
      events.push("detach-first");
      return element;
    });
    secondTrack.attach.mockImplementation((element) => {
      events.push("attach-second");
      return element;
    });

    act(replaceTrack);

    expect(events).toEqual(["detach-first", "attach-second"]);
    expect(firstTrack.detach).toHaveBeenLastCalledWith(video);
    expect(secondTrack.attach).toHaveBeenCalledWith(video);

    view.unmount();
    expect(firstTrack.detach).toHaveBeenCalledTimes(firstTrack.attach.mock.calls.length);
    expect(secondTrack.detach).toHaveBeenCalledTimes(secondTrack.attach.mock.calls.length);
    expect(secondTrack.detach).toHaveBeenLastCalledWith(video);
  });
});
