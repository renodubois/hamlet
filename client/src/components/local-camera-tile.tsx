import { createEffect, onCleanup, Show } from "solid-js";
import type { LocalVideoTrack } from "livekit-client";
import { useOptionalVoiceChat } from "../contexts/voice-chat";

function AttachedLocalCameraVideo(props: { track: LocalVideoTrack; label: string }) {
  let videoRef: HTMLVideoElement | undefined;
  let attachedTrack: LocalVideoTrack | null = null;

  createEffect(() => {
    const track = props.track;
    if (!videoRef || attachedTrack === track) return;
    if (attachedTrack) attachedTrack.detach(videoRef);
    track.attach(videoRef);
    attachedTrack = track;
  });

  onCleanup(() => {
    if (attachedTrack && videoRef) {
      attachedTrack.detach(videoRef);
    }
    attachedTrack = null;
  });

  return (
    <video
      ref={(el) => {
        videoRef = el;
      }}
      class="aspect-video h-40 rounded bg-black object-cover"
      autoplay
      muted
      playsinline
      aria-label={props.label}
    />
  );
}

export default function LocalCameraTile() {
  const voice = useOptionalVoiceChat();
  if (!voice) return null;

  return (
    <Show when={voice.localCameraTrack()}>
      {(track) => (
        <section
          class="flex-shrink-0 border-b border-gray-200 bg-gray-950 p-4 text-gray-100"
          role="region"
          aria-label="Local camera preview"
        >
          <div class="flex flex-wrap items-center gap-4">
            <AttachedLocalCameraVideo track={track()} label="Your camera video" />
            <div class="min-w-0">
              <p class="text-xs uppercase tracking-wide text-gray-400">Camera on</p>
              <h2 class="text-lg font-semibold">Your camera</h2>
              <p class="text-sm text-gray-300">Only your local preview is shown here.</p>
            </div>
          </div>
        </section>
      )}
    </Show>
  );
}
