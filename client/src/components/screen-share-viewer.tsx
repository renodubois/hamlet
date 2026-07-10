import { useRef } from "react";

import { useAfterRenderEffect, registerCleanup, If } from "../hooks/react-state";
import type { RemoteVideoTrack } from "livekit-client";
import { useOptionalVoiceChat } from "../contexts/voice-chat";
import { screenShareDisplayName } from "../voice/screen-share";
import { Button } from "./ui/button";

function AttachedScreenShareVideo(props: { track: RemoteVideoTrack; label: string }) {
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
      className="h-full max-h-80 w-full rounded-md bg-black object-contain"
      autoPlay
      playsInline
      aria-label={props.label}
    />
  );
}

export default function ScreenShareViewer() {
  const voice = useOptionalVoiceChat();
  if (!voice) return null;

  return (
    <If when={voice.watchingScreenShare()}>
      {(stream) => {
        const sharerName = () => screenShareDisplayName(stream());
        return (
          <section
            className="flex-shrink-0 border-b border-border bg-card p-4 text-card-foreground"
            role="region"
            aria-label={`Screen share viewer for ${sharerName()}`}
          >
            <div className="mb-3 flex items-center gap-3">
              <div className="min-w-0 flex-1">
                <p className="text-xs uppercase tracking-wide text-muted-foreground">
                  Watching screen share
                </p>
                <h2 className="truncate text-lg font-semibold">{sharerName()}'s screen</h2>
              </div>
              <Button
                type="button"
                variant="secondary"
                size="lg"
                aria-label={`Stop watching ${sharerName()}'s screen share`}
                onClick={() => void voice.stopWatchingScreenShare()}
              >
                Stop watching
              </Button>
            </div>
            <div className="flex min-h-48 items-center justify-center rounded-md bg-black">
              <If
                when={voice.watchingScreenShareTrack()}
                fallback={
                  <p className="p-6 text-sm text-white/80" role="status">
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
              </If>
            </div>
          </section>
        );
      }}
    </If>
  );
}
