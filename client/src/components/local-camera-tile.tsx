import { useOptionalVoiceChat } from "../contexts/voice-chat";
import AttachedVideoTrack from "./attached-video-track";

export default function LocalCameraTile() {
  const voice = useOptionalVoiceChat();
  if (!voice) return null;

  const track = voice.localCameraTrack;
  if (!track) return null;

  return (
    <section
      className="flex-shrink-0 border-b border-border bg-card p-4 text-card-foreground"
      role="region"
      aria-label="Local camera preview"
    >
      <div className="flex flex-wrap items-center gap-4">
        <AttachedVideoTrack
          key="local-camera"
          track={track}
          className="aspect-video h-40 rounded-md bg-black object-cover"
          autoPlay
          muted
          playsInline
          aria-label="Your camera video"
        />
        <div className="min-w-0">
          <p className="text-xs uppercase tracking-wide text-muted-foreground">Camera on</p>
          <h2 className="text-lg font-semibold">Your camera</h2>
          <p className="text-sm text-muted-foreground">Only your local preview is shown here.</p>
        </div>
      </div>
    </section>
  );
}
