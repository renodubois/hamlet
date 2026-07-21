import { afterEach, describe, expect, test, vi } from "vitest";
import type { RemoteAudioTrack } from "livekit-client";
import { createAudioRouter } from "./audio-routing";

interface FakeTrack {
  sid?: string;
  element: HTMLAudioElement;
  pause: ReturnType<typeof vi.fn>;
  attach: ReturnType<typeof vi.fn>;
  detach: ReturnType<typeof vi.fn>;
}

function makeTrack(sid?: string): FakeTrack {
  const element = document.createElement("audio");
  const pause = vi.fn();
  Object.defineProperty(element, "pause", { value: pause, configurable: true });
  const track: FakeTrack = {
    sid,
    element,
    pause,
    attach: vi.fn(() => element),
    detach: vi.fn(() => element),
  };
  return track;
}

function asRemoteTrack(track: FakeTrack): RemoteAudioTrack {
  return track as unknown as RemoteAudioTrack;
}

afterEach(() => {
  document.body.replaceChildren();
});

describe("createAudioRouter", () => {
  test("owns one element per track and detaches idempotently", () => {
    const track = makeTrack("track-1");
    const router = createAudioRouter();

    router.attach(asRemoteTrack(track));
    router.attach(asRemoteTrack(track));

    expect(track.attach).toHaveBeenCalledTimes(1);
    expect(document.body.contains(track.element)).toBe(true);

    router.detach(asRemoteTrack(track));
    router.detach(asRemoteTrack(track));

    expect(track.detach).toHaveBeenCalledTimes(1);
    expect(track.detach).toHaveBeenCalledWith(track.element);
    expect(track.pause).toHaveBeenCalledTimes(1);
    expect(track.element.srcObject).toBeNull();
    expect(document.body.contains(track.element)).toBe(false);
  });

  test("replaces a prior track object with the same stable SID", () => {
    const first = makeTrack("shared-sid");
    const replacement = makeTrack("shared-sid");
    const router = createAudioRouter();

    router.attach(asRemoteTrack(first));
    router.attach(asRemoteTrack(replacement));

    expect(first.detach).toHaveBeenCalledWith(first.element);
    expect(document.body.contains(first.element)).toBe(false);
    expect(replacement.attach).toHaveBeenCalledTimes(1);
    expect(document.body.contains(replacement.element)).toBe(true);

    // A late unsubscribe for the replaced object must not remove its successor.
    router.detach(asRemoteTrack(first));
    expect(document.body.contains(replacement.element)).toBe(true);
  });

  test("uses object identity when a track has no SID", () => {
    const first = makeTrack();
    const second = makeTrack();
    const router = createAudioRouter();

    router.attach(asRemoteTrack(first));
    router.attach(asRemoteTrack(second));

    expect(document.body.querySelectorAll("audio")).toHaveLength(2);
    router.detachAll();
    router.detachAll();
    expect(first.detach).toHaveBeenCalledTimes(1);
    expect(second.detach).toHaveBeenCalledTimes(1);
    expect(document.body.querySelectorAll("audio")).toHaveLength(0);
  });

  test("applies deafen state and reroutes existing and future elements", async () => {
    const first = makeTrack("first");
    const second = makeTrack("second");
    const firstSetSinkId = vi.fn(async () => {});
    const secondSetSinkId = vi.fn(async () => {});
    Object.defineProperty(first.element, "setSinkId", { value: firstSetSinkId });
    Object.defineProperty(second.element, "setSinkId", { value: secondSetSinkId });

    const router = createAudioRouter("speaker-a");
    router.setDeafened(true);
    router.attach(asRemoteTrack(first));
    router.setOutputDevice("speaker-b");
    router.attach(asRemoteTrack(second));
    router.setOutputDevice("");
    await Promise.resolve();

    expect(first.element.muted).toBe(true);
    expect(second.element.muted).toBe(true);
    expect(firstSetSinkId.mock.calls).toEqual([["speaker-a"], ["speaker-b"], [""]]);
    expect(secondSetSinkId.mock.calls).toEqual([["speaker-b"], [""]]);
  });

  test("ignores unsupported or rejected output routing", async () => {
    const track = makeTrack("track");
    Object.defineProperty(track.element, "setSinkId", {
      value: vi.fn(async () => {
        throw new Error("not allowed");
      }),
    });
    const router = createAudioRouter("speaker");

    expect(() => router.attach(asRemoteTrack(track))).not.toThrow();
    router.setOutputDevice("other");
    await Promise.resolve();
    await Promise.resolve();
    expect(document.body.contains(track.element)).toBe(true);
  });
});
