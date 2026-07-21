import { LocalAudioTrack, type LocalTrackPublication, type Room, Track } from "livekit-client";

export interface InputGainPreferences {
  readonly gain: number;
  readonly inputDeviceId: string;
  readonly noiseSuppression: boolean;
}

export interface InputGainHandle {
  readonly sourceStream: MediaStream;
  readonly processedTrack: MediaStreamTrack;
  readonly publication: LocalTrackPublication;
  readonly context: AudioContext;
  /** Alias retained for callers that prefer the Web Audio API's full name. */
  readonly audioContext: AudioContext;
  dispose(): Promise<void>;
}

const DEFAULT_CAPTURE_PREFERENCES: Omit<InputGainPreferences, "gain"> = {
  inputDeviceId: "",
  noiseSuppression: true,
};

function normalizePreferences(preferences: InputGainPreferences | number): InputGainPreferences {
  return typeof preferences === "number"
    ? { ...DEFAULT_CAPTURE_PREFERENCES, gain: preferences }
    : preferences;
}

function stopStream(stream: MediaStream | undefined): void {
  stream?.getTracks().forEach((track) => track.stop());
}

async function closeContext(context: AudioContext | undefined): Promise<void> {
  if (context && context.state !== "closed") await context.close();
}

/**
 * Replace the default mic publication with a gain-adjusted track.
 *
 * The object form keeps capture preferences explicit. The numeric form is an
 * additive compatibility overload for the provider while it is being migrated;
 * it uses browser-default input and noise suppression settings.
 */
export async function applyInputGain(
  room: Room,
  preferences: InputGainPreferences,
): Promise<InputGainHandle | null>;
export async function applyInputGain(room: Room, gain: number): Promise<InputGainHandle | null>;
export async function applyInputGain(
  room: Room,
  preferencesOrGain: InputGainPreferences | number,
): Promise<InputGainHandle | null> {
  const preferences = normalizePreferences(preferencesOrGain);
  if (Math.abs(preferences.gain - 1) < 0.01) return null;

  let context: AudioContext | undefined;
  let sourceStream: MediaStream | undefined;
  let processedTrack: MediaStreamTrack | undefined;
  let localTrack: LocalAudioTrack | undefined;
  let publication: LocalTrackPublication | undefined;
  let replacedDefaultTrack: LocalAudioTrack | undefined;

  const disposeOwnedResources = async (): Promise<void> => {
    if (publication && localTrack) {
      await room.localParticipant.unpublishTrack(localTrack, false).catch(() => undefined);
    }
    try {
      localTrack?.stop();
    } catch {
      // Continue disposing the underlying browser-owned resources.
    }
    try {
      // LocalAudioTrack.stop() normally stops its MediaStreamTrack. Explicitly
      // stop it when a partial/mock wrapper did not, without double-stopping it.
      if (processedTrack?.readyState !== "ended") processedTrack?.stop();
    } catch {
      // Continue disposing the capture stream and context.
    }
    try {
      stopStream(sourceStream);
    } catch {
      // Closing the context must not depend on track cleanup succeeding.
    }
    await closeContext(context).catch(() => undefined);
  };

  try {
    context = new AudioContext();
    sourceStream = await navigator.mediaDevices.getUserMedia({
      audio: {
        deviceId: preferences.inputDeviceId ? { exact: preferences.inputDeviceId } : undefined,
        noiseSuppression: preferences.noiseSuppression,
        echoCancellation: true,
        autoGainControl: true,
      },
    });

    const source = context.createMediaStreamSource(sourceStream);
    const gainNode = context.createGain();
    gainNode.gain.value = preferences.gain;
    const destination = context.createMediaStreamDestination();
    source.connect(gainNode).connect(destination);
    processedTrack = destination.stream.getAudioTracks()[0];
    if (!processedTrack) throw new Error("Input gain processing produced no audio track");

    localTrack = new LocalAudioTrack(processedTrack, undefined, false);
    const existing = room.localParticipant.getTrackPublication(Track.Source.Microphone);
    if (existing?.audioTrack) {
      replacedDefaultTrack = existing.audioTrack;
      await room.localParticipant.unpublishTrack(existing.audioTrack);
    }
    try {
      publication = await room.localParticipant.publishTrack(localTrack, {
        source: Track.Source.Microphone,
      });
    } catch (error) {
      // Do not strand the room without a microphone when replacement publishing
      // fails after the default publication was removed.
      if (replacedDefaultTrack) {
        await room.localParticipant
          .publishTrack(replacedDefaultTrack, { source: Track.Source.Microphone })
          .catch(() => undefined);
        replacedDefaultTrack = undefined;
      }
      throw error;
    }
    replacedDefaultTrack = undefined;

    let disposed = false;
    const ownedContext = context;
    const ownedSourceStream = sourceStream;
    const ownedProcessedTrack = processedTrack;
    const ownedPublication = publication;
    return {
      sourceStream: ownedSourceStream,
      processedTrack: ownedProcessedTrack,
      publication: ownedPublication,
      context: ownedContext,
      audioContext: ownedContext,
      async dispose() {
        if (disposed) return;
        disposed = true;
        await disposeOwnedResources();
      },
    };
  } catch (error) {
    await disposeOwnedResources();
    throw error;
  }
}
