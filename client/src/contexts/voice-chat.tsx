import { createContext, useContext, useRef, type ReactNode } from "react";

import { useComputedValue, useSignalState, registerCleanup } from "../hooks/react-state";
import {
  type LocalTrackPublication,
  type LocalVideoTrack,
  type Participant,
  ParticipantEvent,
  RemoteAudioTrack,
  type RemoteParticipant,
  type RemoteTrackPublication,
  RemoteVideoTrack,
  Room,
  RoomEvent,
  Track,
  type VideoCaptureOptions,
} from "livekit-client";
import {
  getVoiceToken,
  postVoiceSpeaking,
  postVoiceStatus,
  type CameraStream,
  type ScreenShareStream,
} from "../api";
import {
  VOICE_CAMERA_STORAGE_KEY,
  VOICE_INPUT_STORAGE_KEY,
  getInputGain,
  getNoiseSuppressionEnabled,
} from "../voice/settings";
import { createAudioRouter } from "../voice/audio-routing";
import { applyInputGain } from "../voice/livekit";
import { cameraKey, sortCameraStreams } from "../voice/camera";
import { isSameScreenShare } from "../voice/screen-share";

export interface RemoteCameraTile {
  stream: CameraStream;
  track: RemoteVideoTrack | null;
}

interface VoiceChatContextValue {
  activeChannelId: () => number | null;
  isConnecting: () => boolean;
  isMuted: () => boolean;
  isDeafened: () => boolean;
  isScreenSharing: () => boolean;
  isScreenShareStarting: () => boolean;
  isCameraEnabled: () => boolean;
  isCameraBusy: () => boolean;
  localCameraTrack: () => LocalVideoTrack | null;
  remoteCameraTiles: () => readonly RemoteCameraTile[];
  watchingScreenShare: () => ScreenShareStream | null;
  watchingScreenShareTrack: () => RemoteVideoTrack | null;
  lastError: () => string | null;
  speakingUserIds: () => ReadonlySet<number>;
  join: (channelId: number) => Promise<void>;
  leave: () => Promise<void>;
  toggleMuted: () => Promise<void>;
  toggleDeafened: () => Promise<void>;
  startScreenShare: () => Promise<void>;
  stopScreenShare: () => Promise<void>;
  startCamera: () => Promise<void>;
  stopCamera: () => Promise<void>;
  syncRemoteCameraStreams: (channelId: number, streams: readonly CameraStream[]) => void;
  watchScreenShare: (stream: ScreenShareStream) => Promise<void>;
  stopWatchingScreenShare: () => Promise<void>;
}

const SCREEN_SHARE_CAPTURE_OPTIONS = {
  audio: false,
  video: true,
  systemAudio: "exclude",
} as const;

function formatScreenShareStartError(error: unknown): string {
  const name = error instanceof Error ? error.name : "";
  if (name === "AbortError" || name === "NotAllowedError" || name === "SecurityError") {
    return "Screen share was canceled or denied.";
  }
  if (error instanceof Error && error.message) {
    return `Could not start screen share: ${error.message}`;
  }
  return "Could not start screen share";
}

function formatScreenShareStopError(error: unknown): string {
  if (error instanceof Error && error.message) {
    return `Could not stop screen share: ${error.message}`;
  }
  return "Could not stop screen share";
}

function formatCameraStartError(error: unknown): string {
  const name = error instanceof Error ? error.name : "";
  if (name === "NotAllowedError" || name === "SecurityError") {
    return "Camera permission was denied.";
  }
  if (name === "AbortError") return "Camera start was canceled.";
  if (
    name === "NotFoundError" ||
    name === "DevicesNotFoundError" ||
    name === "OverconstrainedError"
  ) {
    return "No camera device was found.";
  }
  if (error instanceof Error && error.message) {
    return `Could not start camera: ${error.message}`;
  }
  return "Could not start camera";
}

function formatCameraStopError(error: unknown): string {
  if (error instanceof Error && error.message) {
    return `Could not stop camera: ${error.message}`;
  }
  return "Could not stop camera";
}

function getCameraCaptureOptions(): VideoCaptureOptions | undefined {
  const cameraDeviceId = localStorage.getItem(VOICE_CAMERA_STORAGE_KEY) ?? "";
  return cameraDeviceId ? { deviceId: { exact: cameraDeviceId } } : undefined;
}

function isScreenSharePublication(publication: LocalTrackPublication): boolean {
  return (
    publication.source === Track.Source.ScreenShare ||
    publication.track?.source === Track.Source.ScreenShare
  );
}

function isCameraPublication(publication: LocalTrackPublication): boolean {
  return (
    publication.source === Track.Source.Camera || publication.track?.source === Track.Source.Camera
  );
}

function localVideoTrackFromPublication(
  publication: LocalTrackPublication,
): LocalVideoTrack | null {
  const track = publication.videoTrack ?? publication.track;
  if (!track) return null;
  if (track.kind === Track.Kind.Video || track.source === Track.Source.Camera) {
    return track as LocalVideoTrack;
  }
  return null;
}

