import { useRef } from "react";

import { useAfterRenderEffect, registerCleanup, If } from "../hooks/react-state";
import type { LocalVideoTrack } from "livekit-client";
import { useOptionalVoiceChat } from "../contexts/voice-chat";

function AttachedLocalCameraVideo(props: { track: LocalVideoTrack; label: string }) {
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const attachedElementRef = useRef<HTMLVideoElement | null>(null);
  const attachedTrackRef = useRef<LocalVideoTrack | null>(null);

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
      className="aspect-video h-40 rounded bg-black object-cover"
      autoPlay
      muted
      playsInline
      aria-label={props.label}
    />
  );
}

export default function LocalCameraTile() {
  const voice = useOptionalVoiceChat();
  if (!voice) return null;

  return (
    <If when={voice.localCameraTrack()}>
      {(track) => (
        <section
          className="flex-shrink-0 border-b border-gray-200 bg-gray-950 p-4 text-gray-100"
          role="region"
          aria-label="Local camera preview"
        >
          <div className="flex flex-wrap items-center gap-4">
            <AttachedLocalCameraVideo track={track()} label="Your camera video" />
            <div className="min-w-0">
              <p className="text-xs uppercase tracking-wide text-gray-400">Camera on</p>
              <h2 className="text-lg font-semibold">Your camera</h2>
              <p className="text-sm text-gray-300">Only your local preview is shown here.</p>
            </div>
          </div>
        </section>
      )}
    </If>
  );
}
