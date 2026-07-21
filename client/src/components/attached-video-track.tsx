import { useEffect, useRef, type ComponentPropsWithoutRef } from "react";
import type { LocalVideoTrack, RemoteVideoTrack } from "livekit-client";

type AttachedVideoTrackProps = Omit<ComponentPropsWithoutRef<"video">, "ref"> & {
  track: LocalVideoTrack | RemoteVideoTrack;
};

export default function AttachedVideoTrack({ track, ...videoProps }: AttachedVideoTrackProps) {
  const videoRef = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    track.attach(video);
    return () => {
      track.detach(video);
    };
  }, [track]);

  return <video ref={videoRef} {...videoProps} />;
}
