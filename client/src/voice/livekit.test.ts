import { beforeEach, describe, expect, test, vi } from "vitest";
import type { Room } from "livekit-client";

const livekitMock = vi.hoisted(() => {
  const localTracks: Array<{ mediaStreamTrack: MediaStreamTrack; stop: ReturnType<typeof vi.fn> }> =
    [];

  class LocalAudioTrack {
    readonly stop = vi.fn();

    constructor(
      readonly mediaStreamTrack: MediaStreamTrack,
      _constraints?: unknown,
      _userProvidedTrack?: boolean,
    ) {
      localTracks.push(this);
    }
  }

  return {
    LocalAudioTrack,
    localTracks,
    Track: { Source: { Microphone: "microphone" } },
  };
});

vi.mock("livekit-client", () => livekitMock);

import { applyInputGain } from "./livekit";

interface AudioFixture {
  readonly sourceTrack: MediaStreamTrack;
  readonly sourceTrackStop: ReturnType<typeof vi.fn>;
  readonly processedTrack: MediaStreamTrack;
  readonly processedTrackStop: ReturnType<typeof vi.fn>;
  readonly sourceStream: MediaStream;
  readonly context: AudioContext;
  readonly contextClose: ReturnType<typeof vi.fn>;
  readonly getUserMedia: ReturnType<typeof vi.fn>;
}

function audioFixture(hasProcessedTrack = true): AudioFixture {
  const sourceTrackStop = vi.fn();
  const processedTrackStop = vi.fn();
  const sourceTrack = { stop: sourceTrackStop } as unknown as MediaStreamTrack;
  const processedTrack = { stop: processedTrackStop } as unknown as MediaStreamTrack;
  const sourceStream = {
    getTracks: () => [sourceTrack],
    getAudioTracks: () => [sourceTrack],
  } as unknown as MediaStream;
  const destinationStream = {
    getAudioTracks: () => (hasProcessedTrack ? [processedTrack] : []),
  } as unknown as MediaStream;
  const destination = { stream: destinationStream };
  const gainNode = {
    gain: { value: 1 },
    connect: vi.fn(() => destination),
  };
  const source = { connect: vi.fn(() => gainNode) };
  const contextClose = vi.fn(async () => {
    Object.assign(context, { state: "closed" });
  });
  const context = {
    state: "running",
    createMediaStreamSource: vi.fn(() => source),
    createGain: vi.fn(() => gainNode),
    createMediaStreamDestination: vi.fn(() => destination),
    close: contextClose,
  } as unknown as AudioContext;
  const AudioContextConstructor = vi.fn(function FakeAudioContext() {
    return context;
  });
  vi.stubGlobal("AudioContext", AudioContextConstructor);

  const getUserMedia = vi.fn(async () => sourceStream);
  Object.defineProperty(navigator, "mediaDevices", {
    value: { getUserMedia },
    configurable: true,
  });

  return {
    sourceTrack,
    sourceTrackStop,
    processedTrack,
    processedTrackStop,
    sourceStream,
    context,
    contextClose,
    getUserMedia,
  };
}

function roomFixture(options: { publishError?: Error; existingTrack?: object } = {}) {
  const publication = { trackSid: "published-track" };
  const publishTrack = vi.fn(async () => publication);
  if (options.publishError) publishTrack.mockRejectedValueOnce(options.publishError);
  const unpublishTrack = vi.fn(async () => undefined);
  const getTrackPublication = vi.fn(() =>
    options.existingTrack ? { audioTrack: options.existingTrack } : undefined,
  );
  const room = {
    localParticipant: { publishTrack, unpublishTrack, getTrackPublication },
  } as unknown as Room;
  return { room, publication, publishTrack, unpublishTrack, getTrackPublication };
}

beforeEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  livekitMock.localTracks.length = 0;
});

