import { createContext, createSignal, onCleanup, type JSX, useContext } from "solid-js";
import {
  LocalAudioTrack,
  type Participant,
  ParticipantEvent,
  RemoteAudioTrack,
  Room,
  RoomEvent,
  Track,
} from "livekit-client";
import { getVoiceToken, postVoiceSpeaking } from "./api";
import {
  VOICE_INPUT_STORAGE_KEY,
  VOICE_OUTPUT_STORAGE_KEY,
  getInputGain,
  getNoiseSuppressionEnabled,
} from "./components/voice_settings";

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

// Chromium's HTMLAudioElement.setSinkId is the only way to route audio to a
// non-default output device from inside a webview. It's gated behind a feature
// check so Safari/WebKit fall back to the system default cleanly.
type SinkIdAudio = HTMLAudioElement & { setSinkId?: (id: string) => Promise<void> };

export function VoiceChatProvider(props: { children: JSX.Element }) {
  const [activeChannelId, setActiveChannelId] = createSignal<number | null>(null);
  const [isConnecting, setIsConnecting] = createSignal(false);
  const [isMuted, setIsMuted] = createSignal(false);
  const [isDeafened, setIsDeafened] = createSignal(false);
  const [lastError, setLastError] = createSignal<string | null>(null);
  const [speakingUserIds, setSpeakingUserIds] = createSignal<ReadonlySet<number>>(new Set());

  let room: Room | null = null;
  // Last speaking state we POSTed for the local participant, so we only emit
  // on transitions rather than on every IsSpeakingChanged callback.
  let lastLocalSpeaking = false;
  // user_id → speaking (true). Absent keys are not speaking. Rebuilt into the
  // reactive speakingUserIds signal on every transition.
  const speakingById = new Map<number, boolean>();
  // Each subscribed remote audio track gets its own <audio> element so we can
  // (a) drive output routing via setSinkId per-track and (b) mute them all
  // together when the user deafens.
  const audioElements = new Map<string, HTMLAudioElement>();

  function detachAll() {
    audioElements.forEach((el) => {
      el.pause();
      el.srcObject = null;
      el.remove();
    });
    audioElements.clear();
  }

  async function leave(): Promise<void> {
    // Capture channel id before we reset it so the final "stopped speaking"
    // broadcast lands in the correct room.
    const leavingChannelId = activeChannelId();
    if (room) {
      const r = room;
      room = null;
      await r.disconnect().catch(() => {});
    }
    detachAll();
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

  function attachRemoteAudio(track: RemoteAudioTrack): void {
    const el = track.attach() as SinkIdAudio;
    el.autoplay = true;
    el.muted = isDeafened();
    const outputId = localStorage.getItem(VOICE_OUTPUT_STORAGE_KEY) ?? "";
    if (outputId && typeof el.setSinkId === "function") {
      el.setSinkId(outputId).catch(() => {
        // Fallback to default output is fine; the user will hear audio either way.
      });
    }
    // Append to body so the browser actually plays it. LiveKit's attach()
    // returns a detached element by default.
    document.body.appendChild(el);
    audioElements.set(track.sid ?? Math.random().toString(), el);
  }

  function detachTrack(track: RemoteAudioTrack): void {
    const key = track.sid ?? "";
    const el = audioElements.get(key);
    if (el) {
      el.pause();
      el.srcObject = null;
      el.remove();
      audioElements.delete(key);
    }
    track.detach().forEach((e) => e.remove());
  }

  async function join(channelId: number): Promise<void> {
    setLastError(null);
    setIsConnecting(true);
    try {
      // Auto-leave any current session before switching.
      if (room) await leave();

      const { url, token } = await getVoiceToken(channelId);
      console.log("[voice] token OK — url:", url, "channel:", channelId);

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
          attachRemoteAudio(track);
        }
      });

      newRoom.on(RoomEvent.TrackUnsubscribed, (track) => {
        if (track instanceof RemoteAudioTrack) detachTrack(track);
      });

      newRoom.on(RoomEvent.Disconnected, (reason) => {
        console.warn("[voice] disconnected, reason:", reason);
        // LiveKit disconnected us (server-side kick, network failure, etc.).
        room = null;
        detachAll();
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

      console.log("[voice] connecting…");
      await newRoom.connect(url, token);
      console.log("[voice] connected; enabling mic…");
      await newRoom.localParticipant.setMicrophoneEnabled(true);
      console.log("[voice] mic enabled");

      // Identity on the local participant is only populated after connect
      // resolves, so we wire these listeners here. Remote participants that
      // were already in the room at join-time also need manual wiring —
      // ParticipantConnected only fires for subsequent joiners.
      wireSpeakingListener(newRoom.localParticipant, true);
      newRoom.remoteParticipants.forEach((p) => wireSpeakingListener(p, false));

      // Apply the saved input gain by swapping the default capture track for
      // one that's routed through a Web Audio GainNode.
      await applyInputGain(newRoom, getInputGain()).catch((err) => {
        console.warn("[voice] applyInputGain failed (continuing):", err);
      });

      room = newRoom;
      setActiveChannelId(channelId);
      console.log("[voice] activeChannelId set to", channelId);
    } catch (e) {
      console.error("[voice] join failed:", e);
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
    audioElements.forEach((el) => {
      el.muted = next;
    });
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

/**
 * Replace the default mic publication with a track that passes through a
 * GainNode so user-configured input volume takes effect. Skipped when the gain
 * is exactly 1.0 (the native capture path has lower latency and fewer moving
 * parts).
 */
async function applyInputGain(room: Room, gain: number): Promise<void> {
  if (Math.abs(gain - 1) < 0.01) return;
  const audioCtx = new AudioContext();
  const inputDeviceId = localStorage.getItem(VOICE_INPUT_STORAGE_KEY) ?? "";
  const stream = await navigator.mediaDevices.getUserMedia({
    audio: {
      deviceId: inputDeviceId ? { exact: inputDeviceId } : undefined,
      noiseSuppression: getNoiseSuppressionEnabled(),
      echoCancellation: true,
      autoGainControl: true,
    },
  });
  const source = audioCtx.createMediaStreamSource(stream);
  const gainNode = audioCtx.createGain();
  gainNode.gain.value = gain;
  const dest = audioCtx.createMediaStreamDestination();
  source.connect(gainNode).connect(dest);
  const processed = dest.stream.getAudioTracks()[0];
  if (!processed) return;
  const localTrack = new LocalAudioTrack(processed, undefined, false);
  // Unpublish the default mic track, then publish the gain-adjusted one.
  const existing = room.localParticipant.getTrackPublication(Track.Source.Microphone);
  if (existing?.audioTrack) {
    await room.localParticipant.unpublishTrack(existing.audioTrack);
  }
  await room.localParticipant.publishTrack(localTrack, { source: Track.Source.Microphone });
}
