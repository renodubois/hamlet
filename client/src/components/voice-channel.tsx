import { useRef } from "react";

import {
  useAfterRenderEffect,
  useComputedValue,
  useSignalState,
  registerCleanup,
  useMountEffect,
  useStaticSignalRerender,
} from "../hooks/react-state";
import {
  listCameraStreams,
  listScreenShareStreams,
  listVoiceParticipants,
  type CameraStream,
  type CameraVideoStopped,
  type Channel,
  type ScreenShareStream,
  type VoiceParticipant,
} from "../api";
import { useEvents } from "../contexts/events";
import { useVoiceChat } from "../contexts/voice-chat";
import Avatar from "./avatar";
import {
  cameraDisplayName,
  cameraKey,
  isSameCameraStream,
  sortCameraStreams,
} from "../voice/camera";
import { showSpeakingIndicatorsEverywhere } from "../voice/settings";
import { isSameScreenShare, screenShareDisplayName, screenShareKey } from "../voice/screen-share";
import {
  CameraIcon,
  CameraOffIcon,
  HeadphoneOffIcon,
  MicOffIcon,
  ScreenShareIcon,
  ScreenShareOffIcon,
  VoiceChannelIcon,
} from "./icons";

/**
 * Everything the sidebar needs to render a single voice channel: the channel
 * row itself, the live participant list beneath it, active screen-share and
 * camera discovery, and (when this is the channel we're connected to)
 * media controls.
 *
 * Participant and media state are fetched once on mount and kept current
 * by listening for SSE events — the server is the source of truth, driven by
 * LiveKit webhooks.
 */
function sortScreenShares(streams: ScreenShareStream[]): ScreenShareStream[] {
  return [...streams].sort((a, b) => {
    if (a.started_at !== b.started_at) return a.started_at - b.started_at;
    if (a.channel_id !== b.channel_id) return a.channel_id - b.channel_id;
    if (a.sharer_user_id !== b.sharer_user_id) return a.sharer_user_id - b.sharer_user_id;
    const participant = a.participant_identity.localeCompare(b.participant_identity);
    if (participant !== 0) return participant;
    return a.track_sid.localeCompare(b.track_sid);
  });
}

