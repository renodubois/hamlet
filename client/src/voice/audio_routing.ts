import type { RemoteAudioTrack } from "livekit-client";
import { VOICE_OUTPUT_STORAGE_KEY } from "./settings";

// Chromium's HTMLAudioElement.setSinkId is the only way to route audio to a
// non-default output device from inside a webview. Gated behind a feature
// check so Safari/WebKit fall back to the system default cleanly.
type SinkIdAudio = HTMLAudioElement & { setSinkId?: (id: string) => Promise<void> };

export interface AudioRouter {
  attach(track: RemoteAudioTrack): void;
  detach(track: RemoteAudioTrack): void;
  detachAll(): void;
  setDeafened(deafened: boolean): void;
}

/**
 * Manages the per-track <audio> elements for incoming voice. Each remote
 * track gets its own element so we can (a) steer per-track output via
 * setSinkId and (b) mute everything together when the user deafens.
 */
export function createAudioRouter(): AudioRouter {
  const audioElements = new Map<string, HTMLAudioElement>();
  let deafened = false;

  function attach(track: RemoteAudioTrack): void {
    const el = track.attach() as SinkIdAudio;
    el.autoplay = true;
    el.muted = deafened;
    const outputId = localStorage.getItem(VOICE_OUTPUT_STORAGE_KEY) ?? "";
    if (outputId && typeof el.setSinkId === "function") {
      el.setSinkId(outputId).catch(() => {
        // Fallback to default output is fine; the user hears audio either way.
      });
    }
    // LiveKit's attach() returns a detached element. Append to body so the
    // browser actually plays it.
    document.body.appendChild(el);
    audioElements.set(track.sid ?? Math.random().toString(), el);
  }

  function detach(track: RemoteAudioTrack): void {
    const key = track.sid ?? "";
    const el = audioElements.get(key);
    if (el) {
      el.pause();
      el.srcObject = null;
      el.remove();
      audioElements.delete(key);
    }
    track.detach().forEach((e) => e.remove());
  }

  function detachAll(): void {
    audioElements.forEach((el) => {
      el.pause();
      el.srcObject = null;
      el.remove();
    });
    audioElements.clear();
  }

  function setDeafened(value: boolean): void {
    deafened = value;
    audioElements.forEach((el) => {
      el.muted = value;
    });
  }

  return { attach, detach, detachAll, setDeafened };
}
