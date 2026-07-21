import { useOptionalVoiceChat } from "../contexts/voice-chat";
import { screenShareDisplayName, screenShareKey } from "../voice/screen-share";
import AttachedVideoTrack from "./attached-video-track";
import { Button } from "./ui/button";

export default function ScreenShareViewer() {
  const voice = useOptionalVoiceChat();
  if (!voice) return null;

  const stream = voice.watchingScreenShare;
  if (!stream) return null;

  const sharerName = screenShareDisplayName(stream);
  const track = voice.watchingScreenShareTrack;

  return (
    <section
      className="flex-shrink-0 border-b border-border bg-card p-4 text-card-foreground"
      role="region"
      aria-label={`Screen share viewer for ${sharerName}`}
    >
      <div className="mb-3 flex items-center gap-3">
        <div className="min-w-0 flex-1">
          <p className="text-xs uppercase tracking-wide text-muted-foreground">
            Watching screen share
          </p>
          <h2 className="truncate text-lg font-semibold">{sharerName}'s screen</h2>
        </div>
        <Button
          type="button"
          variant="secondary"
          size="lg"
          aria-label={`Stop watching ${sharerName}'s screen share`}
          onClick={() => void voice.stopWatchingScreenShare()}
        >
          Stop watching
        </Button>
      </div>
      <div className="flex min-h-48 items-center justify-center rounded-md bg-black">
        {track ? (
          <AttachedVideoTrack
            key={screenShareKey(stream)}
            track={track}
            className="h-full max-h-80 w-full rounded-md bg-black object-contain"
            autoPlay
            playsInline
            aria-label={`${sharerName}'s screen share video`}
          />
        ) : (
          <p className="p-6 text-sm text-white/80" role="status">
            Connecting to {sharerName}'s screen…
          </p>
        )}
      </div>
    </section>
  );
}
