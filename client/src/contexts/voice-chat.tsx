import { createContext, createSignal, onCleanup, type JSX, useContext } from "solid-js";
import {
  type LocalTrackPublication,
  type Participant,
  ParticipantEvent,
  RemoteAudioTrack,
  type RemoteParticipant,
  type RemoteTrackPublication,
  RemoteVideoTrack,
  Room,
  RoomEvent,
  Track,
} from "livekit-client";
import { getVoiceToken, postVoiceSpeaking, type ScreenShareStream } from "../api";
import {
  VOICE_INPUT_STORAGE_KEY,
  getInputGain,
  getNoiseSuppressionEnabled,
} from "../voice/settings";
import { createAudioRouter } from "../voice/audio-routing";
import { applyInputGain } from "../voice/livekit";
import { isSameScreenShare } from "../voice/screen-share";

interface VoiceChatContextValue {
  activeChannelId: () => number | null;
  isConnecting: () => boolean;
  isMuted: () => boolean;
  isDeafened: () => boolean;
  isScreenSharing: () => boolean;
  isScreenShareStarting: () => boolean;
  watchingScreenShare: () => ScreenShareStream | null;
  watchingScreenShareTrack: () => RemoteVideoTrack | null;
  lastError: () => string | null;
  speakingUserIds: () => ReadonlySet<number>;
  join: (channelId: number) => Promise<void>;
  leave: () => Promise<void>;
  toggleMuted: () => Promise<void>;
  toggleDeafened: () => void;
  startScreenShare: () => Promise<void>;
  stopScreenShare: () => Promise<void>;
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

function isScreenSharePublication(publication: LocalTrackPublication): boolean {
  return (
    publication.source === Track.Source.ScreenShare ||
    publication.track?.source === Track.Source.ScreenShare
  );
}

const VoiceChatContext = createContext<VoiceChatContextValue>();

export function VoiceChatProvider(props: { children: JSX.Element }) {
  const [activeChannelId, setActiveChannelId] = createSignal<number | null>(null);
  const [isConnecting, setIsConnecting] = createSignal(false);
  const [isMuted, setIsMuted] = createSignal(false);
  const [isDeafened, setIsDeafened] = createSignal(false);
  const [isScreenSharing, setIsScreenSharing] = createSignal(false);
  const [isScreenShareStarting, setIsScreenShareStarting] = createSignal(false);
  const [watchingScreenShare, setWatchingScreenShare] = createSignal<ScreenShareStream | null>(
    null,
  );
  const [watchingScreenShareTrack, setWatchingScreenShareTrack] =
    createSignal<RemoteVideoTrack | null>(null);
  const [lastError, setLastError] = createSignal<string | null>(null);
  const [speakingUserIds, setSpeakingUserIds] = createSignal<ReadonlySet<number>>(new Set());

  const audio = createAudioRouter();
  let room: Room | null = null;
  let screenSharePublication: LocalTrackPublication | undefined;
  let cleanupScreenShareTrackEnded: (() => void) | undefined;
  // Last speaking state we POSTed for the local participant, so we only emit
  // on transitions rather than on every IsSpeakingChanged callback.
  let lastLocalSpeaking = false;
  // user_id → speaking (true). Absent keys are not speaking. Rebuilt into the
  // reactive speakingUserIds signal on every transition.
  const speakingById = new Map<number, boolean>();

  function clearScreenShareState(): void {
    cleanupScreenShareTrackEnded?.();
    cleanupScreenShareTrackEnded = undefined;
    screenSharePublication = undefined;
    setIsScreenSharing(false);
    setIsScreenShareStarting(false);
  }

  function bindScreenSharePublication(publication: LocalTrackPublication, currentRoom: Room): void {
    cleanupScreenShareTrackEnded?.();
    cleanupScreenShareTrackEnded = undefined;
    screenSharePublication = publication;

    const mediaTrack = publication.track?.mediaStreamTrack;
    if (!mediaTrack) return;

    const handleEnded = () => {
      if (room !== currentRoom || screenSharePublication !== publication) return;
      clearScreenShareState();
      void currentRoom.localParticipant.setScreenShareEnabled(false).catch(() => {});
    };
    mediaTrack.addEventListener("ended", handleEnded, { once: true });
    cleanupScreenShareTrackEnded = () => mediaTrack.removeEventListener("ended", handleEnded);
  }

  function clearWatchedScreenShareState(): void {
    setWatchingScreenShare(null);
    setWatchingScreenShareTrack(null);
  }

  function stopWatchingScreenShareInRoom(currentRoom = room): void {
    clearWatchedScreenShareState();
    updateAllRemotePublicationSubscriptions(currentRoom);
  }

  function isRemoteMicrophonePublication(publication: RemoteTrackPublication): boolean {
    return publication.kind === Track.Kind.Audio && publication.source === Track.Source.Microphone;
  }

  function isRemoteScreenShareVideoPublication(publication: RemoteTrackPublication): boolean {
    return publication.kind === Track.Kind.Video && publication.source === Track.Source.ScreenShare;
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

  function updateRemotePublicationSubscription(
    publication: RemoteTrackPublication,
    participant: RemoteParticipant,
  ): void {
    if (isRemoteMicrophonePublication(publication)) {
      publication.setSubscribed(true);
      return;
    }

    if (!isRemoteScreenShareVideoPublication(publication)) return;

    const watched = watchingScreenShare();
    const shouldWatch = Boolean(
      watched && doesPublicationMatchScreenShare(participant, publication, watched),
    );
    publication.setEnabled(shouldWatch);
    publication.setSubscribed(shouldWatch);
    if (shouldWatch && publication.track instanceof RemoteVideoTrack) {
      setWatchingScreenShareTrack(publication.track);
    }
  }

  function updateAllRemotePublicationSubscriptions(currentRoom = room): void {
    currentRoom?.remoteParticipants.forEach((participant) => {
      participant.trackPublications.forEach((publication) => {
        updateRemotePublicationSubscription(publication, participant);
      });
    });
  }

  function findScreenSharePublication(
    currentRoom: Room,
    stream: ScreenShareStream,
  ): { participant: RemoteParticipant; publication: RemoteTrackPublication } | null {
    const participant = currentRoom.remoteParticipants.get(stream.participant_identity);
    const publication = participant?.trackPublications.get(stream.track_sid);
    if (!participant || !publication || !isRemoteScreenShareVideoPublication(publication)) {
      return null;
    }
    return { participant, publication };
  }

  async function stopLocalScreenShare(reportErrors: boolean): Promise<void> {
    const currentRoom = room;
    const existingPublication = currentRoom?.localParticipant.getTrackPublication(
      Track.Source.ScreenShare,
    );
    const shouldStop =
      isScreenSharing() ||
      isScreenShareStarting() ||
      screenSharePublication != null ||
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
    const currentRoom = room;
    if (!currentRoom || activeChannelId() == null) {
      const message = "Join a voice channel before sharing your screen.";
      setLastError(message);
      throw new Error(message);
    }
    if (isScreenShareStarting()) return;

    const existingPublication = currentRoom.localParticipant.getTrackPublication(
      Track.Source.ScreenShare,
    );
    if (isScreenSharing() || screenSharePublication || existingPublication) {
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
      if (room !== currentRoom || activeChannelId() == null) {
        await currentRoom.localParticipant.setScreenShareEnabled(false).catch(() => {});
        clearScreenShareState();
        return;
      }
      bindScreenSharePublication(sharePublication, currentRoom);
      setIsScreenSharing(true);
    } catch (e) {
      clearScreenShareState();
      await currentRoom.localParticipant.setScreenShareEnabled(false).catch(() => {});
      if (room !== currentRoom || activeChannelId() == null) return;
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

  async function watchScreenShare(stream: ScreenShareStream): Promise<void> {
    const currentRoom = room;
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
    // broadcast lands in the correct room.
    const leavingChannelId = activeChannelId();
    if (room) {
      const r = room;
      await stopLocalScreenShare(false).catch(() => {});
      stopWatchingScreenShareInRoom(r);
      room = null;
      await r.disconnect().catch(() => {});
    }
    clearScreenShareState();
    clearWatchedScreenShareState();
    audio.detachAll();
    setActiveChannelId(null);
    setIsMuted(false);
    setIsDeafened(false);
    speakingById.clear();
    setSpeakingUserIds(new Set<number>());
    if (lastLocalSpeaking && leavingChannelId != null) {
      void postVoiceSpeaking(leavingChannelId, false);
    }
    lastLocalSpeaking = false;
  }

  async function join(channelId: number): Promise<void> {
    setLastError(null);
    setIsConnecting(true);
    try {
      // Auto-leave any current session before switching.
      if (room) await leave();

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
        if (
          track instanceof RemoteVideoTrack &&
          isRemoteScreenShareVideoPublication(publication) &&
          watched &&
          doesPublicationMatchScreenShare(participant, publication, watched)
        ) {
          setWatchingScreenShareTrack(track);
        }
      });

      newRoom.on(RoomEvent.TrackUnpublished, (publication, participant) => {
        const watched = watchingScreenShare();
        if (!isRemoteScreenShareVideoPublication(publication)) return;
        publication.setEnabled(false);
        publication.setSubscribed(false);
        if (watched && doesPublicationMatchScreenShare(participant, publication, watched)) {
          clearWatchedScreenShareState();
        }
      });

      newRoom.on(RoomEvent.TrackUnsubscribed, (track) => {
        if (track instanceof RemoteAudioTrack) {
          audio.detach(track);
          return;
        }
        if (track instanceof RemoteVideoTrack && watchingScreenShareTrack() === track) {
          setWatchingScreenShareTrack(null);
        }
      });

      newRoom.on(RoomEvent.LocalTrackUnpublished, (publication) => {
        if (
          isScreenSharePublication(publication) &&
          (!screenSharePublication || publication === screenSharePublication)
        ) {
          clearScreenShareState();
        }
      });

      newRoom.on(RoomEvent.Disconnected, () => {
        // LiveKit disconnected us (server-side kick, network failure, etc.).
        room = null;
        clearScreenShareState();
        clearWatchedScreenShareState();
        audio.detachAll();
        setActiveChannelId(null);
        setIsMuted(false);
        setIsDeafened(false);
        speakingById.clear();
        setSpeakingUserIds(new Set<number>());
        lastLocalSpeaking = false;
      });

      // Per-participant speaking detection is meaningfully snappier than
      // RoomEvent.ActiveSpeakersChanged, which the SFU aggregates on a ~500ms
      // server tick. IsSpeakingChanged is driven by local audio-level samples,
      // so the ring tracks the waveform much more closely.
      const wireSpeakingListener = (p: Participant, isLocal: boolean) => {
        const id = Number(p.identity);
        if (!Number.isFinite(id)) return;
        p.on(ParticipantEvent.IsSpeakingChanged, (speaking: boolean) => {
          const prev = speakingById.get(id) ?? false;
          if (speaking === prev) return;
          if (speaking) speakingById.set(id, true);
          else speakingById.delete(id);
          setSpeakingUserIds(new Set(speakingById.keys()));
          if (isLocal && speaking !== lastLocalSpeaking) {
            lastLocalSpeaking = speaking;
            // `channelId` is captured directly — activeChannelId() isn't set
            // until after connect() resolves, but this listener can fire as
            // soon as the mic track publishes.
            void postVoiceSpeaking(channelId, speaking);
          }
        });
      };

      // Register room-level participant churn handlers up front so we don't
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
          if (!isRemoteScreenShareVideoPublication(publication)) return;
          disconnectedScreenShareSids.add(publication.trackSid);
          publication.setEnabled(false);
          publication.setSubscribed(false);
          if (
            publication.track instanceof RemoteVideoTrack &&
            watchingScreenShareTrack() === publication.track
          ) {
            setWatchingScreenShareTrack(null);
          }
        });
        if (
          watched?.participant_identity === p.identity &&
          (disconnectedScreenShareSids.size === 0 ||
            disconnectedScreenShareSids.has(watched.track_sid))
        ) {
          clearWatchedScreenShareState();
        }
        const id = Number(p.identity);
        if (!Number.isFinite(id)) return;
        if (speakingById.delete(id)) {
          setSpeakingUserIds(new Set(speakingById.keys()));
        }
      });

