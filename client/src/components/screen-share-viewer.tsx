import { createEffect, onCleanup, Show } from "solid-js";
import type { RemoteVideoTrack } from "livekit-client";
import { useOptionalVoiceChat } from "../contexts/voice-chat";
import { screenShareDisplayName } from "../voice/screen-share";

function AttachedScreenShareVideo(props: { track: RemoteVideoTrack; label: string }) {
  let videoRef: HTMLVideoElement | undefined;
  let attachedTrack: RemoteVideoTrack | null = null;

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
      class="h-full max-h-80 w-full rounded bg-black object-contain"
      autoplay
      playsinline
      aria-label={props.label}
    />
  );
}

export default function ScreenShareViewer() {
  const voice = useOptionalVoiceChat();
  if (!voice) return null;

  return (
    <Show when={voice.watchingScreenShare()}>
      {(stream) => {
        const sharerName = () => screenShareDisplayName(stream());
        return (
          <section
            class="flex-shrink-0 border-b border-gray-200 bg-gray-950 p-4 text-gray-100"
            role="region"
            aria-label={`Screen share viewer for ${sharerName()}`}
          >
            <div class="mb-3 flex items-center gap-3">
              <div class="min-w-0 flex-1">
                <p class="text-xs uppercase tracking-wide text-gray-400">Watching screen share</p>
                <h2 class="truncate text-lg font-semibold">{sharerName()}'s screen</h2>
              </div>
              <button
                type="button"
                class="rounded bg-gray-800 px-3 py-2 text-sm font-medium text-white hover:bg-gray-700 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-white"
                aria-label={`Stop watching ${sharerName()}'s screen share`}
                onClick={() => void voice.stopWatchingScreenShare()}
              >
                Stop watching
              </button>
            </div>
            <div class="flex min-h-48 items-center justify-center rounded bg-black">
              <Show
                when={voice.watchingScreenShareTrack()}
                fallback={
                  <p class="p-6 text-sm text-gray-300" role="status">
                    Connecting to {sharerName()}'s screen…
                  </p>
                }
              >
                {(track) => (
                  <AttachedScreenShareVideo
                    track={track()}
                    label={`${sharerName()}'s screen share video`}
                  />
                )}
              </Show>
            </div>
          </section>
        );
      }}
    </Show>
  );
}
