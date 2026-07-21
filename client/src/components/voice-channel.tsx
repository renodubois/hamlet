import { useEffect, useMemo, useReducer, useRef, useState } from "react";
import {
  listCameraStreams,
  listScreenShareStreams,
  listVoiceParticipants,
  type Channel,
  type ScreenShareStream,
} from "../api";
import { useEvents } from "../contexts/events";
import { useVoiceChat } from "../contexts/voice-chat";
import { useVoicePreferences } from "../contexts/voice-preferences";
import Avatar from "./avatar";
import { cameraDisplayName } from "../voice/camera";
import { isSameScreenShare, screenShareDisplayName, screenShareKey } from "../voice/screen-share";
import {
  channelPresenceReducer,
  createChannelPresenceState,
  type ChannelPresenceLiveAction,
} from "../voice/channel-presence";
import {
  CameraIcon,
  CameraOffIcon,
  HeadphoneOffIcon,
  MicOffIcon,
  ScreenShareIcon,
  ScreenShareOffIcon,
  VoiceChannelIcon,
} from "./icons";

type UngeneratedLiveAction = ChannelPresenceLiveAction extends infer Action
  ? Action extends { readonly generation: number }
    ? Omit<Action, "generation">
    : never
  : never;

export default function VoiceChannel(props: { channel: Channel }) {
  const events = useEvents();
  const voice = useVoiceChat();
  const { syncRemoteCameraStreams } = voice;
  const { showSpeakingEverywhere } = useVoicePreferences();
  const voiceRef = useRef(voice);
  voiceRef.current = voice;
  const generationRef = useRef(0);
  const [presence, dispatch] = useReducer(
    channelPresenceReducer,
    props.channel.id,
    createChannelPresenceState,
  );
  const [localError, setLocalError] = useState<string | null>(null);
  const participants = presence.participants;
  const screenShares = presence.screenShares;
  const cameraStreams = presence.cameraStreams;

  const isActive = voice.activeChannelId === props.channel.id;
  const isBusy = voice.isConnecting;
  const isWatchingStream = (stream: ScreenShareStream) => {
    const watched = voice.watchingScreenShare;
    return watched ? isSameScreenShare(watched, stream) : false;
  };

  const speakingIds: ReadonlySet<number> = isActive
    ? voice.speakingUserIds
    : showSpeakingEverywhere
      ? presence.speakingUserIds
      : new Set();

  const sharingUserIds = useMemo(
    () => new Set(screenShares.map((stream) => stream.sharer_user_id)),
    [screenShares],
  );
  const cameraUserIds = useMemo(
    () => new Set(cameraStreams.map((stream) => stream.sharer_user_id)),
    [cameraStreams],
  );
  const screenShareAnnouncement = presence.screenShareAnnouncement
    ? `${screenShareDisplayName(presence.screenShareAnnouncement.stream)} ${
        presence.screenShareAnnouncement.kind === "started"
          ? "started sharing screen"
          : "stopped sharing screen"
      } in ${props.channel.name}.`
    : "";
  const cameraAnnouncement = presence.cameraAnnouncement
    ? `${cameraDisplayName(presence.cameraAnnouncement.stream)} ${
        presence.cameraAnnouncement.kind === "started" ? "turned on camera" : "turned off camera"
      } in ${props.channel.name}.`
    : "";

  useEffect(() => {
    syncRemoteCameraStreams(props.channel.id, isActive ? cameraStreams : []);
    return () => syncRemoteCameraStreams(props.channel.id, []);
  }, [cameraStreams, isActive, props.channel.id, syncRemoteCameraStreams]);

  async function handleToggleJoin() {
    setLocalError(null);
    try {
      if (isActive) {
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
      if (voice.isScreenSharing) {
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
      if (voice.isCameraEnabled) {
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

  useEffect(() => {
    let controller: AbortController | null = null;
    let generation = generationRef.current;

    const startBootstrap = () => {
      controller?.abort();
      controller = new AbortController();
      const request = controller;
      generation = ++generationRef.current;
      const requestGeneration = generation;
      dispatch({
        type: "bootstrapStarted",
        channelId: props.channel.id,
        generation: requestGeneration,
      });
      void Promise.allSettled([
        listVoiceParticipants(props.channel.id, request.signal),
        listScreenShareStreams(props.channel.id, request.signal),
        listCameraStreams(props.channel.id, request.signal),
      ]).then(([participantResult, screenShareResult, cameraResult]) => {
        if (request.signal.aborted) return;
        if (
          participantResult.status === "fulfilled" &&
          screenShareResult.status === "fulfilled" &&
          cameraResult.status === "fulfilled"
        ) {
          dispatch({
            type: "bootstrapSucceeded",
            channelId: props.channel.id,
            generation: requestGeneration,
            participants: participantResult.value,
            screenShares: screenShareResult.value,
            cameraStreams: cameraResult.value,
          });
          return;
        }
        const rejected = [participantResult, screenShareResult, cameraResult].find(
          (result): result is PromiseRejectedResult => result.status === "rejected",
        );
        dispatch({
          type: "bootstrapFailed",
          channelId: props.channel.id,
          generation: requestGeneration,
          error: rejected?.reason ?? new Error("Presence bootstrap failed"),
          participants:
            participantResult.status === "fulfilled" ? participantResult.value : undefined,
          screenShares:
            screenShareResult.status === "fulfilled" ? screenShareResult.value : undefined,
          cameraStreams: cameraResult.status === "fulfilled" ? cameraResult.value : undefined,
        });
      });
    };

    const live = (action: UngeneratedLiveAction) => {
      dispatch({ ...action, generation } as ChannelPresenceLiveAction);
    };

    const unsubJoined = events.onVoiceParticipantJoined((participant) =>
      live({ type: "participantJoined", participant }),
    );
    const unsubLeft = events.onVoiceParticipantLeft((participant) => {
      if (participant.channel_id === props.channel.id) {
        const watched = voiceRef.current.watchingScreenShare;
        if (
          watched?.channel_id === participant.channel_id &&
          watched.sharer_user_id === participant.user_id
        ) {
          void voiceRef.current.stopWatchingScreenShare();
        }
      }
      live({ type: "participantLeft", participant });
    });
    const unsubStatus = events.onVoiceParticipantStatusChanged((status) =>
      live({ type: "participantStatusChanged", status }),
    );
    const unsubSpeaking = events.onVoiceParticipantSpeakingChanged((speaking) =>
      live({ type: "participantSpeakingChanged", speaking }),
    );
    const unsubScreenShareStarted = events.onScreenShareStarted((stream) =>
      live({ type: "screenShareStarted", stream }),
    );
    const unsubScreenShareStopped = events.onScreenShareStopped((stopped) => {
      const watched = voiceRef.current.watchingScreenShare;
      const stoppedWatched = watched && isSameScreenShare(watched, stopped) ? watched : undefined;
      if (stoppedWatched) void voiceRef.current.stopWatchingScreenShare();
      live({ type: "screenShareStopped", stopped, stream: stoppedWatched });
    });
    const unsubCameraStarted = events.onCameraVideoStarted((stream) =>
      live({ type: "cameraStarted", stream }),
    );
    const unsubCameraStopped = events.onCameraVideoStopped((stopped) =>
      live({ type: "cameraStopped", stopped }),
    );
    const unsubConnected = events.onConnected(startBootstrap);

    startBootstrap();
    return () => {
      controller?.abort();
      unsubJoined();
      unsubLeft();
      unsubStatus();
      unsubSpeaking();
      unsubScreenShareStarted();
      unsubScreenShareStopped();
      unsubCameraStarted();
      unsubCameraStopped();
      unsubConnected();
    };
  }, [events, props.channel.id]);

  return (
    <div className="flex flex-col">
      <button
        type="button"
        className={`w-full flex items-center gap-2 px-3 py-1.5 rounded-md text-sm text-left transition-colors disabled:opacity-50 ${
          isActive
            ? "bg-sidebar-accent text-sidebar-accent-foreground font-medium"
            : "text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-foreground"
        }`}
        onClick={() => void handleToggleJoin()}
        disabled={isBusy}
        aria-pressed={isActive}
        aria-label={
          isActive
            ? `Leave voice channel ${props.channel.name}`
            : `Join voice channel ${props.channel.name}`
        }
        draggable={false}
      >
        <VoiceChannelIcon size={14} aria-hidden="true" />
        <span className="flex-1 truncate">{props.channel.name}</span>
        {isBusy && voice.activeChannelId !== props.channel.id ? (
          <span className="text-xs text-sidebar-foreground/70" aria-hidden="true">
            …
          </span>
        ) : null}
      </button>

      {participants.length > 0 ? (
        <ul
          className="ml-6 mt-1 flex flex-col gap-0.5"
          aria-label={`Participants in ${props.channel.name}`}
        >
          {participants.map((p) => (
            <li
              key={p.user_id}
              className="flex items-center gap-2 px-2 py-1 text-xs text-sidebar-foreground"
            >
              <Avatar
                url={p.avatar_url}
                username={p.username}
                size={18}
                isSpeaking={speakingIds.has(p.user_id)}
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
              {sharingUserIds.has(p.user_id) ? (
                <span
                  className="inline-flex h-4 w-4 flex-shrink-0 items-center justify-center rounded bg-green-900/60 text-green-300"
                  role="img"
                  aria-label={`${p.username} is sharing screen`}
                  title={`${p.username} is sharing screen`}
                >
                  <ScreenShareIcon size={12} aria-hidden="true" />
                </span>
              ) : null}
              {cameraUserIds.has(p.user_id) ? (
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

      {cameraStreams.length > 0 && !isActive ? (
        <section
          className="ml-6 mt-1 mb-1 rounded-md border border-sidebar-primary/40 bg-sidebar-primary/10 p-2"
          aria-label={`Active cameras in ${props.channel.name}`}
        >
          <div className="flex items-center gap-1 text-[11px] font-medium uppercase tracking-wide text-sidebar-primary">
            <CameraIcon size={12} aria-hidden="true" />
            <span>
              {cameraStreams.length === 1
                ? "1 camera live"
                : `${cameraStreams.length} cameras live`}
            </span>
          </div>
          <p className="mt-1 text-[11px] text-sidebar-foreground/70">Join voice to view cameras.</p>
        </section>
      ) : null}

      {screenShares.length > 0 ? (
        <section
          className="ml-6 mt-1 mb-1 rounded-md border border-green-900/70 bg-green-950/20 p-2"
          aria-label={`Active screen shares in ${props.channel.name}`}
        >
          <div className="mb-1 flex items-center gap-1 text-[11px] font-medium uppercase tracking-wide text-green-300">
            <ScreenShareIcon size={12} aria-hidden="true" />
            <span>
              {screenShares.length === 1 ? "1 stream live" : `${screenShares.length} streams live`}
            </span>
          </div>
          <ul
            className="flex max-h-28 flex-col gap-1 overflow-y-auto"
            aria-label={`Screen shares in ${props.channel.name}`}
          >
            {screenShares.map((stream) => {
              const name = screenShareDisplayName(stream);
              return (
                <li
                  key={screenShareKey(stream)}
                  className="flex items-center gap-2 rounded-md bg-black/30 px-2 py-1 text-xs text-sidebar-foreground"
                >
                  <Avatar url={stream.avatar_url} username={name} size={16} />
                  <span className="min-w-0 flex-1 truncate">{name}'s screen</span>
                  {isActive ? (
                    <button
                      type="button"
                      className={`flex-shrink-0 rounded-md bg-green-700 px-2 py-0.5 text-[11px] font-medium text-white transition-colors hover:bg-green-600 disabled:opacity-50 ${
                        isWatchingStream(stream) ? "bg-sidebar-accent text-green-200" : ""
                      }`}
                      disabled={isBusy || isWatchingStream(stream)}
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

      {screenShareAnnouncement ? (
        <p className="sr-only" role="status" aria-live="polite">
          {screenShareAnnouncement}
        </p>
      ) : null}
      {cameraAnnouncement ? (
        <p className="sr-only" role="status" aria-live="polite">
          {cameraAnnouncement}
        </p>
      ) : null}

      {isActive ? (
        <>
          <div
            className="ml-6 mt-1 mb-1 flex items-center gap-1"
            role="group"
            aria-label="Voice controls"
          >
            <button
              type="button"
              className={`p-1.5 rounded-md transition-colors hover:bg-sidebar-accent ${
                voice.isCameraEnabled
                  ? "text-green-300 bg-sidebar-accent"
                  : "text-sidebar-foreground"
              }`}
              aria-pressed={voice.isCameraEnabled}
              aria-busy={voice.isCameraBusy}
              aria-label={
                voice.cameraStatus === "stopping"
                  ? "Stopping camera"
                  : voice.cameraStatus === "starting"
                    ? "Starting camera"
                    : voice.isCameraEnabled
                      ? "Turn off camera"
                      : "Turn on camera"
              }
              title={voice.isCameraEnabled ? "Turn off camera" : "Turn on camera"}
              disabled={voice.isCameraBusy}
              onClick={() => void handleToggleCamera()}
            >
              {voice.isCameraEnabled ? (
                <CameraOffIcon size={14} aria-hidden="true" />
              ) : (
                <CameraIcon size={14} aria-hidden="true" />
              )}
            </button>
            <button
              type="button"
              className={`p-1.5 rounded-md transition-colors hover:bg-sidebar-accent ${
                voice.isScreenSharing
                  ? "text-green-300 bg-sidebar-accent"
                  : "text-sidebar-foreground"
              }`}
              aria-pressed={voice.isScreenSharing}
              aria-busy={voice.isScreenShareBusy}
              aria-label={
                voice.screenShareStatus === "stopping"
                  ? "Stopping screen share"
                  : voice.screenShareStatus === "starting"
                    ? "Starting screen share"
                    : voice.isScreenSharing
                      ? "Stop sharing screen"
                      : "Share screen"
              }
              disabled={voice.isScreenShareBusy}
              onClick={() => void handleToggleScreenShare()}
            >
              {voice.isScreenSharing ? (
                <ScreenShareOffIcon size={14} aria-hidden="true" />
              ) : (
                <ScreenShareIcon size={14} aria-hidden="true" />
              )}
            </button>
          </div>
          {voice.isCameraEnabled || voice.isCameraBusy ? (
            <p className="ml-6 mb-1 text-xs text-green-300" role="status">
              {voice.cameraStatus === "stopping"
                ? "Stopping camera…"
                : voice.cameraStatus === "starting"
                  ? "Starting camera…"
                  : "Camera on"}
            </p>
          ) : null}
          {voice.isScreenSharing || voice.isScreenShareBusy ? (
            <p className="ml-6 mb-1 text-xs text-green-300" role="status">
              {voice.screenShareStatus === "stopping"
                ? "Stopping screen share…"
                : voice.screenShareStatus === "starting"
                  ? "Starting screen share…"
                  : "Sharing screen"}
            </p>
          ) : null}
        </>
      ) : null}

      {localError || voice.lastError ? (
        <p className="ml-6 mb-1 text-xs text-destructive" role="alert">
          {localError ?? voice.lastError}
        </p>
      ) : null}
    </div>
  );
}
