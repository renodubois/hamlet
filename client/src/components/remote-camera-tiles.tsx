import { useOptionalVoiceChat, type RemoteCameraTile } from "../contexts/voice-chat";
import { cameraDisplayName, cameraKey } from "../voice/camera";
import AttachedVideoTrack from "./attached-video-track";
import { CameraIcon } from "./icons";

function RemoteCameraTileCard(props: { tile: RemoteCameraTile }) {
  const name = cameraDisplayName(props.tile.stream);

  return (
    <article
      className="min-w-0 rounded-md border border-border bg-muted p-2"
      aria-label={`${name}'s camera`}
    >
      <div className="flex aspect-video items-center justify-center rounded-md bg-black">
        {props.tile.track ? (
          <AttachedVideoTrack
            key={cameraKey(props.tile.stream)}
            track={props.tile.track}
            className="h-full w-full rounded-md bg-black object-cover"
            autoPlay
            playsInline
            aria-label={`${name}'s camera video`}
          />
        ) : (
          <p className="p-4 text-center text-sm text-white/80" role="status">
            Connecting to {name}'s camera…
          </p>
        )}
      </div>
      <div className="mt-2 min-w-0">
        <p className="truncate text-sm font-medium text-foreground">{name}</p>
        <p className="text-xs text-muted-foreground">Camera</p>
      </div>
    </article>
  );
}

export default function RemoteCameraTiles() {
  const voice = useOptionalVoiceChat();
  if (!voice) return null;

  const tiles = voice.remoteCameraTiles;
  if (voice.activeChannelId == null || tiles.length === 0) return null;

  return (
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
            {tiles.length === 1 ? "1 camera live" : `${tiles.length} cameras live`}
          </h2>
        </div>
      </div>
      <div className="grid max-h-72 grid-cols-1 gap-3 overflow-y-auto sm:grid-cols-2 xl:grid-cols-3">
        {tiles.map((tile) => (
          <RemoteCameraTileCard key={cameraKey(tile.stream)} tile={tile} />
        ))}
      </div>
    </section>
  );
}