export default function VoiceChannel(props: { channel: Channel }) {
  useStaticSignalRerender();
  const events = useEvents();
  const voice = useVoiceChat();
  const [participants, setParticipants] = useSignalState<VoiceParticipant[]>([]);
  const [screenShares, setScreenShares] = useSignalState<ScreenShareStream[]>([]);
  const [cameraStreams, setCameraStreams] = useSignalState<CameraStream[]>([]);
  const [screenShareAnnouncement, setScreenShareAnnouncement] = useSignalState("");
  const [cameraAnnouncement, setCameraAnnouncement] = useSignalState("");
  const [localError, setLocalError] = useSignalState<string | null>(null);
  // Speakers known from SSE broadcasts — used to render the ring when we're
  // NOT connected to this channel. In-channel speakers come from LiveKit via
  // voice.speakingUserIds() instead.
  const [remoteSpeakers, setRemoteSpeakers] = useSignalState<ReadonlySet<number>>(new Set());
  const stoppedScreenShareKeysRef = useRef(new Set<string>());
  const stoppedCameraKeysRef = useRef(new Set<string>());
  const stoppedScreenShareKeys = stoppedScreenShareKeysRef.current;
  const stoppedCameraKeys = stoppedCameraKeysRef.current;

  const isActive = () => voice.activeChannelId() === props.channel.id;
  const isBusy = () => voice.isConnecting();
  const isWatchingStream = (stream: ScreenShareStream) => {
    const watched = voice.watchingScreenShare();
    return watched ? isSameScreenShare(watched, stream) : false;
  };

  const speakingIds = useComputedValue<ReadonlySet<number>>(() => {
    if (isActive()) return voice.speakingUserIds();
    if (showSpeakingIndicatorsEverywhere()) return remoteSpeakers();
    return new Set();
  });

  const sharingUserIds = useComputedValue<ReadonlySet<number>>(
    () => new Set(screenShares().map((stream) => stream.sharer_user_id)),
  );

  const cameraUserIds = useComputedValue<ReadonlySet<number>>(
    () => new Set(cameraStreams().map((stream) => stream.sharer_user_id)),
  );

  useAfterRenderEffect(() => {
    if (!isActive()) return;
    voice.syncRemoteCameraStreams(props.channel.id, cameraStreams());
  });

  async function handleToggleJoin() {
    setLocalError(null);
    try {
      if (isActive()) {
        await voice.leave();
      } else {
        await voice.join(props.channel.id);
      }
    } catch (e) {
      setLocalError(e instanceof Error ? e.message : "Could not connect");
    }
  }

  async function handleToggleScreenShare() {
    setLocalError(null);
    try {
      if (voice.isScreenSharing()) {
        await voice.stopScreenShare();
      } else {
        await voice.startScreenShare();
      }
    } catch (e) {
      setLocalError(e instanceof Error ? e.message : "Could not update screen share");
    }
  }

  async function handleToggleCamera() {
    setLocalError(null);
    try {
      if (voice.isCameraEnabled()) {
        await voice.stopCamera();
      } else {
        await voice.startCamera();
      }
    } catch (e) {
      setLocalError(e instanceof Error ? e.message : "Could not update camera");
    }
  }

  async function handleWatchScreenShare(stream: ScreenShareStream) {
    setLocalError(null);
    try {
      await voice.watchScreenShare(stream);
    } catch (e) {
      setLocalError(e instanceof Error ? e.message : "Could not watch screen share");
    }
  }

  useMountEffect(() => {
    void listVoiceParticipants(props.channel.id)
      .then(setParticipants)
      .catch(() => {
        // Server may not have voice configured (503) — the sidebar still works.
      });
    void listScreenShareStreams(props.channel.id)
      .then((streams) => {
        setScreenShares((prev) => {
          const byKey = new Map(prev.map((stream) => [screenShareKey(stream), stream]));
          for (const stream of streams) {
            if (stoppedScreenShareKeys.has(screenShareKey(stream))) continue;
            byKey.set(screenShareKey(stream), stream);
          }
          return sortScreenShares([...byKey.values()]);
        });
      })
      .catch(() => {
        // Screen-share discovery is best-effort; the rest of voice stays usable.
      });
    void listCameraStreams(props.channel.id)
      .then((streams) => {
        setCameraStreams((prev) => {
          const byKey = new Map(prev.map((stream) => [cameraKey(stream), stream]));
          for (const stream of streams) {
            if (stoppedCameraKeys.has(cameraKey(stream))) continue;
            byKey.set(cameraKey(stream), stream);
          }
          return sortCameraStreams([...byKey.values()]);
        });
      })
      .catch(() => {
        // Camera discovery is best-effort; the rest of voice stays usable.
      });

    const unsubJoined = events.onVoiceParticipantJoined((p) => {
      if (p.channel_id !== props.channel.id) return;
      setParticipants((prev) => {
        const existing = prev.findIndex((x) => x.user_id === p.user_id);
        if (existing < 0) return [...prev, p];
        return prev.map((participant, index) => (index === existing ? p : participant));
      });
    });
    const unsubLeft = events.onVoiceParticipantLeft((p) => {
      if (p.channel_id !== props.channel.id) return;
      setParticipants((prev) => prev.filter((x) => x.user_id !== p.user_id));
      setRemoteSpeakers((prev) => {
        if (!prev.has(p.user_id)) return prev;
        const next = new Set(prev);
        next.delete(p.user_id);
        return next;
      });
      setScreenShares((prev) => {
        const removed = prev.filter((stream) => stream.sharer_user_id === p.user_id);
        removed.forEach((stream) => stoppedScreenShareKeys.add(screenShareKey(stream)));
        const watched = voice.watchingScreenShare();
        if (
          watched &&
          watched.channel_id === p.channel_id &&
          (watched.sharer_user_id === p.user_id ||
            removed.some((stream) => isSameScreenShare(watched, stream)))
        ) {
          void voice.stopWatchingScreenShare();
        }
        return prev.filter((stream) => stream.sharer_user_id !== p.user_id);
      });
      setCameraStreams((prev) => {
        const removed = prev.filter((stream) => stream.sharer_user_id === p.user_id);
        removed.forEach((stream) => stoppedCameraKeys.add(cameraKey(stream)));
        return prev.filter((stream) => stream.sharer_user_id !== p.user_id);
      });
    });
    const unsubSpeaking = events.onVoiceParticipantSpeakingChanged((s) => {
      if (s.channel_id !== props.channel.id) return;
      setRemoteSpeakers((prev) => {
        const has = prev.has(s.user_id);
        if (s.speaking === has) return prev;
        const next = new Set(prev);
        if (s.speaking) next.add(s.user_id);
        else next.delete(s.user_id);
        return next;
      });
    });
    const unsubStatus = events.onVoiceParticipantStatusChanged((s) => {
      if (s.channel_id !== props.channel.id) return;
      setParticipants((prev) =>
        prev.map((participant) =>
          participant.user_id === s.user_id
            ? { ...participant, muted: s.muted, deafened: s.deafened }
            : participant,
        ),
      );
    });
    const unsubScreenShareStarted = events.onScreenShareStarted((stream) => {
      if (stream.channel_id !== props.channel.id) return;
      if (stoppedScreenShareKeys.has(screenShareKey(stream))) return;
      let shouldAnnounce = false;
      setScreenShares((prev) => {
        const existing = prev.findIndex((current) => isSameScreenShare(current, stream));
        if (existing >= 0) return prev;
        shouldAnnounce = true;
        return sortScreenShares([...prev, stream]);
      });
      if (shouldAnnounce) {
        setScreenShareAnnouncement(
          `${screenShareDisplayName(stream)} started sharing screen in ${props.channel.name}.`,
        );
      }
    });
    const unsubScreenShareStopped = events.onScreenShareStopped((stopped) => {
      if (stopped.channel_id !== props.channel.id) return;
      stoppedScreenShareKeys.add(screenShareKey(stopped));
      let removed: ScreenShareStream | undefined;
      setScreenShares((prev) => {
        removed = prev.find((stream) => isSameScreenShare(stream, stopped));
        if (!removed) return prev;
        return prev.filter((stream) => !isSameScreenShare(stream, stopped));
      });
      const watched = voice.watchingScreenShare();
      const stoppedWatched = watched && isSameScreenShare(watched, stopped) ? watched : undefined;
      const ended = removed ?? stoppedWatched;
      if (stoppedWatched) {
        void voice.stopWatchingScreenShare();
      }
      if (ended) {
        setScreenShareAnnouncement(
          `${screenShareDisplayName(ended)} stopped sharing screen in ${props.channel.name}.`,
        );
      }
    });
    const unsubCameraStarted = events.onCameraVideoStarted((stream) => {
      if (stream.channel_id !== props.channel.id) return;
      if (stoppedCameraKeys.has(cameraKey(stream))) return;
      let shouldAnnounce = false;
      setCameraStreams((prev) => {
        const existing = prev.findIndex((current) => isSameCameraStream(current, stream));
        if (existing >= 0) return prev;
        shouldAnnounce = true;
        return sortCameraStreams([...prev, stream]);
      });
      if (shouldAnnounce) {
        setCameraAnnouncement(
          `${cameraDisplayName(stream)} turned on camera in ${props.channel.name}.`,
        );
      }
    });
    const unsubCameraStopped = events.onCameraVideoStopped((stopped: CameraVideoStopped) => {
      if (stopped.channel_id !== props.channel.id) return;
      stoppedCameraKeys.add(cameraKey(stopped));
      let removed: CameraStream | undefined;
      setCameraStreams((prev) => {
        removed = prev.find((stream) => isSameCameraStream(stream, stopped));
        if (!removed) return prev;
        return prev.filter((stream) => !isSameCameraStream(stream, stopped));
      });
      if (removed) {
        setCameraAnnouncement(
          `${cameraDisplayName(removed)} turned off camera in ${props.channel.name}.`,
        );
      }
    });

    registerCleanup(() => {
      unsubJoined();
      unsubLeft();
      unsubSpeaking();
      unsubStatus();
      unsubScreenShareStarted();
      unsubScreenShareStopped();
      unsubCameraStarted();
      unsubCameraStopped();
      voice.syncRemoteCameraStreams(props.channel.id, []);
    });
  });

  return (
    <div className="flex flex-col">
      <button
        type="button"
        className={`w-full flex items-center gap-2 px-3 py-1.5 rounded-md text-sm text-left transition-colors disabled:opacity-50 ${
          isActive()
            ? "bg-sidebar-accent text-sidebar-accent-foreground font-medium"
            : "text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-foreground"
        }`}
        onClick={() => void handleToggleJoin()}
        disabled={isBusy()}
        aria-pressed={isActive()}
        aria-label={
          isActive()
            ? `Leave voice channel ${props.channel.name}`
            : `Join voice channel ${props.channel.name}`
        }
        draggable={false}
      >
        <VoiceChannelIcon size={14} aria-hidden="true" />
        <span className="flex-1 truncate">{props.channel.name}</span>
        {isBusy() && voice.activeChannelId() !== props.channel.id ? (
          <span className="text-xs text-sidebar-foreground/70" aria-hidden="true">
            …
          </span>
        ) : null}
      </button>

      {participants().length > 0 ? (
        <ul
          className="ml-6 mt-1 flex flex-col gap-0.5"
          aria-label={`Participants in ${props.channel.name}`}
        >
          {participants().map((p) => (
            <li
              key={p.user_id}
              className="flex items-center gap-2 px-2 py-1 text-xs text-sidebar-foreground"
            >
              <Avatar
                url={p.avatar_url}
                username={p.username}
                size={18}
                isSpeaking={speakingIds().has(p.user_id)}
              />
              <span className="truncate">{p.username}</span>
              {p.muted ? (
                <span
                  className="inline-flex h-4 w-4 flex-shrink-0 items-center justify-center rounded bg-black/30 text-destructive"
                  role="img"
                  aria-label={`${p.username} is muted`}
                  title={`${p.username} is muted`}
                >
                  <MicOffIcon size={12} aria-hidden="true" />
                </span>
              ) : null}
              {p.deafened ? (
                <span
                  className="inline-flex h-4 w-4 flex-shrink-0 items-center justify-center rounded bg-black/30 text-destructive"
                  role="img"
                  aria-label={`${p.username} is deafened`}
                  title={`${p.username} is deafened`}
                >
                  <HeadphoneOffIcon size={12} aria-hidden="true" />
                </span>
              ) : null}
              {sharingUserIds().has(p.user_id) ? (
                <span
                  className="inline-flex h-4 w-4 flex-shrink-0 items-center justify-center rounded bg-green-900/60 text-green-300"
                  role="img"
                  aria-label={`${p.username} is sharing screen`}
                  title={`${p.username} is sharing screen`}
                >
                  <ScreenShareIcon size={12} aria-hidden="true" />
                </span>
              ) : null}
              {cameraUserIds().has(p.user_id) ? (
                <span
                  className="inline-flex h-4 w-4 flex-shrink-0 items-center justify-center rounded bg-sidebar-primary/20 text-sidebar-primary"
                  role="img"
                  aria-label={`${p.username} has camera on`}
                  title={`${p.username} has camera on`}
                >
                  <CameraIcon size={12} aria-hidden="true" />
                </span>
              ) : null}
            </li>
          ))}
        </ul>
      ) : null}

      {cameraStreams().length > 0 && !isActive() ? (
        <section
          className="ml-6 mt-1 mb-1 rounded-md border border-sidebar-primary/40 bg-sidebar-primary/10 p-2"
          aria-label={`Active cameras in ${props.channel.name}`}
        >
          <div className="flex items-center gap-1 text-[11px] font-medium uppercase tracking-wide text-sidebar-primary">
            <CameraIcon size={12} aria-hidden="true" />
            <span>
              {cameraStreams().length === 1
                ? "1 camera live"
                : `${cameraStreams().length} cameras live`}
            </span>
          </div>
          <p className="mt-1 text-[11px] text-sidebar-foreground/70">Join voice to view cameras.</p>
        </section>
      ) : null}

      {screenShares().length > 0 ? (
        <section
          className="ml-6 mt-1 mb-1 rounded-md border border-green-900/70 bg-green-950/20 p-2"
          aria-label={`Active screen shares in ${props.channel.name}`}
        >
          <div className="mb-1 flex items-center gap-1 text-[11px] font-medium uppercase tracking-wide text-green-300">
            <ScreenShareIcon size={12} aria-hidden="true" />
            <span>
              {screenShares().length === 1
                ? "1 stream live"
                : `${screenShares().length} streams live`}
            </span>
          </div>
          <ul
            className="flex max-h-28 flex-col gap-1 overflow-y-auto"
            aria-label={`Screen shares in ${props.channel.name}`}
          >
            {screenShares().map((stream) => {
              const name = screenShareDisplayName(stream);
              return (
                <li
                  key={screenShareKey(stream)}
                  className="flex items-center gap-2 rounded-md bg-black/30 px-2 py-1 text-xs text-sidebar-foreground"
                >
                  <Avatar url={stream.avatar_url} username={name} size={16} />
                  <span className="min-w-0 flex-1 truncate">{name}'s screen</span>
                  {isActive() ? (
                    <button
                      type="button"
                      className={`flex-shrink-0 rounded-md bg-green-700 px-2 py-0.5 text-[11px] font-medium text-white transition-colors hover:bg-green-600 disabled:opacity-50 ${
                        isWatchingStream(stream) ? "bg-sidebar-accent text-green-200" : ""
                      }`}
                      disabled={isBusy() || isWatchingStream(stream)}
                      aria-label={
                        isWatchingStream(stream)
                          ? `Watching ${name}'s screen share`
                          : `Watch ${name}'s screen share`
                      }
                      aria-pressed={isWatchingStream(stream)}
                      onClick={() => void handleWatchScreenShare(stream)}
                    >
                      {isWatchingStream(stream) ? "Watching" : "Watch"}
                    </button>
                  ) : (
                    <span className="flex-shrink-0 text-[11px] text-sidebar-foreground/70">
                      Join voice
                    </span>
                  )}
                </li>
              );
            })}
          </ul>
        </section>
      ) : null}

      {screenShareAnnouncement() ? (
        <p className="sr-only" role="status" aria-live="polite">
          {screenShareAnnouncement()}
        </p>
      ) : null}
      {cameraAnnouncement() ? (
        <p className="sr-only" role="status" aria-live="polite">
          {cameraAnnouncement()}
        </p>
      ) : null}

      {isActive() ? (
        <>
          <div
            className="ml-6 mt-1 mb-1 flex items-center gap-1"
            role="group"
            aria-label="Voice controls"
          >
            <button
              type="button"
              className={`p-1.5 rounded-md transition-colors hover:bg-sidebar-accent ${
                voice.isCameraEnabled()
                  ? "text-green-300 bg-sidebar-accent"
                  : "text-sidebar-foreground"
              }`}
              aria-pressed={voice.isCameraEnabled()}
              aria-busy={voice.isCameraBusy()}
              aria-label={
                voice.isCameraBusy()
                  ? voice.isCameraEnabled()
                    ? "Stopping camera"
                    : "Starting camera"
                  : voice.isCameraEnabled()
                    ? "Turn off camera"
                    : "Turn on camera"
              }
              title={voice.isCameraEnabled() ? "Turn off camera" : "Turn on camera"}
              disabled={voice.isCameraBusy()}
              onClick={() => void handleToggleCamera()}
            >
              {voice.isCameraEnabled() ? (
                <CameraOffIcon size={14} aria-hidden="true" />
              ) : (
                <CameraIcon size={14} aria-hidden="true" />
              )}
            </button>
            <button
              type="button"
              className={`p-1.5 rounded-md transition-colors hover:bg-sidebar-accent ${
                voice.isScreenSharing()
                  ? "text-green-300 bg-sidebar-accent"
                  : "text-sidebar-foreground"
              }`}
              aria-pressed={voice.isScreenSharing()}
              aria-label={voice.isScreenSharing() ? "Stop sharing screen" : "Share screen"}
              disabled={voice.isScreenShareStarting()}
              onClick={() => void handleToggleScreenShare()}
            >
              {voice.isScreenSharing() ? (
                <ScreenShareOffIcon size={14} aria-hidden="true" />
              ) : (
                <ScreenShareIcon size={14} aria-hidden="true" />
              )}
            </button>
          </div>
          {voice.isCameraEnabled() || voice.isCameraBusy() ? (
            <p className="ml-6 mb-1 text-xs text-green-300" role="status">
              {!voice.isCameraBusy()
                ? "Camera on"
                : voice.isCameraEnabled()
                  ? "Stopping camera…"
                  : "Starting camera…"}
            </p>
          ) : null}
          {voice.isScreenSharing() ? (
            <p className="ml-6 mb-1 text-xs text-green-300" role="status">
              Sharing screen
            </p>
          ) : null}
        </>
      ) : null}

      {localError() || voice.lastError() ? (
        <p className="ml-6 mb-1 text-xs text-destructive" role="alert">
          {localError() ?? voice.lastError()}
        </p>
      ) : null}
    </div>
  );
}