function stopCameraPublicationTrack(publication: LocalTrackPublication | undefined): void {
  try {
    publication?.track?.stop();
  } catch {
    // Best-effort local cleanup; the caller owns reporting any unpublish error.
  }
}

const VoiceChatContext = createContext<VoiceChatContextValue | undefined>(undefined);

export function VoiceChatProvider(props: { children: ReactNode }) {
  const [activeChannelId, setActiveChannelId] = useSignalState<number | null>(null);
  const [isConnecting, setIsConnecting] = useSignalState(false);
  const [isMuted, setIsMuted] = useSignalState(false);
  const [isDeafened, setIsDeafened] = useSignalState(false);
  const [isScreenSharing, setIsScreenSharing] = useSignalState(false);
  const [isScreenShareStarting, setIsScreenShareStarting] = useSignalState(false);
  const [isCameraEnabled, setIsCameraEnabled] = useSignalState(false);
  const [isCameraBusy, setIsCameraBusy] = useSignalState(false);
  const [localCameraTrack, setLocalCameraTrack] = useSignalState<LocalVideoTrack | null>(null);
  const [remoteCameraStreams, setRemoteCameraStreams] = useSignalState<CameraStream[]>([]);
  const [remoteCameraTracks, setRemoteCameraTracks] = useSignalState<
    ReadonlyMap<string, RemoteVideoTrack>
  >(new Map());
  const remoteCameraTiles = useComputedValue<readonly RemoteCameraTile[]>(() => {
    const tracks = remoteCameraTracks();
    return remoteCameraStreams().map((stream) => ({
      stream,
      track: tracks.get(cameraKey(stream)) ?? null,
    }));
  });
  const [watchingScreenShare, setWatchingScreenShare] = useSignalState<ScreenShareStream | null>(
    null,
  );
  const [watchingScreenShareTrack, setWatchingScreenShareTrack] =
    useSignalState<RemoteVideoTrack | null>(null);
  const [lastError, setLastError] = useSignalState<string | null>(null);
  const [speakingUserIds, setSpeakingUserIds] = useSignalState<ReadonlySet<number>>(new Set());

  const audioRef = useRef<ReturnType<typeof createAudioRouter> | null>(null);
  if (!audioRef.current) audioRef.current = createAudioRouter();
  const roomRef = useRef<Room | null>(null);
  const screenSharePublicationRef = useRef<LocalTrackPublication | undefined>(undefined);
  const cleanupScreenShareTrackEndedRef = useRef<(() => void) | undefined>(undefined);
  const cameraPublicationRef = useRef<LocalTrackPublication | undefined>(undefined);
  const cleanupCameraTrackEndedRef = useRef<(() => void) | undefined>(undefined);
  // Last speaking state we POSTed for the local participant, so we only emit
  // on transitions rather than on every IsSpeakingChanged callback.
  const lastLocalSpeakingRef = useRef(false);
  // user_id → speaking (true). Absent keys are not speaking. Rebuilt into the
  // reactive speakingUserIds signal on every transition.
  const speakingByIdRef = useRef(new Map<number, boolean>());
  // The control mutations touch LiveKit and the server, so serialize them and
  // keep the latest requested state outside React render state. That prevents rapid
  // mute/deafen clicks from posting stale status bits out of order.
  const desiredMutedRef = useRef(false);
  const desiredDeafenedRef = useRef(false);
  const mutedBeforeDeafenRef = useRef<boolean | null>(null);
  const controlUpdateRef = useRef<Promise<void>>(Promise.resolve());
  const audio = audioRef.current;

  function enqueueControlUpdate(update: () => Promise<void>): Promise<void> {
    const run = controlUpdateRef.current.catch(() => {}).then(update);
    controlUpdateRef.current = run.catch(() => {});
    return run;
  }

  function clearScreenShareState(): void {
    cleanupScreenShareTrackEndedRef.current?.();
    cleanupScreenShareTrackEndedRef.current = undefined;
    screenSharePublicationRef.current = undefined;
    setIsScreenSharing(false);
    setIsScreenShareStarting(false);
  }

  function bindScreenSharePublication(publication: LocalTrackPublication, currentRoom: Room): void {
    cleanupScreenShareTrackEndedRef.current?.();
    cleanupScreenShareTrackEndedRef.current = undefined;
    screenSharePublicationRef.current = publication;

    const mediaTrack = publication.track?.mediaStreamTrack;
    if (!mediaTrack) return;

    const handleEnded = () => {
      if (roomRef.current !== currentRoom || screenSharePublicationRef.current !== publication)
        return;
      clearScreenShareState();
      void currentRoom.localParticipant.setScreenShareEnabled(false).catch(() => {});
    };
    mediaTrack.addEventListener("ended", handleEnded, { once: true });
    cleanupScreenShareTrackEndedRef.current = () =>
      mediaTrack.removeEventListener("ended", handleEnded);
  }

  function clearCameraState(): void {
    cleanupCameraTrackEndedRef.current?.();
    cleanupCameraTrackEndedRef.current = undefined;
    cameraPublicationRef.current = undefined;
    setLocalCameraTrack(null);
    setIsCameraEnabled(false);
    setIsCameraBusy(false);
  }

  function bindCameraPublication(publication: LocalTrackPublication, currentRoom: Room): void {
    const track = localVideoTrackFromPublication(publication);
    if (!track) return;

    cleanupCameraTrackEndedRef.current?.();
    cleanupCameraTrackEndedRef.current = undefined;
    cameraPublicationRef.current = publication;
    setLocalCameraTrack(track);
    setIsCameraEnabled(true);

    const mediaTrack = publication.track?.mediaStreamTrack;
    if (!mediaTrack) return;

    const handleEnded = () => {
      if (roomRef.current !== currentRoom || cameraPublicationRef.current !== publication) return;
      void stopLocalCamera(false).catch(() => {});
    };
    mediaTrack.addEventListener("ended", handleEnded, { once: true });
    cleanupCameraTrackEndedRef.current = () => mediaTrack.removeEventListener("ended", handleEnded);
  }

  async function unpublishCameraPublication(
    currentRoom: Room,
    publication: LocalTrackPublication | undefined,
  ): Promise<void> {
    if (publication?.track) {
      await currentRoom.localParticipant.unpublishTrack(publication.track, true);
      return;
    }
    await currentRoom.localParticipant.setCameraEnabled(false);
  }

  function clearWatchedScreenShareState(): void {
    setWatchingScreenShare(null);
    setWatchingScreenShareTrack(null);
  }

  function stopWatchingScreenShareInRoom(currentRoom = roomRef.current): void {
    clearWatchedScreenShareState();
    updateAllRemotePublicationSubscriptions(currentRoom);
  }

  function clearRemoteCameraState(): void {
    setRemoteCameraStreams([]);
    setRemoteCameraTracks(new Map());
  }

  function retainRemoteCameraTracksForStreams(streams: readonly CameraStream[]): void {
    const keys = new Set(streams.map(cameraKey));
    setRemoteCameraTracks((prev) => {
      let changed = false;
      const next = new Map<string, RemoteVideoTrack>();
      prev.forEach((track, key) => {
        if (keys.has(key)) next.set(key, track);
        else changed = true;
      });
      return changed ? next : prev;
    });
  }

  function setRemoteCameraTrack(stream: CameraStream, track: RemoteVideoTrack): void {
    const key = cameraKey(stream);
    setRemoteCameraTracks((prev) => {
      if (prev.get(key) === track) return prev;
      const next = new Map(prev);
      next.set(key, track);
      return next;
    });
  }

  function clearRemoteCameraTracksForPublication(
    participant: RemoteParticipant,
    publication: RemoteTrackPublication,
  ): void {
    const keys = remoteCameraStreams()
      .filter((stream) => doesPublicationMatchCamera(participant, publication, stream))
      .map(cameraKey);
    if (keys.length === 0) return;
    const staleKeys = new Set(keys);
    setRemoteCameraTracks((prev) => {
      let changed = false;
      const next = new Map(prev);
      staleKeys.forEach((key) => {
        changed = next.delete(key) || changed;
      });
      return changed ? next : prev;
    });
  }

  function clearRemoteCameraTracksForTrack(track: RemoteVideoTrack): void {
    setRemoteCameraTracks((prev) => {
      let changed = false;
      const next = new Map<string, RemoteVideoTrack>();
      prev.forEach((currentTrack, key) => {
        if (currentTrack === track) changed = true;
        else next.set(key, currentTrack);
      });
      return changed ? next : prev;
    });
  }

  function normalizeRemoteCameraStreams(
    channelId: number,
    streams: readonly CameraStream[],
    currentRoom: Room,
  ): CameraStream[] {
    const localIdentity = currentRoom.localParticipant.identity;
    const byKey = new Map<string, CameraStream>();
    for (const stream of streams) {
      if (stream.channel_id !== channelId) continue;
      if (stream.source !== Track.Source.Camera) continue;
      if (localIdentity && stream.participant_identity === localIdentity) continue;
      byKey.set(cameraKey(stream), stream);
    }
    return sortCameraStreams([...byKey.values()]);
  }

  function syncRemoteCameraStreams(channelId: number, streams: readonly CameraStream[]): void {
    const currentRoom = roomRef.current;
    if (!currentRoom || activeChannelId() !== channelId) return;
    const normalized = normalizeRemoteCameraStreams(channelId, streams, currentRoom);
    setRemoteCameraStreams(normalized);
    retainRemoteCameraTracksForStreams(normalized);
    updateAllRemotePublicationSubscriptions(currentRoom);
  }

  function removeRemoteCameraStreamsForParticipant(participantIdentity: string): void {
    const next = remoteCameraStreams().filter(
      (stream) => stream.participant_identity !== participantIdentity,
    );
    if (next.length === remoteCameraStreams().length) return;
    setRemoteCameraStreams(next);
    retainRemoteCameraTracksForStreams(next);
  }

  function isRemoteMicrophonePublication(publication: RemoteTrackPublication): boolean {
    return publication.kind === Track.Kind.Audio && publication.source === Track.Source.Microphone;
  }

  function isRemoteScreenShareVideoPublication(publication: RemoteTrackPublication): boolean {
    return publication.kind === Track.Kind.Video && publication.source === Track.Source.ScreenShare;
  }

  function isRemoteCameraVideoPublication(publication: RemoteTrackPublication): boolean {
    return publication.kind === Track.Kind.Video && publication.source === Track.Source.Camera;
  }

  function doesPublicationMatchScreenShare(
    participant: RemoteParticipant,
    publication: RemoteTrackPublication,
    stream: ScreenShareStream,
  ): boolean {
    return (
      participant.identity === stream.participant_identity &&
      publication.trackSid === stream.track_sid
    );
  }

  function doesPublicationMatchCamera(
    participant: RemoteParticipant,
    publication: RemoteTrackPublication,
    stream: CameraStream,
  ): boolean {
    return (
      participant.identity === stream.participant_identity &&
      publication.trackSid === stream.track_sid
    );
  }

  function findRemoteCameraStream(
    participant: RemoteParticipant,
    publication: RemoteTrackPublication,
  ): CameraStream | null {
    return (
      remoteCameraStreams().find((stream) =>
        doesPublicationMatchCamera(participant, publication, stream),
      ) ?? null
    );
  }

  function updateRemotePublicationSubscription(
    publication: RemoteTrackPublication,
    participant: RemoteParticipant,
  ): void {
    if (isRemoteMicrophonePublication(publication)) {
      publication.setSubscribed(true);
      return;
    }

    if (isRemoteScreenShareVideoPublication(publication)) {
      const watched = watchingScreenShare();
      const shouldWatch = Boolean(
        watched && doesPublicationMatchScreenShare(participant, publication, watched),
      );
      publication.setEnabled(shouldWatch);
      publication.setSubscribed(shouldWatch);
      if (shouldWatch && publication.track instanceof RemoteVideoTrack) {
        setWatchingScreenShareTrack(publication.track);
      }
      return;
    }

    if (!isRemoteCameraVideoPublication(publication)) return;

    const cameraStream = findRemoteCameraStream(participant, publication);
    const shouldSubscribe = cameraStream != null;
    publication.setEnabled(shouldSubscribe);
    publication.setSubscribed(shouldSubscribe);
    if (cameraStream && publication.track instanceof RemoteVideoTrack) {
      setRemoteCameraTrack(cameraStream, publication.track);
    } else if (!cameraStream) {
      clearRemoteCameraTracksForPublication(participant, publication);
    }
  }

  function updateAllRemotePublicationSubscriptions(currentRoom = roomRef.current): void {
    currentRoom?.remoteParticipants.forEach((participant) => {
      participant.trackPublications.forEach((publication) => {
        updateRemotePublicationSubscription(publication, participant);
      });
    });
  }

  function findScreenSharePublication(
    currentRoom: Room,
    stream: ScreenShareStream,
  ): {
    participant: RemoteParticipant;
    publication: RemoteTrackPublication;
  } | null {
    const participant = currentRoom.remoteParticipants.get(stream.participant_identity);
    const publication = participant?.trackPublications.get(stream.track_sid);
    if (!participant || !publication || !isRemoteScreenShareVideoPublication(publication)) {
      return null;
    }
    return { participant, publication };
  }

  async function stopLocalScreenShare(reportErrors: boolean): Promise<void> {
    const currentRoom = roomRef.current;
    const existingPublication = currentRoom?.localParticipant.getTrackPublication(
      Track.Source.ScreenShare,
    );
    const shouldStop =
      isScreenSharing() ||
      isScreenShareStarting() ||
      screenSharePublicationRef.current != null ||
      existingPublication != null;

    clearScreenShareState();
    if (!currentRoom || !shouldStop) return;

    try {
      await currentRoom.localParticipant.setScreenShareEnabled(false);
    } catch (e) {
      if (reportErrors) {
        const message = formatScreenShareStopError(e);
        setLastError(message);
        throw new Error(message);
      }
    }
  }

  async function startScreenShare(): Promise<void> {
    const currentRoom = roomRef.current;
    if (!currentRoom || activeChannelId() == null) {
      const message = "Join a voice channel before sharing your screen.";
      setLastError(message);
      throw new Error(message);
    }
    if (isScreenShareStarting()) return;

    const existingPublication = currentRoom.localParticipant.getTrackPublication(
      Track.Source.ScreenShare,
    );
    if (isScreenSharing() || screenSharePublicationRef.current || existingPublication) {
      if (existingPublication && isScreenSharePublication(existingPublication)) {
        bindScreenSharePublication(existingPublication, currentRoom);
        setIsScreenSharing(true);
      }
      const message = "Stop your current screen share before starting another.";
      setLastError(message);
      throw new Error(message);
    }

    setLastError(null);
    setIsScreenShareStarting(true);
    try {
      const publication = await currentRoom.localParticipant.setScreenShareEnabled(
        true,
        SCREEN_SHARE_CAPTURE_OPTIONS,
      );
      const sharePublication =
        publication ?? currentRoom.localParticipant.getTrackPublication(Track.Source.ScreenShare);
      if (!sharePublication || !isScreenSharePublication(sharePublication)) {
        throw new Error("Screen share track was not published");
      }
      if (roomRef.current !== currentRoom || activeChannelId() == null) {
        await currentRoom.localParticipant.setScreenShareEnabled(false).catch(() => {});
        clearScreenShareState();
        return;
      }
      bindScreenSharePublication(sharePublication, currentRoom);
      setIsScreenSharing(true);
    } catch (e) {
      clearScreenShareState();
      await currentRoom.localParticipant.setScreenShareEnabled(false).catch(() => {});
      if (roomRef.current !== currentRoom || activeChannelId() == null) return;
      const message = formatScreenShareStartError(e);
      setLastError(message);
      throw new Error(message);
    } finally {
      setIsScreenShareStarting(false);
    }
  }

  async function stopScreenShare(): Promise<void> {
    setLastError(null);
    await stopLocalScreenShare(true);
  }

  async function stopLocalCamera(reportErrors: boolean): Promise<void> {
    const currentRoom = roomRef.current;
    const existingPublication = currentRoom?.localParticipant.getTrackPublication(
      Track.Source.Camera,
    );
    const publication = cameraPublicationRef.current ?? existingPublication;
    const shouldStop =
      isCameraEnabled() ||
      isCameraBusy() ||
      localCameraTrack() != null ||
      cameraPublicationRef.current != null ||
      existingPublication != null;

    if (!currentRoom || !shouldStop) {
      clearCameraState();
      return;
    }

    setIsCameraBusy(true);
    try {
      await unpublishCameraPublication(currentRoom, publication);
    } catch (e) {
      if (reportErrors) {
        const message = formatCameraStopError(e);
        setLastError(message);
        throw new Error(message);
      }
    } finally {
      if (!cameraPublicationRef.current || cameraPublicationRef.current === publication) {
        clearCameraState();
      } else {
        setIsCameraBusy(false);
      }
      stopCameraPublicationTrack(publication);
    }
  }

  async function startCamera(): Promise<void> {
    const currentRoom = roomRef.current;
    if (!currentRoom || activeChannelId() == null) {
      const message = "Join a voice channel before turning on your camera.";
      setLastError(message);
      throw new Error(message);
    }
    if (isCameraBusy()) return;

    const existingPublication = currentRoom.localParticipant.getTrackPublication(
      Track.Source.Camera,
    );
    if (isCameraEnabled() || cameraPublicationRef.current || existingPublication) {
      if (existingPublication && isCameraPublication(existingPublication)) {
        bindCameraPublication(existingPublication, currentRoom);
      }
      const message = "Camera is already on.";
      setLastError(message);
      throw new Error(message);
    }

    setLastError(null);
    setIsCameraBusy(true);
    try {
      const publication = await currentRoom.localParticipant.setCameraEnabled(
        true,
        getCameraCaptureOptions(),
      );
      const cameraTrackPublication =
        publication ?? currentRoom.localParticipant.getTrackPublication(Track.Source.Camera);
      if (!cameraTrackPublication || !isCameraPublication(cameraTrackPublication)) {
        throw new Error("Camera track was not published");
      }
      if (!localVideoTrackFromPublication(cameraTrackPublication)) {
        throw new Error("Camera track was not published");
      }
      if (roomRef.current !== currentRoom || activeChannelId() == null) {
        await unpublishCameraPublication(currentRoom, cameraTrackPublication).catch(() => {
          stopCameraPublicationTrack(cameraTrackPublication);
        });
        if (
          !cameraPublicationRef.current ||
          cameraPublicationRef.current === cameraTrackPublication
        )
          clearCameraState();
        return;
      }
      bindCameraPublication(cameraTrackPublication, currentRoom);
    } catch (e) {
      const published = currentRoom.localParticipant.getTrackPublication(Track.Source.Camera);
      if (published) {
        await unpublishCameraPublication(currentRoom, published).catch(() => {
          stopCameraPublicationTrack(published);
        });
      }
      if (!published || !cameraPublicationRef.current || cameraPublicationRef.current === published)
        clearCameraState();
      if (roomRef.current !== currentRoom || activeChannelId() == null) return;
      const message = formatCameraStartError(e);
      setLastError(message);
      throw new Error(message);
    } finally {
      setIsCameraBusy(false);
    }
  }

  async function stopCamera(): Promise<void> {
    setLastError(null);
    await stopLocalCamera(true);
  }

  async function watchScreenShare(stream: ScreenShareStream): Promise<void> {
    const currentRoom = roomRef.current;
    if (!currentRoom || activeChannelId() !== stream.channel_id) {
      const message = "Join the sharer's voice channel before watching their screen.";
      setLastError(message);
      throw new Error(message);
    }

    setLastError(null);
    const previous = watchingScreenShare();
    const isSwitching = previous && !isSameScreenShare(previous, stream);
    if (isSwitching) {
      setWatchingScreenShareTrack(null);
      setWatchingScreenShare(null);
      updateAllRemotePublicationSubscriptions(currentRoom);
    } else if (!previous) {
      setWatchingScreenShareTrack(null);
    }
    setWatchingScreenShare(stream);

    const selected = findScreenSharePublication(currentRoom, stream);
    updateAllRemotePublicationSubscriptions(currentRoom);
    if (selected?.publication.track instanceof RemoteVideoTrack) {
      setWatchingScreenShareTrack(selected.publication.track);
    }
  }

  async function stopWatchingScreenShare(): Promise<void> {
    stopWatchingScreenShareInRoom();
  }

  async function leave(): Promise<void> {
    // Capture channel id before we reset it so the final "stopped speaking"
    // broadcast lands in the correct roomRef.current.
    const leavingChannelId = activeChannelId();
    if (roomRef.current) {
      const r = roomRef.current;
      await stopLocalCamera(false).catch(() => {});
      await stopLocalScreenShare(false).catch(() => {});
      clearRemoteCameraState();
      stopWatchingScreenShareInRoom(r);
      roomRef.current = null;
      await r.disconnect().catch(() => {});
    }
    clearCameraState();
    clearScreenShareState();
    clearWatchedScreenShareState();
    clearRemoteCameraState();
    audio.detachAll();
    setActiveChannelId(null);
    speakingByIdRef.current.clear();
    setSpeakingUserIds(new Set<number>());
    if (lastLocalSpeakingRef.current && leavingChannelId != null) {
      void postVoiceSpeaking(leavingChannelId, false);
    }
    lastLocalSpeakingRef.current = false;
  }

  async function join(channelId: number): Promise<void> {
    setLastError(null);
    setIsConnecting(true);
    try {
      // Auto-leave any current session before switching.
      if (roomRef.current) await leave();

      // Publish current controls before connecting so LiveKit's join webhook
      // can include pre-call mute/deafen status in the SSE join payload.
      const effectiveMuted = desiredMutedRef.current || desiredDeafenedRef.current;
      if (effectiveMuted !== desiredMutedRef.current) desiredMutedRef.current = effectiveMuted;
      if (effectiveMuted !== isMuted()) setIsMuted(effectiveMuted);
      await postVoiceStatus(effectiveMuted, desiredDeafenedRef.current);

      const { url, token } = await getVoiceToken(channelId);

      const inputDeviceId = localStorage.getItem(VOICE_INPUT_STORAGE_KEY) ?? "";
      const newRoom = new Room({
        adaptiveStream: true,
        dynacast: true,
        audioCaptureDefaults: {
          deviceId: inputDeviceId || undefined,
          noiseSuppression: getNoiseSuppressionEnabled(),
          echoCancellation: true,
          autoGainControl: true,
        },
      });

      newRoom.on(RoomEvent.TrackPublished, (publication, participant) => {
        updateRemotePublicationSubscription(publication, participant);
      });

      newRoom.on(RoomEvent.TrackSubscribed, (track, publication, participant) => {
        if (track.kind === Track.Kind.Audio && track instanceof RemoteAudioTrack) {
          audio.attach(track);
          return;
        }
        const watched = watchingScreenShare();
        if (track instanceof RemoteVideoTrack) {
          if (
            isRemoteScreenShareVideoPublication(publication) &&
            watched &&
            doesPublicationMatchScreenShare(participant, publication, watched)
          ) {
            setWatchingScreenShareTrack(track);
          }
          if (isRemoteCameraVideoPublication(publication)) {
            const cameraStream = findRemoteCameraStream(participant, publication);
            if (cameraStream) setRemoteCameraTrack(cameraStream, track);
          }
        }
      });

      newRoom.on(RoomEvent.TrackUnpublished, (publication, participant) => {
        const watched = watchingScreenShare();
        if (isRemoteScreenShareVideoPublication(publication)) {
          publication.setEnabled(false);
          publication.setSubscribed(false);
          if (watched && doesPublicationMatchScreenShare(participant, publication, watched)) {
            clearWatchedScreenShareState();
          }
          return;
        }
        if (!isRemoteCameraVideoPublication(publication)) return;
        publication.setEnabled(false);
        publication.setSubscribed(false);
        clearRemoteCameraTracksForPublication(participant, publication);
      });

      newRoom.on(RoomEvent.TrackUnsubscribed, (track) => {
        if (track instanceof RemoteAudioTrack) {
          audio.detach(track);
          return;
        }
        if (track instanceof RemoteVideoTrack && watchingScreenShareTrack() === track) {
          setWatchingScreenShareTrack(null);
        }
        if (track instanceof RemoteVideoTrack) {
          clearRemoteCameraTracksForTrack(track);
        }
      });

      newRoom.on(RoomEvent.LocalTrackUnpublished, (publication) => {
        if (
          isScreenSharePublication(publication) &&
          (!screenSharePublicationRef.current || publication === screenSharePublicationRef.current)
        ) {
          clearScreenShareState();
        }
        if (
          isCameraPublication(publication) &&
          (!cameraPublicationRef.current || publication === cameraPublicationRef.current)
        ) {
          clearCameraState();
        }
      });

      newRoom.on(RoomEvent.Disconnected, () => {
        // LiveKit disconnected us (server-side kick, network failure, etc.).
        stopCameraPublicationTrack(
          cameraPublicationRef.current ??
            newRoom.localParticipant.getTrackPublication(Track.Source.Camera),
        );
        roomRef.current = null;
        clearCameraState();
        clearScreenShareState();
        clearWatchedScreenShareState();
        clearRemoteCameraState();
        audio.detachAll();
        setActiveChannelId(null);
        speakingByIdRef.current.clear();
        setSpeakingUserIds(new Set<number>());
        lastLocalSpeakingRef.current = false;
      });

      // Per-participant speaking detection is meaningfully snappier than
      // RoomEvent.ActiveSpeakersChanged, which the SFU aggregates on a ~500ms
      // server tick. IsSpeakingChanged is driven by local audio-level samples,
      // so the ring tracks the waveform much more closely.
      const wireSpeakingListener = (p: Participant, isLocal: boolean) => {
        const id = Number(p.identity);
        if (!Number.isFinite(id)) return;
        p.on(ParticipantEvent.IsSpeakingChanged, (speaking: boolean) => {
          const prev = speakingByIdRef.current.get(id) ?? false;
          if (speaking === prev) return;
          if (speaking) speakingByIdRef.current.set(id, true);
          else speakingByIdRef.current.delete(id);
          setSpeakingUserIds(new Set(speakingByIdRef.current.keys()));
          if (isLocal && speaking !== lastLocalSpeakingRef.current) {
            lastLocalSpeakingRef.current = speaking;
            // `channelId` is captured directly — activeChannelId() isn't set
            // until after connect() resolves, but this listener can fire as
            // soon as the mic track publishes.
            void postVoiceSpeaking(channelId, speaking);
          }
        });
      };

      // Register roomRef.current-level participant churn handlers up front so we don't
      // miss joins/leaves that fire during or right after connect().
      newRoom.on(RoomEvent.ParticipantConnected, (p) => {
        wireSpeakingListener(p, false);
        p.trackPublications.forEach((publication) => {
          updateRemotePublicationSubscription(publication, p);
        });
      });
      newRoom.on(RoomEvent.ParticipantDisconnected, (p) => {
        const watched = watchingScreenShare();
        const disconnectedScreenShareSids = new Set<string>();
        p.trackPublications.forEach((publication) => {
          if (isRemoteScreenShareVideoPublication(publication)) {
            disconnectedScreenShareSids.add(publication.trackSid);
            publication.setEnabled(false);
            publication.setSubscribed(false);
            if (
              publication.track instanceof RemoteVideoTrack &&
              watchingScreenShareTrack() === publication.track
            ) {
              setWatchingScreenShareTrack(null);
            }
          }
          if (isRemoteCameraVideoPublication(publication)) {
            publication.setEnabled(false);
            publication.setSubscribed(false);
            clearRemoteCameraTracksForPublication(p, publication);
          }
        });
        removeRemoteCameraStreamsForParticipant(p.identity);
        if (
          watched?.participant_identity === p.identity &&
          (disconnectedScreenShareSids.size === 0 ||
            disconnectedScreenShareSids.has(watched.track_sid))
        ) {
          clearWatchedScreenShareState();
        }
        const id = Number(p.identity);
        if (!Number.isFinite(id)) return;
        if (speakingByIdRef.current.delete(id)) {
          setSpeakingUserIds(new Set(speakingByIdRef.current.keys()));
        }
      });

      audio.setDeafened(desiredDeafenedRef.current);
      await newRoom.connect(url, token, { autoSubscribe: false });
      updateAllRemotePublicationSubscriptions(newRoom);
      await newRoom.localParticipant.setMicrophoneEnabled(!effectiveMuted);

      // Identity on the local participant is only populated after connect
      // resolves, so we wire these listeners here. Remote participants that
      // were already in the roomRef.current at join-time also need manual wiring —
      // ParticipantConnected only fires for subsequent joiners.
      wireSpeakingListener(newRoom.localParticipant, true);
      newRoom.remoteParticipants.forEach((p) => wireSpeakingListener(p, false));

      // Apply the saved input gain by swapping the default capture track for
      // one that's routed through a Web Audio GainNode. Failure here is fine —
      // the default capture path is already publishing.
      await applyInputGain(newRoom, getInputGain()).catch(() => {});

      roomRef.current = newRoom;
      setActiveChannelId(channelId);
    } catch (e) {
      setLastError(e instanceof Error ? e.message : "Could not join voice channel");
      await leave();
    } finally {
      setIsConnecting(false);
    }
  }

  async function toggleMuted(): Promise<void> {
    return enqueueControlUpdate(async () => {
      const currentRoom = roomRef.current;
      const previousMuted = desiredMutedRef.current;
      const previousDeafened = desiredDeafenedRef.current;
      const previousMutedBeforeDeafen = mutedBeforeDeafenRef.current;
      const nextMuted = !previousMuted;
      const nextDeafened = previousDeafened && nextMuted;

      desiredMutedRef.current = nextMuted;
      desiredDeafenedRef.current = nextDeafened;
      if (!nextDeafened && previousDeafened) mutedBeforeDeafenRef.current = null;

      const undeafenBeforeEnablingMic = previousDeafened && !nextDeafened && !nextMuted;
      if (undeafenBeforeEnablingMic) {
        audio.setDeafened(false);
        setIsDeafened(false);
      }

      try {
        if (currentRoom) {
          await currentRoom.localParticipant.setMicrophoneEnabled(!nextMuted);
        }
      } catch (e) {
        desiredMutedRef.current = previousMuted;
        desiredDeafenedRef.current = previousDeafened;
        mutedBeforeDeafenRef.current = previousMutedBeforeDeafen;
        if (undeafenBeforeEnablingMic) {
          audio.setDeafened(previousDeafened);
          setIsDeafened(previousDeafened);
        }
        throw e;
      }

      if (previousDeafened !== nextDeafened && !undeafenBeforeEnablingMic) {
        audio.setDeafened(nextDeafened);
        setIsDeafened(nextDeafened);
      }
      setIsMuted(nextMuted);
      await postVoiceStatus(nextMuted, nextDeafened);
    });
  }

  async function toggleDeafened(): Promise<void> {
    return enqueueControlUpdate(async () => {
      const currentRoom = roomRef.current;
      const previousMuted = desiredMutedRef.current;
      const previousDeafened = desiredDeafenedRef.current;
      const previousMutedBeforeDeafen = mutedBeforeDeafenRef.current;
      const nextDeafened = !previousDeafened;
      const nextMuted = nextDeafened ? true : (mutedBeforeDeafenRef.current ?? previousMuted);

      desiredMutedRef.current = nextMuted;
      desiredDeafenedRef.current = nextDeafened;
      mutedBeforeDeafenRef.current = nextDeafened ? previousMuted : null;

      const undeafenBeforeEnablingMic = previousDeafened && !nextDeafened && !nextMuted;
      if (undeafenBeforeEnablingMic) {
        audio.setDeafened(false);
        setIsDeafened(false);
      }

      try {
        if (currentRoom && nextMuted !== previousMuted) {
          await currentRoom.localParticipant.setMicrophoneEnabled(!nextMuted);
        }
      } catch (e) {
        desiredMutedRef.current = previousMuted;
        desiredDeafenedRef.current = previousDeafened;
        mutedBeforeDeafenRef.current = previousMutedBeforeDeafen;
        if (undeafenBeforeEnablingMic) {
          audio.setDeafened(previousDeafened);
          setIsDeafened(previousDeafened);
        }
        throw e;
      }

      if (!undeafenBeforeEnablingMic) {
        audio.setDeafened(nextDeafened);
        setIsDeafened(nextDeafened);
      }
      setIsMuted(nextMuted);
      await postVoiceStatus(nextMuted, nextDeafened);
    });
  }

  registerCleanup(() => {
    void leave();
  });

  return (
    <VoiceChatContext.Provider
      value={{
        activeChannelId,
        isConnecting,
        isMuted,
        isDeafened,
        isScreenSharing,
        isScreenShareStarting,
        isCameraEnabled,
        isCameraBusy,
        localCameraTrack,
        remoteCameraTiles,
        watchingScreenShare,
        watchingScreenShareTrack,
        lastError,
        speakingUserIds,
        join,
        leave,
        toggleMuted,
        toggleDeafened,
        startScreenShare,
        stopScreenShare,
        startCamera,
        stopCamera,
        syncRemoteCameraStreams,
        watchScreenShare,
        stopWatchingScreenShare,
      }}
    >
      {props.children}
    </VoiceChatContext.Provider>
  );
}

export function useOptionalVoiceChat() {
  return useContext(VoiceChatContext);
}

export function useVoiceChat() {
  const ctx = useOptionalVoiceChat();
  if (!ctx) throw new Error("useVoiceChat must be used inside VoiceChatProvider");
  return ctx;
}
