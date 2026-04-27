import { createContext, createSignal, onCleanup, type JSX, useContext } from "solid-js";
import {
  type Participant,
  ParticipantEvent,
  RemoteAudioTrack,
  Room,
  RoomEvent,
  Track,
} from "livekit-client";
import { getVoiceToken, postVoiceSpeaking } from "../api";
import {
  VOICE_INPUT_STORAGE_KEY,
  getInputGain,
  getNoiseSuppressionEnabled,
} from "../voice/settings";
import { createAudioRouter } from "../voice/audio_routing";
import { applyInputGain } from "../voice/livekit";

interface VoiceChatContextValue {
  activeChannelId: () => number | null;
  isConnecting: () => boolean;
  isMuted: () => boolean;
  isDeafened: () => boolean;
  lastError: () => string | null;
  speakingUserIds: () => ReadonlySet<number>;
  join: (channelId: number) => Promise<void>;
  leave: () => Promise<void>;
  toggleMuted: () => Promise<void>;
  toggleDeafened: () => void;
}

const VoiceChatContext = createContext<VoiceChatContextValue>();

export function VoiceChatProvider(props: { children: JSX.Element }) {
  const [activeChannelId, setActiveChannelId] = createSignal<number | null>(null);
  const [isConnecting, setIsConnecting] = createSignal(false);
  const [isMuted, setIsMuted] = createSignal(false);
  const [isDeafened, setIsDeafened] = createSignal(false);
  const [lastError, setLastError] = createSignal<string | null>(null);
  const [speakingUserIds, setSpeakingUserIds] = createSignal<ReadonlySet<number>>(new Set());

  const audio = createAudioRouter();
  let room: Room | null = null;
  // Last speaking state we POSTed for the local participant, so we only emit
  // on transitions rather than on every IsSpeakingChanged callback.
  let lastLocalSpeaking = false;
  // user_id → speaking (true). Absent keys are not speaking. Rebuilt into the
  // reactive speakingUserIds signal on every transition.
  const speakingById = new Map<number, boolean>();

  async function leave(): Promise<void> {
    // Capture channel id before we reset it so the final "stopped speaking"
    // broadcast lands in the correct room.
    const leavingChannelId = activeChannelId();
    if (room) {
      const r = room;
      room = null;
      await r.disconnect().catch(() => {});
    }
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

      newRoom.on(RoomEvent.TrackSubscribed, (track) => {
        if (track.kind === Track.Kind.Audio && track instanceof RemoteAudioTrack) {
          audio.attach(track);
        }
      });

      newRoom.on(RoomEvent.TrackUnsubscribed, (track) => {
        if (track instanceof RemoteAudioTrack) audio.detach(track);
      });

      newRoom.on(RoomEvent.Disconnected, () => {
        // LiveKit disconnected us (server-side kick, network failure, etc.).
        room = null;
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
      newRoom.on(RoomEvent.ParticipantConnected, (p) => wireSpeakingListener(p, false));
      newRoom.on(RoomEvent.ParticipantDisconnected, (p) => {
        const id = Number(p.identity);
        if (!Number.isFinite(id)) return;
        if (speakingById.delete(id)) {
          setSpeakingUserIds(new Set(speakingById.keys()));
        }
      });

      await newRoom.connect(url, token);
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
        lastError,
        speakingUserIds,
        join,
        leave,
        toggleMuted,
        toggleDeafened,
      }}
    >
      {props.children}
    </VoiceChatContext.Provider>
  );
}

export function useVoiceChat() {
  const ctx = useContext(VoiceChatContext);
  if (!ctx) throw new Error("useVoiceChat must be used inside VoiceChatProvider");
  return ctx;
}