      await newRoom.connect(url, token, { autoSubscribe: false });
      updateAllRemotePublicationSubscriptions(newRoom);
      await newRoom.localParticipant.setMicrophoneEnabled(true);

      // Identity on the local participant is only populated after connect
      // resolves, so we wire these listeners here. Remote participants that
      // were already in the room at join-time also need manual wiring —
      // ParticipantConnected only fires for subsequent joiners.
      wireSpeakingListener(newRoom.localParticipant, true);
      newRoom.remoteParticipants.forEach((p) => wireSpeakingListener(p, false));

      // Apply the saved input gain by swapping the default capture track for
      // one that's routed through a Web Audio GainNode. Failure here is fine —
      // the default capture path is already publishing.
      await applyInputGain(newRoom, getInputGain()).catch(() => {});

      room = newRoom;
      setActiveChannelId(channelId);
    } catch (e) {
      setLastError(e instanceof Error ? e.message : "Could not join voice channel");
      await leave();
    } finally {
      setIsConnecting(false);
    }
  }

  async function toggleMuted(): Promise<void> {
    if (!room) return;
    const next = !isMuted();
    await room.localParticipant.setMicrophoneEnabled(!next);
    setIsMuted(next);
  }

  function toggleDeafened(): void {
    const next = !isDeafened();
    audio.setDeafened(next);
    setIsDeafened(next);
  }

  onCleanup(() => {
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
