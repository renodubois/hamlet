import type { RemoteAudioTrack } from "livekit-client";

// Chromium's HTMLAudioElement.setSinkId is the only way to route audio to a
// non-default output device from inside a webview. Gated behind a feature
// check so Safari/WebKit fall back to the system default cleanly.
type SinkIdAudio = HTMLAudioElement & { setSinkId?: (id: string) => Promise<void> };

interface OwnedAudioElement {
  readonly track: RemoteAudioTrack;
  readonly element: SinkIdAudio;
  readonly sid: string | undefined;
}

export interface AudioRouter {
  attach(track: RemoteAudioTrack): void;
  detach(track: RemoteAudioTrack): void;
  detachAll(): void;
  setDeafened(deafened: boolean): void;
  setOutputDevice(id: string): void;
}

/**
 * Manages one owned <audio> element per incoming LiveKit track. Track objects
 * are the authoritative identity; a SID additionally prevents duplicate
 * attachment when LiveKit replaces the object for the same publication.
 */
export function createAudioRouter(initialOutputDeviceId = ""): AudioRouter {
  const ownership = new Map<RemoteAudioTrack, OwnedAudioElement>();
  const tracksBySid = new Map<string, RemoteAudioTrack>();
  let deafened = false;
  let outputDeviceId = initialOutputDeviceId;

  function route(element: SinkIdAudio): void {
    if (typeof element.setSinkId !== "function") return;
    try {
      void element.setSinkId(outputDeviceId).catch(() => {
        // Falling back to the browser's current output is safe.
      });
    } catch {
      // Some implementations can reject synchronously; keep current output.
    }
  }

  function removeOwned(owned: OwnedAudioElement): void {
    ownership.delete(owned.track);
    if (owned.sid && tracksBySid.get(owned.sid) === owned.track) {
      tracksBySid.delete(owned.sid);
    }

    // Detach the exact element so other consumers of the track are untouched.
    try {
      owned.track.detach(owned.element);
    } catch {
      // DOM cleanup below still needs to run if LiveKit already detached it.
    }
    try {
      owned.element.pause();
    } catch {
      // Continue severing DOM media ownership.
    }
    try {
      owned.element.srcObject = null;
    } catch {
      // Removing the element still prevents further playback.
    }
    owned.element.remove();
  }

  function ownerForSid(sid: string): OwnedAudioElement | undefined {
    const indexedTrack = tracksBySid.get(sid);
    if (indexedTrack) return ownership.get(indexedTrack);

    // A track can receive its SID after attachment. Reconcile that transition
    // deterministically instead of inventing an unstable fallback key.
    for (const owned of ownership.values()) {
      if (owned.track.sid === sid) {
        tracksBySid.set(sid, owned.track);
        return owned;
      }
    }
    return undefined;
  }

  function attach(track: RemoteAudioTrack): void {
    if (ownership.has(track)) return;

    const sid = track.sid;
    if (sid) {
      const previous = ownerForSid(sid);
      if (previous) removeOwned(previous);
    }

    const element = track.attach() as SinkIdAudio;
    element.autoplay = true;
    element.muted = deafened;
    route(element);
    document.body.appendChild(element);

    const owned = { track, element, sid };
    ownership.set(track, owned);
    if (sid) tracksBySid.set(sid, track);
  }

  function detach(track: RemoteAudioTrack): void {
    const owned = ownership.get(track);
    if (owned) removeOwned(owned);
  }

  function detachAll(): void {
    for (const owned of ownership.values()) removeOwned(owned);
  }

  function setDeafened(value: boolean): void {
    deafened = value;
    ownership.forEach(({ element }) => {
      element.muted = value;
    });
  }

  function setOutputDevice(id: string): void {
    outputDeviceId = id;
    ownership.forEach(({ element }) => route(element));
  }

  return { attach, detach, detachAll, setDeafened, setOutputDevice };
}
