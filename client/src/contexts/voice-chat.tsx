import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useSyncExternalStore,
  type ReactNode,
} from "react";
import { Room } from "livekit-client";

import { getVoiceToken, postVoiceSpeaking, postVoiceStatus } from "../api";
import { createAudioRouter } from "../voice/audio-routing";
import { applyInputGain } from "../voice/livekit";
import {
  createVoiceSession,
  type VoiceSession,
  type VoiceSessionDependencies,
} from "../voice/voice-session";
import type { MediaStatus, RemoteCameraTile } from "../voice/voice-state";
import { useVoicePreferences } from "./voice-preferences";

export type { RemoteCameraTile } from "../voice/voice-state";

export interface VoiceChatContextValue {
  readonly activeChannelId: number | null;
  readonly isConnecting: boolean;
  readonly isMuted: boolean;
  readonly isDeafened: boolean;
  readonly isScreenSharing: boolean;
  readonly screenShareStatus: MediaStatus;
  readonly isScreenShareStarting: boolean;
  readonly isScreenShareBusy: boolean;
  readonly isCameraEnabled: boolean;
  readonly cameraStatus: MediaStatus;
  readonly isCameraBusy: boolean;
  readonly localCameraTrack: ReturnType<VoiceSession["getSnapshot"]>["localCameraTrack"];
  readonly remoteCameraTiles: readonly RemoteCameraTile[];
  readonly watchingScreenShare: ReturnType<VoiceSession["getSnapshot"]>["watchedScreenShare"];
  readonly watchingScreenShareTrack: ReturnType<
    VoiceSession["getSnapshot"]
  >["watchedScreenShareTrack"];
  readonly lastError: string | null;
  readonly speakingUserIds: ReadonlySet<number>;
  readonly join: VoiceSession["join"];
  readonly leave: VoiceSession["leave"];
  readonly toggleMuted: VoiceSession["toggleMuted"];
  readonly toggleDeafened: VoiceSession["toggleDeafened"];
  readonly startScreenShare: VoiceSession["startScreenShare"];
  readonly stopScreenShare: VoiceSession["stopScreenShare"];
  readonly startCamera: VoiceSession["startCamera"];
  readonly stopCamera: VoiceSession["stopCamera"];
  readonly syncRemoteCameraStreams: VoiceSession["syncRemoteCameraStreams"];
  readonly watchScreenShare: VoiceSession["watchScreenShare"];
  readonly stopWatchingScreenShare: VoiceSession["stopWatchingScreenShare"];
}

export type VoiceSessionFactory = (dependencies: VoiceSessionDependencies) => VoiceSession;

const VoiceChatContext = createContext<VoiceChatContextValue | undefined>(undefined);

export function VoiceChatProvider(props: {
  children: ReactNode;
  /** Injectable session boundary for deterministic provider tests. */
  createSession?: VoiceSessionFactory;
}) {
  const preferences = useVoicePreferences();
  const preferencesRef = useRef(preferences.snapshot);
  preferencesRef.current = preferences.snapshot;

  const sessionRef = useRef<VoiceSession | null>(null);
  if (!sessionRef.current) {
    const dependencies: VoiceSessionDependencies = {
      getToken: getVoiceToken,
      postStatus: postVoiceStatus,
      postSpeaking: postVoiceSpeaking,
      createRoom: (options) => new Room(options),
      createAudioRouter,
      applyInputGain,
      getPreferences: () => preferencesRef.current,
    };
    sessionRef.current = (props.createSession ?? createVoiceSession)(dependencies);
  }
  const session = sessionRef.current;
  const snapshot = useSyncExternalStore(
    (listener) => session.subscribe(listener),
    () => session.getSnapshot(),
    () => session.getSnapshot(),
  );

  useEffect(() => {
    session.applyPreferences(preferences.snapshot);
  }, [preferences.snapshot, session]);

  useEffect(() => {
    session.activate();
    return () => {
      void session.deactivate();
    };
  }, [session]);

  const commands = useMemo(
    () => ({
      join: (channelId: number) => session.join(channelId),
      leave: () => session.leave(),
      toggleMuted: () => session.toggleMuted(),
      toggleDeafened: () => session.toggleDeafened(),
      startScreenShare: () => session.startScreenShare(),
      stopScreenShare: () => session.stopScreenShare(),
      startCamera: () => session.startCamera(),
      stopCamera: () => session.stopCamera(),
      syncRemoteCameraStreams: (
        channelId: number,
        streams: Parameters<VoiceSession["syncRemoteCameraStreams"]>[1],
      ) => session.syncRemoteCameraStreams(channelId, streams),
      watchScreenShare: (stream: Parameters<VoiceSession["watchScreenShare"]>[0]) =>
        session.watchScreenShare(stream),
      stopWatchingScreenShare: () => session.stopWatchingScreenShare(),
    }),
    [session],
  );

  const value = useMemo<VoiceChatContextValue>(
    () => ({
      activeChannelId: snapshot.activeChannelId,
      isConnecting: snapshot.connectionStatus === "connecting",
      isMuted: snapshot.muted,
      isDeafened: snapshot.deafened,
      isScreenSharing: snapshot.screenSharePublicationVisible,
      screenShareStatus: snapshot.screenShareStatus,
      isScreenShareStarting: snapshot.screenShareStatus === "starting",
      isScreenShareBusy:
        snapshot.screenShareStatus === "starting" || snapshot.screenShareStatus === "stopping",
      isCameraEnabled: snapshot.cameraStatus === "on",
      cameraStatus: snapshot.cameraStatus,
      isCameraBusy: snapshot.cameraStatus === "starting" || snapshot.cameraStatus === "stopping",
      localCameraTrack: snapshot.localCameraTrack,
      remoteCameraTiles: snapshot.remoteCameraTiles,
      watchingScreenShare: snapshot.watchedScreenShare,
      watchingScreenShareTrack: snapshot.watchedScreenShareTrack,
      lastError: snapshot.error,
      speakingUserIds: snapshot.speakingUserIds,
      ...commands,
    }),
    [commands, snapshot],
  );

  return <VoiceChatContext.Provider value={value}>{props.children}</VoiceChatContext.Provider>;
}

export function useOptionalVoiceChat(): VoiceChatContextValue | undefined {
  return useContext(VoiceChatContext);
}

export function useVoiceChat(): VoiceChatContextValue {
  const context = useOptionalVoiceChat();
  if (!context) throw new Error("useVoiceChat must be used inside VoiceChatProvider");
  return context;
}
