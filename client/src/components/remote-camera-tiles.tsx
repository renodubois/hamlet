import { createEffect, For, onCleanup, Show } from "solid-js";
import type { RemoteVideoTrack } from "livekit-client";
import { useOptionalVoiceChat, type RemoteCameraTile } from "../contexts/voice-chat";
import { cameraDisplayName } from "../voice/camera";
import { CameraIcon } from "./icons";

function AttachedRemoteCameraVideo(props: { track: RemoteVideoTrack; label: string }) {
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
      class="h-full w-full rounded bg-black object-cover"
      autoplay
      playsinline
      aria-label={props.label}
    />
  );
}

function RemoteCameraTileCard(props: { tile: RemoteCameraTile }) {
  const name = () => cameraDisplayName(props.tile.stream);

  return (
    <article
      class="min-w-0 rounded border border-gray-800 bg-gray-900/80 p-2"
      aria-label={`${name()}'s camera`}
    >
      <div class="flex aspect-video items-center justify-center rounded bg-black">
        <Show
          when={props.tile.track}
          fallback={
            <p class="p-4 text-center text-sm text-gray-300" role="status">
              Connecting to {name()}'s camera…
            </p>
          }
        >
          {(track) => (
            <AttachedRemoteCameraVideo track={track()} label={`${name()}'s camera video`} />
          )}
        </Show>
      </div>
      <div class="mt-2 min-w-0">
        <p class="truncate text-sm font-medium text-gray-100">{name()}</p>
        <p class="text-xs text-gray-400">Camera</p>
      </div>
    </article>
  );
}

export default function RemoteCameraTiles() {
  const voice = useOptionalVoiceChat();
  if (!voice) return null;

  const tiles = () => voice.remoteCameraTiles();

  return (
    <Show when={voice.activeChannelId() != null && tiles().length > 0}>
      <section
        class="flex-shrink-0 border-b border-gray-200 bg-gray-950 p-4 text-gray-100"
        role="region"
        aria-label="Remote camera tiles"
      >
        <div class="mb-3 flex items-center gap-2">
          <CameraIcon size={16} aria-hidden="true" />
          <div class="min-w-0">
            <p class="text-xs uppercase tracking-wide text-gray-400">Cameras</p>
            <h2 class="text-lg font-semibold">
              {tiles().length === 1 ? "1 camera live" : `${tiles().length} cameras live`}
            </h2>
          </div>
        </div>
        <div class="grid max-h-72 grid-cols-1 gap-3 overflow-y-auto sm:grid-cols-2 xl:grid-cols-3">
          <For each={tiles()}>{(tile) => <RemoteCameraTileCard tile={tile} />}</For>
        </div>
      </section>
    </Show>
  );
}
