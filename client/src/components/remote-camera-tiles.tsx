import { useRef } from "react";

import { useAfterRenderEffect, List, registerCleanup, If } from "../hooks/react-state";
import type { RemoteVideoTrack } from "livekit-client";
import { useOptionalVoiceChat, type RemoteCameraTile } from "../contexts/voice-chat";
import { cameraDisplayName } from "../voice/camera";
import { CameraIcon } from "./icons";

function AttachedRemoteCameraVideo(props: { track: RemoteVideoTrack; label: string }) {
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const attachedElementRef = useRef<HTMLVideoElement | null>(null);
  const attachedTrackRef = useRef<RemoteVideoTrack | null>(null);

  useAfterRenderEffect(() => {
    const video = videoRef.current;
    const track = props.track;
    const attachedTrack = attachedTrackRef.current;
    if (!video || attachedTrack === track) return;
    if (attachedTrack && attachedElementRef.current) {
      attachedTrack.detach(attachedElementRef.current);
    }
    track.attach(video);
    attachedElementRef.current = video;
    attachedTrackRef.current = track;
  });

  registerCleanup(() => {
    const video = attachedElementRef.current ?? videoRef.current;
    const attachedTrack = attachedTrackRef.current;
    if (attachedTrack && video) {
      attachedTrack.detach(video);
    }
    attachedElementRef.current = null;
    attachedTrackRef.current = null;
  });

  return (
    <video
      ref={(el) => {
        videoRef.current = el;
      }}
      className="h-full w-full rounded-md bg-black object-cover"
      autoPlay
      playsInline
      aria-label={props.label}
    />
  );
}

function RemoteCameraTileCard(props: { tile: RemoteCameraTile }) {
  const name = () => cameraDisplayName(props.tile.stream);

  return (
    <article
      className="min-w-0 rounded-md border border-border bg-muted p-2"
      aria-label={`${name()}'s camera`}
    >
      <div className="flex aspect-video items-center justify-center rounded-md bg-black">
        <If
          when={props.tile.track}
          fallback={
            <p className="p-4 text-center text-sm text-white/80" role="status">
              Connecting to {name()}'s camera…
            </p>
          }
        >
          {(track) => (
            <AttachedRemoteCameraVideo track={track()} label={`${name()}'s camera video`} />
          )}
        </If>
      </div>
      <div className="mt-2 min-w-0">
        <p className="truncate text-sm font-medium text-foreground">{name()}</p>
        <p className="text-xs text-muted-foreground">Camera</p>
      </div>
    </article>
  );
}

export default function RemoteCameraTiles() {
  const voice = useOptionalVoiceChat();
  if (!voice) return null;

  const tiles = () => voice.remoteCameraTiles();

  return (
    <If when={voice.activeChannelId() != null && tiles().length > 0}>
      <section
        className="flex-shrink-0 border-b border-border bg-card p-4 text-card-foreground"
        role="region"
        aria-label="Remote camera tiles"
      >
        <div className="mb-3 flex items-center gap-2">
          <CameraIcon size={16} aria-hidden="true" />
          <div className="min-w-0">
            <p className="text-xs uppercase tracking-wide text-muted-foreground">Cameras</p>
            <h2 className="text-lg font-semibold">
              {tiles().length === 1 ? "1 camera live" : `${tiles().length} cameras live`}
            </h2>
          </div>
        </div>
        <div className="grid max-h-72 grid-cols-1 gap-3 overflow-y-auto sm:grid-cols-2 xl:grid-cols-3">
          <List each={tiles()}>{(tile) => <RemoteCameraTileCard tile={tile} />}</List>
        </div>
      </section>
    </If>
  );
}