describe("applyInputGain", () => {
  test("does not allocate resources for unity gain", async () => {
    const { room } = roomFixture();
    const getUserMedia = vi.fn();
    Object.defineProperty(navigator, "mediaDevices", {
      value: { getUserMedia },
      configurable: true,
    });

    await expect(
      applyInputGain(room, {
        gain: 1,
        inputDeviceId: "microphone-a",
        noiseSuppression: false,
      }),
    ).resolves.toBeNull();
    expect(getUserMedia).not.toHaveBeenCalled();
  });

  test("passes capture preferences and returns an idempotent disposable owner", async () => {
    const audio = audioFixture();
    const existingTrack = {};
    const room = roomFixture({ existingTrack });

    const handle = await applyInputGain(room.room, {
      gain: 1.5,
      inputDeviceId: "microphone-a",
      noiseSuppression: false,
    });

    expect(audio.getUserMedia).toHaveBeenCalledWith({
      audio: {
        deviceId: { exact: "microphone-a" },
        noiseSuppression: false,
        echoCancellation: true,
        autoGainControl: true,
      },
    });
    expect(room.unpublishTrack).toHaveBeenNthCalledWith(1, existingTrack);
    expect(room.publishTrack).toHaveBeenCalledWith(livekitMock.localTracks[0], {
      source: "microphone",
    });
    expect(handle).toMatchObject({
      sourceStream: audio.sourceStream,
      processedTrack: audio.processedTrack,
      publication: room.publication,
      context: audio.context,
      audioContext: audio.context,
    });

    await handle?.dispose();
    await handle?.dispose();

    expect(room.unpublishTrack).toHaveBeenCalledTimes(2);
    expect(room.unpublishTrack).toHaveBeenNthCalledWith(2, livekitMock.localTracks[0], false);
    expect(livekitMock.localTracks[0]?.stop.mock.calls).toHaveLength(1);
    expect(audio.processedTrackStop).toHaveBeenCalledTimes(1);
    expect(audio.sourceTrackStop).toHaveBeenCalledTimes(1);
    expect(audio.contextClose).toHaveBeenCalledTimes(1);
  });

  test("keeps the numeric compatibility overload free of storage reads", async () => {
    const audio = audioFixture();
    const room = roomFixture();
    const storageRead = vi.spyOn(Storage.prototype, "getItem");

    const handle = await applyInputGain(room.room, 0.5);

    expect(storageRead).not.toHaveBeenCalled();
    expect(audio.getUserMedia).toHaveBeenCalledWith({
      audio: {
        deviceId: undefined,
        noiseSuppression: true,
        echoCancellation: true,
        autoGainControl: true,
      },
    });
    await handle?.dispose();
  });

  test("disposes stream and context when processing creates no audio track", async () => {
    const audio = audioFixture(false);
    const room = roomFixture();

    await expect(
      applyInputGain(room.room, {
        gain: 1.5,
        inputDeviceId: "",
        noiseSuppression: true,
      }),
    ).rejects.toThrow("produced no audio track");

    expect(audio.sourceTrackStop).toHaveBeenCalledTimes(1);
    expect(audio.contextClose).toHaveBeenCalledTimes(1);
    expect(room.publishTrack).not.toHaveBeenCalled();
  });

  test("disposes every created resource when publication fails", async () => {
    const audio = audioFixture();
    const room = roomFixture({ publishError: new Error("publish failed") });

    await expect(
      applyInputGain(room.room, {
        gain: 1.5,
        inputDeviceId: "",
        noiseSuppression: true,
      }),
    ).rejects.toThrow("publish failed");

    expect(livekitMock.localTracks[0]?.stop.mock.calls).toHaveLength(1);
    expect(audio.processedTrackStop).toHaveBeenCalledTimes(1);
    expect(audio.sourceTrackStop).toHaveBeenCalledTimes(1);
    expect(audio.contextClose).toHaveBeenCalledTimes(1);
  });

  test("restores the default microphone when replacement publication fails", async () => {
    const audio = audioFixture();
    const existingTrack = {};
    const room = roomFixture({ existingTrack, publishError: new Error("publish failed") });

    await expect(applyInputGain(room.room, 1.5)).rejects.toThrow("publish failed");

    expect(room.unpublishTrack).toHaveBeenCalledWith(existingTrack);
    expect(room.publishTrack).toHaveBeenNthCalledWith(2, existingTrack, {
      source: "microphone",
    });
    expect(audio.sourceTrackStop).toHaveBeenCalledOnce();
  });

  test("closes the context when capture rejects", async () => {
    const audio = audioFixture();
    const room = roomFixture();
    audio.getUserMedia.mockRejectedValueOnce(new Error("permission denied"));

    await expect(applyInputGain(room.room, 1.5)).rejects.toThrow("permission denied");
    expect(audio.contextClose).toHaveBeenCalledTimes(1);
  });
});
