import { useRef } from "react";

import {
  useAfterRenderEffect,
  useComputedValue,
  useSignalState,
  List,
  registerCleanup,
  useMountEffect,
  If,
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
        className={`w-full flex items-center gap-2 px-3 py-1.5 rounded text-sm text-left disabled:opacity-50 ${
          isActive()
            ? "bg-gray-700 text-white font-medium"
            : "text-gray-400 hover:bg-gray-700 hover:text-gray-200"
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
        <If when={isBusy() && voice.activeChannelId() !== props.channel.id}>
          <span className="text-xs text-gray-400" aria-hidden="true">
            …
          </span>
        </If>
      </button>

      <If when={participants().length > 0}>
        <ul
          className="ml-6 mt-1 flex flex-col gap-0.5"
          aria-label={`Participants in ${props.channel.name}`}
        >
          <List each={participants()}>
            {(p) => (
              <li className="flex items-center gap-2 px-2 py-1 text-xs text-gray-300">
                <Avatar
                  url={p.avatar_url}
                  username={p.username}
                  size={18}
                  isSpeaking={speakingIds().has(p.user_id)}
                />
                <span className="truncate">{p.username}</span>
                <If when={p.muted}>
                  <span
                    className="inline-flex h-4 w-4 flex-shrink-0 items-center justify-center rounded bg-gray-900/70 text-red-300"
                    role="img"
                    aria-label={`${p.username} is muted`}
                    title={`${p.username} is muted`}
                  >
                    <MicOffIcon size={12} aria-hidden="true" />
                  </span>
                </If>
                <If when={p.deafened}>
                  <span
                    className="inline-flex h-4 w-4 flex-shrink-0 items-center justify-center rounded bg-gray-900/70 text-red-300"
                    role="img"
                    aria-label={`${p.username} is deafened`}
                    title={`${p.username} is deafened`}
                  >
                    <HeadphoneOffIcon size={12} aria-hidden="true" />
                  </span>
                </If>
                <If when={sharingUserIds().has(p.user_id)}>
                  <span
                    className="inline-flex h-4 w-4 flex-shrink-0 items-center justify-center rounded bg-green-900/60 text-green-300"
                    role="img"
                    aria-label={`${p.username} is sharing screen`}
                    title={`${p.username} is sharing screen`}
                  >
                    <ScreenShareIcon size={12} aria-hidden="true" />
                  </span>
                </If>
                <If when={cameraUserIds().has(p.user_id)}>
                  <span
                    className="inline-flex h-4 w-4 flex-shrink-0 items-center justify-center rounded bg-blue-900/60 text-blue-300"
                    role="img"
                    aria-label={`${p.username} has camera on`}
                    title={`${p.username} has camera on`}
                  >
                    <CameraIcon size={12} aria-hidden="true" />
                  </span>
                </If>
              </li>
            )}
          </List>
        </ul>
      </If>

      <If when={cameraStreams().length > 0 && !isActive()}>
        <section
          className="ml-6 mt-1 mb-1 rounded border border-blue-900/70 bg-blue-950/20 p-2"
          aria-label={`Active cameras in ${props.channel.name}`}
        >
          <div className="flex items-center gap-1 text-[11px] font-medium uppercase tracking-wide text-blue-300">
            <CameraIcon size={12} aria-hidden="true" />
            <span>
              {cameraStreams().length === 1
                ? "1 camera live"
                : `${cameraStreams().length} cameras live`}
            </span>
          </div>
          <p className="mt-1 text-[11px] text-gray-400">Join voice to view cameras.</p>
        </section>
      </If>

      <If when={screenShares().length > 0}>
        <section
          className="ml-6 mt-1 mb-1 rounded border border-green-900/70 bg-green-950/20 p-2"
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
            <List each={screenShares()}>
              {(stream) => {
                const name = screenShareDisplayName(stream);
                return (
                  <li className="flex items-center gap-2 rounded bg-gray-900/60 px-2 py-1 text-xs text-gray-200">
                    <Avatar url={stream.avatar_url} username={name} size={16} />
                    <span className="min-w-0 flex-1 truncate">{name}'s screen</span>
                    <If
                      when={isActive()}
                      fallback={
                        <span className="flex-shrink-0 text-[11px] text-gray-400">Join voice</span>
                      }
                    >
                      <button
                        type="button"
                        className={`flex-shrink-0 rounded bg-green-700 px-2 py-0.5 text-[11px] font-medium text-white hover:bg-green-600 disabled:opacity-50 ${
                          isWatchingStream(stream) ? "bg-gray-700 text-green-200" : ""
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
                        <If when={isWatchingStream(stream)} fallback="Watch">
                          Watching
                        </If>
                      </button>
                    </If>
                  </li>
                );
              }}
            </List>
          </ul>
        </section>
      </If>

      <If when={screenShareAnnouncement()}>
        <p className="sr-only" role="status" aria-live="polite">
          {screenShareAnnouncement()}
        </p>
      </If>
      <If when={cameraAnnouncement()}>
        <p className="sr-only" role="status" aria-live="polite">
          {cameraAnnouncement()}
        </p>
      </If>

      <If when={isActive()}>
        <div
          className="ml-6 mt-1 mb-1 flex items-center gap-1"
          role="group"
          aria-label="Voice controls"
        >
          <button
            type="button"
            className={`p-1.5 rounded hover:bg-gray-700 ${
              voice.isCameraEnabled() ? "text-green-300 bg-gray-700" : "text-gray-300"
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
            <If
              when={voice.isCameraEnabled()}
              fallback={<CameraIcon size={14} aria-hidden="true" />}
            >
              <CameraOffIcon size={14} aria-hidden="true" />
            </If>
          </button>
          <button
            type="button"
            className={`p-1.5 rounded hover:bg-gray-700 ${
              voice.isScreenSharing() ? "text-green-300 bg-gray-700" : "text-gray-300"
            }`}
            aria-pressed={voice.isScreenSharing()}
            aria-label={voice.isScreenSharing() ? "Stop sharing screen" : "Share screen"}
            disabled={voice.isScreenShareStarting()}
            onClick={() => void handleToggleScreenShare()}
          >
            <If
              when={voice.isScreenSharing()}
              fallback={<ScreenShareIcon size={14} aria-hidden="true" />}
            >
              <ScreenShareOffIcon size={14} aria-hidden="true" />
            </If>
          </button>
        </div>
        <If when={voice.isCameraEnabled() || voice.isCameraBusy()}>
          <p className="ml-6 mb-1 text-xs text-green-300" role="status">
            <If
              when={!voice.isCameraBusy()}
              fallback={voice.isCameraEnabled() ? "Stopping camera…" : "Starting camera…"}
            >
              Camera on
            </If>
          </p>
        </If>
        <If when={voice.isScreenSharing()}>
          <p className="ml-6 mb-1 text-xs text-green-300" role="status">
            Sharing screen
          </p>
        </If>
      </If>

      <If when={localError() || voice.lastError()}>
        <p className="ml-6 mb-1 text-xs text-red-400" role="alert">
          {localError() ?? voice.lastError()}
        </p>
      </If>
    </div>
  );
}
