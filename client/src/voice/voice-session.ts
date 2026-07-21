import {
  ParticipantEvent,
  RemoteAudioTrack,
  RemoteVideoTrack,
  RoomEvent,
  Track,
  type LocalTrackPublication,
  type LocalVideoTrack,
  type Participant,
  type RemoteParticipant,
  type RemoteTrackPublication,
  type Room,
  type RoomOptions,
  type VideoCaptureOptions,
} from "livekit-client";

import type { CameraStream, ScreenShareStream, VoiceToken } from "../api";
import { cameraKey, sortCameraStreams } from "./camera";
import type { AudioRouter } from "./audio-routing";
import type { InputGainHandle, InputGainPreferences } from "./livekit";
import { isSameScreenShare } from "./screen-share";
import type { VoicePreferencesSnapshot } from "./settings";
import {
  createVoiceSnapshot,
  resetVoiceConnection,
  updateVoiceSnapshot,
  type RemoteCameraTile,
  type VoiceSnapshot,
  type VoiceSnapshotUpdate,
} from "./voice-state";

export interface VoiceSessionDependencies {
  readonly getToken: (channelId: number) => Promise<VoiceToken>;
  readonly postStatus: (muted: boolean, deafened: boolean) => Promise<void>;
  readonly postSpeaking: (channelId: number, speaking: boolean) => Promise<void>;
  readonly createRoom: (options: RoomOptions) => Room;
  readonly createAudioRouter: (outputDeviceId: string) => AudioRouter;
  readonly applyInputGain: (
    room: Room,
    preferences: InputGainPreferences,
  ) => Promise<InputGainHandle | null>;
  readonly getPreferences: () => VoicePreferencesSnapshot;
}

export interface VoiceSession {
  subscribe(listener: () => void): () => void;
  getSnapshot(): VoiceSnapshot;
  activate(): void;
  deactivate(): Promise<void>;
  join(channelId: number): Promise<void>;
  leave(): Promise<void>;
  toggleMuted(): Promise<void>;
  toggleDeafened(): Promise<void>;
  startScreenShare(): Promise<void>;
  stopScreenShare(): Promise<void>;
  startCamera(): Promise<void>;
  stopCamera(): Promise<void>;
  syncRemoteCameraStreams(channelId: number, streams: readonly CameraStream[]): void;
  applyPreferences(preferences: VoicePreferencesSnapshot): void;
  watchScreenShare(stream: ScreenShareStream): Promise<void>;
  stopWatchingScreenShare(): Promise<void>;
}

const SCREEN_SHARE_CAPTURE_OPTIONS = {
  audio: false,
  video: true,
  systemAudio: "exclude",
} as const;

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message ? error.message : fallback;
}

function cameraError(error: unknown): string {
  const name = error instanceof Error ? error.name : "";
  if (name === "NotAllowedError" || name === "SecurityError")
    return "Camera permission was denied.";
  if (name === "AbortError") return "Camera start was canceled.";
  if (
    name === "NotFoundError" ||
    name === "DevicesNotFoundError" ||
    name === "OverconstrainedError"
  ) {
    return "No camera device was found.";
  }
  return `Could not start camera${error instanceof Error && error.message ? `: ${error.message}` : ""}`;
}

function shareError(error: unknown): string {
  const name = error instanceof Error ? error.name : "";
  if (name === "AbortError" || name === "NotAllowedError" || name === "SecurityError") {
    return "Screen share was canceled or denied.";
  }
  return `Could not start screen share${error instanceof Error && error.message ? `: ${error.message}` : ""}`;
}

function isCameraPublication(publication: LocalTrackPublication): boolean {
  return (
    publication.source === Track.Source.Camera || publication.track?.source === Track.Source.Camera
  );
}

function isSharePublication(publication: LocalTrackPublication): boolean {
  return (
    publication.source === Track.Source.ScreenShare ||
    publication.track?.source === Track.Source.ScreenShare
  );
}

function cameraTrack(publication: LocalTrackPublication | undefined): LocalVideoTrack | null {
  const track = publication?.videoTrack ?? publication?.track;
  return track && (track.kind === Track.Kind.Video || track.source === Track.Source.Camera)
    ? (track as LocalVideoTrack)
    : null;
}

export function createVoiceSession(dependencies: VoiceSessionDependencies): VoiceSession {
  return new ImperativeVoiceSession(dependencies);
}

class ImperativeVoiceSession implements VoiceSession {
  private snapshot = createVoiceSnapshot();
  private readonly listeners = new Set<() => void>();
  private active = false;
  private epoch = 0;
  private cameraEpoch = 0;
  private shareEpoch = 0;
  private room: Room | null = null;
  private readonly roomListenerCleanup = new Map<Room, () => void>();
  private readonly participantCleanup = new Map<Room, Map<Participant, () => void>>();
  private readonly disposedRooms = new WeakSet<Room>();
  private audio: AudioRouter | null = null;
  private gain: { readonly room: Room; readonly handle: InputGainHandle } | null = null;
  private cameraPublication: LocalTrackPublication | undefined;
  private sharePublication: LocalTrackPublication | undefined;
  private cameraEndedCleanup: (() => void) | null = null;
  private shareEndedCleanup: (() => void) | null = null;
  private remoteStreams: readonly CameraStream[] = [];
  private readonly remoteTracks = new Map<string, RemoteVideoTrack>();
  private desiredMuted = false;
  private desiredDeafened = false;
  private mutedBeforeDeafen: boolean | null = null;
  private controlQueue: Promise<void> = Promise.resolve();
  private cameraQueue: Promise<void> | null = null;
  private cameraStartPromise: Promise<void> | null = null;
  private shareQueue: Promise<void> | null = null;
  private shareStartPromise: Promise<void> | null = null;
  private preferences: VoicePreferencesSnapshot;
  private readonly speakingById = new Set<number>();
  private lastLocalSpeaking = false;

  constructor(private readonly dependencies: VoiceSessionDependencies) {
    this.preferences = dependencies.getPreferences();
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    let subscribed = true;
    return () => {
      if (!subscribed) return;
      subscribed = false;
      this.listeners.delete(listener);
    };
  };

  getSnapshot = (): VoiceSnapshot => this.snapshot;

  activate = (): void => {
    if (this.active) return;
    this.active = true;
    this.audio ??= this.dependencies.createAudioRouter(this.preferences.outputDeviceId);
    this.audio.setOutputDevice(this.preferences.outputDeviceId);
    this.audio.setDeafened(this.desiredDeafened);
  };

  deactivate = async (): Promise<void> => {
    if (!this.active && !this.room) return;
    this.active = false;
    await this.leaveInternal();
  };

  private publish(update: VoiceSnapshotUpdate): void {
    const next = updateVoiceSnapshot(this.snapshot, update);
    if (next === this.snapshot) return;
    this.snapshot = next;
    this.listeners.forEach((listener) => listener());
  }

  private publishReset(error?: string | null): void {
    const previous = this.snapshot;
    const reset = resetVoiceConnection(previous);
    const next = error === undefined ? reset : updateVoiceSnapshot(reset, { error });
    if (next === previous) return;
    this.snapshot = next;
    this.listeners.forEach((listener) => listener());
  }

  private current(epoch: number, room?: Room): boolean {
    return this.active && this.epoch === epoch && (room === undefined || this.room === room);
  }

  applyPreferences = (preferences: VoicePreferencesSnapshot): void => {
    this.preferences = preferences;
    this.audio?.setOutputDevice(preferences.outputDeviceId);
  };

  join = async (channelId: number): Promise<void> => {
    if (!this.active) return;
    const epoch = ++this.epoch;
    this.cameraEpoch++;
    this.shareEpoch++;
    const camera = this.cameraPublication;
    const share = this.sharePublication;
    this.clearTransitionState(this.snapshot.activeChannelId);
    await this.disposeCurrentRoom(camera, share);
    if (!this.current(epoch)) return;
    this.publish({ connectionStatus: "connecting", activeChannelId: null, error: null });

    const effectiveMuted = this.desiredMuted || this.desiredDeafened;
    this.desiredMuted = effectiveMuted;
    this.publish({ muted: effectiveMuted, deafened: this.desiredDeafened });

    try {
      await this.dependencies.postStatus(effectiveMuted, this.desiredDeafened);
      if (!this.current(epoch)) return;
      const token = await this.dependencies.getToken(channelId);
      if (!this.current(epoch)) return;

      const room = this.dependencies.createRoom({
        adaptiveStream: true,
        dynacast: true,
        audioCaptureDefaults: {
          deviceId: this.preferences.inputDeviceId || undefined,
          noiseSuppression: this.preferences.noiseSuppression,
          echoCancellation: true,
          autoGainControl: true,
        },
      });
      // Own the room before the first await so leave/deactivate can always reach it.
      this.room = room;
      this.wireRoom(room, channelId, epoch);
      this.audio?.setDeafened(this.desiredDeafened);

      await room.connect(token.url, token.token, { autoSubscribe: false });
      if (!this.current(epoch, room)) {
        await this.disposeRoom(room);
        return;
      }
      this.updateAllSubscriptions(room);
      await room.localParticipant.setMicrophoneEnabled(!effectiveMuted);
      if (!this.current(epoch, room)) {
        await this.disposeRoom(room);
        return;
      }

      this.wireSpeaking(room.localParticipant, true, channelId, epoch);
      room.remoteParticipants.forEach((participant) =>
        this.wireSpeaking(participant, false, channelId, epoch),
      );

      let replacementHandle: InputGainHandle | null = null;
      try {
        replacementHandle = await this.dependencies.applyInputGain(room, {
          gain: this.preferences.inputGain,
          inputDeviceId: this.preferences.inputDeviceId,
          noiseSuppression: this.preferences.noiseSuppression,
        });
        if (!this.current(epoch, room)) {
          await replacementHandle?.dispose();
          await this.disposeRoom(room);
          return;
        }
        // Publishing the processed track enables it. Serialize the correction with
        // mute/deafen controls so it applies the latest desired state, including a
        // control changed while gain processing was being created.
        if (replacementHandle) {
          await this.enqueueControl(async () => {
            if (!this.current(epoch, room)) return;
            await room.localParticipant.setMicrophoneEnabled(
              !(this.desiredMuted || this.desiredDeafened),
            );
          });
          if (!this.current(epoch, room)) {
            await replacementHandle.dispose();
            return;
          }
        }
        await this.gain?.handle.dispose();
        this.gain = replacementHandle ? { room, handle: replacementHandle } : null;
      } catch (error) {
        await replacementHandle?.dispose().catch(() => undefined);
        // A failed gain setup leaves the default microphone usable. If a published
        // replacement could not be brought in sync with a current mute, fail closed.
        if (replacementHandle && (this.desiredMuted || this.desiredDeafened)) throw error;
      }
      if (!this.current(epoch, room)) return;
      this.publish({ activeChannelId: channelId, connectionStatus: "connected" });
    } catch (error) {
      if (!this.current(epoch)) return;
      await this.disposeCurrentRoom();
      if (this.current(epoch))
        this.publishReset(errorMessage(error, "Could not join voice channel"));
    }
  };

  leave = async (): Promise<void> => {
    await this.leaveInternal();
  };

  private async leaveInternal(): Promise<void> {
    ++this.epoch;
    ++this.cameraEpoch;
    ++this.shareEpoch;
    const camera = this.cameraPublication;
    const share = this.sharePublication;
    this.clearTransitionState(this.snapshot.activeChannelId);
    await this.disposeCurrentRoom(camera, share);
  }

  /** Converges every room-ending path before transport disposal can race it. */
  private clearTransitionState(channelId: number | null): void {
    this.cameraEndedCleanup?.();
    this.shareEndedCleanup?.();
    this.cameraEndedCleanup = null;
    this.shareEndedCleanup = null;
    this.cameraPublication = undefined;
    this.sharePublication = undefined;
    this.remoteStreams = [];
    this.remoteTracks.clear();
    this.speakingById.clear();
    this.audio?.detachAll();
    if (this.lastLocalSpeaking && channelId != null)
      void this.dependencies.postSpeaking(channelId, false);
    this.lastLocalSpeaking = false;
    this.publishReset();
  }

  private async disposeCurrentRoom(
    camera = this.cameraPublication,
    share = this.sharePublication,
  ): Promise<void> {
    const room = this.room;
    if (!room) return;
    this.room = null;
    await this.disposeRoom(room, camera, share);
  }

  private async disposeRoom(
    room: Room,
    ownedCamera?: LocalTrackPublication,
    ownedShare?: LocalTrackPublication,
  ): Promise<void> {
    if (this.disposedRooms.has(room)) return;
    this.disposedRooms.add(room);
    this.roomListenerCleanup.get(room)?.();
    this.roomListenerCleanup.delete(room);
    for (const cleanup of this.participantCleanup.get(room)?.values() ?? []) cleanup();
    this.participantCleanup.delete(room);
    const gain = this.gain?.room === room ? this.gain.handle : null;
    if (this.gain?.room === room) this.gain = null;
    await gain?.dispose().catch(() => undefined);
    const camera =
      ownedCamera ??
      (this.room === room ? this.cameraPublication : undefined) ??
      room.localParticipant.getTrackPublication(Track.Source.Camera);
    if (camera?.track) {
      await room.localParticipant.unpublishTrack(camera.track, true).catch(() => undefined);
      try {
        camera.track.stop();
      } catch {
        /* best effort */
      }
    }
    const share =
      ownedShare ??
      (this.room === room ? this.sharePublication : undefined) ??
      room.localParticipant.getTrackPublication(Track.Source.ScreenShare);
    if (share) await room.localParticipant.setScreenShareEnabled(false).catch(() => undefined);
    // Audio was detached synchronously when this room stopped being current.
    // Do not detach here after awaited media cleanup: a replacement room may
    // already own tracks in the shared router.
    await room.disconnect().catch(() => undefined);
  }

  private wireRoom(room: Room, channelId: number, epoch: number): void {
    const onTrackPublished = (
      publication: RemoteTrackPublication,
      participant: RemoteParticipant,
    ) => this.updateSubscription(publication, participant);
    const onTrackSubscribed = (
      track: unknown,
      publication: RemoteTrackPublication,
      participant: RemoteParticipant,
    ) => {
      if (!this.current(epoch, room)) return;
      if (track instanceof RemoteAudioTrack) this.audio?.attach(track);
      if (track instanceof RemoteVideoTrack) this.acceptVideoTrack(track, publication, participant);
    };
    const onTrackUnpublished = (
      publication: RemoteTrackPublication,
      participant: RemoteParticipant,
    ) => {
      publication.setEnabled(false);
      publication.setSubscribed(false);
      this.removePublicationTrack(publication, participant);
    };
    const onTrackUnsubscribed = (track: unknown) => {
      if (track instanceof RemoteAudioTrack) this.audio?.detach(track);
      if (track instanceof RemoteVideoTrack) this.removeVideoTrack(track);
    };
    const onLocalTrackUnpublished = (publication: LocalTrackPublication) => {
      if (
        isCameraPublication(publication) &&
        (!this.cameraPublication || this.cameraPublication === publication)
      )
        this.clearCamera(this.snapshot.cameraStatus === "stopping" ? "stopping" : "off");
      if (
        isSharePublication(publication) &&
        (!this.sharePublication || this.sharePublication === publication)
      )
        this.clearShare(this.snapshot.screenShareStatus === "stopping" ? "stopping" : "off");
    };
    const onDisconnected = () => {
      if (this.room !== room || this.epoch !== epoch) return;
      ++this.epoch;
      ++this.cameraEpoch;
      ++this.shareEpoch;
      const camera = this.cameraPublication;
      const share = this.sharePublication;
      this.room = null;
      this.clearTransitionState(channelId);
      void this.disposeRoom(room, camera, share);
    };
    const onParticipantConnected = (participant: RemoteParticipant) => {
      this.wireSpeaking(participant, false, channelId, epoch);
      participant.trackPublications.forEach((publication) =>
        this.updateSubscription(publication, participant),
      );
    };
    const onParticipantDisconnected = (participant: RemoteParticipant) => {
      const participantCleanups = this.participantCleanup.get(room);
      participantCleanups?.get(participant)?.();
      participantCleanups?.delete(participant);
      this.remoteStreams = this.remoteStreams.filter(
        (stream) => stream.participant_identity !== participant.identity,
      );
      const watched = this.snapshot.watchedScreenShare;
      if (watched?.participant_identity === participant.identity)
        this.stopWatchingScreenShareNow(room);
      const id = Number(participant.identity);
      if (Number.isFinite(id) && this.speakingById.delete(id))
        this.publish({ speakingUserIds: this.speakingById });
      this.rebuildCameraTiles();
    };

    room.on(RoomEvent.TrackPublished, onTrackPublished);
    room.on(RoomEvent.TrackSubscribed, onTrackSubscribed);
    room.on(RoomEvent.TrackUnpublished, onTrackUnpublished);
    room.on(RoomEvent.TrackUnsubscribed, onTrackUnsubscribed);
    room.on(RoomEvent.LocalTrackUnpublished, onLocalTrackUnpublished);
    room.on(RoomEvent.Disconnected, onDisconnected);
    room.on(RoomEvent.ParticipantConnected, onParticipantConnected);
    room.on(RoomEvent.ParticipantDisconnected, onParticipantDisconnected);
    this.roomListenerCleanup.set(room, () => {
      room.off(RoomEvent.TrackPublished, onTrackPublished);
      room.off(RoomEvent.TrackSubscribed, onTrackSubscribed);
      room.off(RoomEvent.TrackUnpublished, onTrackUnpublished);
      room.off(RoomEvent.TrackUnsubscribed, onTrackUnsubscribed);
      room.off(RoomEvent.LocalTrackUnpublished, onLocalTrackUnpublished);
      room.off(RoomEvent.Disconnected, onDisconnected);
      room.off(RoomEvent.ParticipantConnected, onParticipantConnected);
      room.off(RoomEvent.ParticipantDisconnected, onParticipantDisconnected);
    });
  }

  private wireSpeaking(
    participant: Participant,
    local: boolean,
    channelId: number,
    epoch: number,
  ): void {
    let cleanups = this.participantCleanup.get(this.room as Room);
    if (!cleanups) {
      cleanups = new Map();
      if (this.room) this.participantCleanup.set(this.room, cleanups);
    }
    if (cleanups.has(participant)) return;
    const id = Number(participant.identity);
    if (!Number.isFinite(id)) return;
    const handler = (speaking: boolean) => {
      if (!this.current(epoch)) return;
      const had = this.speakingById.has(id);
      if (had === speaking) return;
      if (speaking) this.speakingById.add(id);
      else this.speakingById.delete(id);
      this.publish({ speakingUserIds: this.speakingById });
      if (local && speaking !== this.lastLocalSpeaking) {
        this.lastLocalSpeaking = speaking;
        void this.dependencies.postSpeaking(channelId, speaking);
      }
    };
    participant.on(ParticipantEvent.IsSpeakingChanged, handler);
    cleanups.set(participant, () => participant.off(ParticipantEvent.IsSpeakingChanged, handler));
  }

  private enqueueControl(operation: () => Promise<void>): Promise<void> {
    const result = this.controlQueue.catch(() => undefined).then(operation);
    this.controlQueue = result.catch(() => undefined);
    return result;
  }

  toggleMuted = (): Promise<void> =>
    this.enqueueControl(async () => {
      const room = this.room;
      const epoch = this.epoch;
      const previousMuted = this.desiredMuted;
      const previousDeafened = this.desiredDeafened;
      const previousBefore = this.mutedBeforeDeafen;
      const muted = !previousMuted;
      const deafened = previousDeafened && muted;
      this.desiredMuted = muted;
      this.desiredDeafened = deafened;
      if (previousDeafened && !deafened) this.mutedBeforeDeafen = null;
      const undeafenFirst = previousDeafened && !deafened && !muted;
      if (undeafenFirst) this.audio?.setDeafened(false);
      try {
        if (room) await room.localParticipant.setMicrophoneEnabled(!muted);
      } catch (error) {
        if (!this.current(epoch, room ?? undefined)) return;
        this.desiredMuted = previousMuted;
        this.desiredDeafened = previousDeafened;
        this.mutedBeforeDeafen = previousBefore;
        if (undeafenFirst) this.audio?.setDeafened(previousDeafened);
        throw error;
      }
      if (!this.current(epoch, room ?? undefined)) return;
      this.audio?.setDeafened(deafened);
      this.publish({ muted, deafened });
      await this.dependencies.postStatus(muted, deafened);
    });

  toggleDeafened = (): Promise<void> =>
    this.enqueueControl(async () => {
      const room = this.room;
      const epoch = this.epoch;
      const previousMuted = this.desiredMuted;
      const previousDeafened = this.desiredDeafened;
      const previousBefore = this.mutedBeforeDeafen;
      const deafened = !previousDeafened;
      const muted = deafened ? true : (this.mutedBeforeDeafen ?? previousMuted);
      this.desiredMuted = muted;
      this.desiredDeafened = deafened;
      this.mutedBeforeDeafen = deafened ? previousMuted : null;
      const undeafenFirst = previousDeafened && !deafened && !muted;
      if (undeafenFirst) this.audio?.setDeafened(false);
      try {
        if (room && muted !== previousMuted)
          await room.localParticipant.setMicrophoneEnabled(!muted);
      } catch (error) {
        if (!this.current(epoch, room ?? undefined)) return;
        this.desiredMuted = previousMuted;
        this.desiredDeafened = previousDeafened;
        this.mutedBeforeDeafen = previousBefore;
        if (undeafenFirst) this.audio?.setDeafened(previousDeafened);
        throw error;
      }
      if (!this.current(epoch, room ?? undefined)) return;
      this.audio?.setDeafened(deafened);
      this.publish({ muted, deafened });
      await this.dependencies.postStatus(muted, deafened);
    });

  startCamera = (): Promise<void> => {
    if (this.snapshot.cameraStatus === "starting" && this.cameraStartPromise)
      return this.cameraStartPromise;
    const epoch = this.epoch;
    const operation = ++this.cameraEpoch;
    const promise = this.enqueueCamera(() => this.startCameraInternal(epoch, operation));
    this.cameraStartPromise = promise;
    void promise.then(
      () => {
        if (this.cameraStartPromise === promise) this.cameraStartPromise = null;
      },
      () => {
        if (this.cameraStartPromise === promise) this.cameraStartPromise = null;
      },
    );
    return promise;
  };

  private async startCameraInternal(epoch: number, operation: number): Promise<void> {
    if (epoch !== this.epoch) return;
    const room = this.room;
    if (!room || this.snapshot.activeChannelId == null)
      return this.fail("Join a voice channel before turning on your camera.");
    if (this.snapshot.cameraStatus !== "off") return;
    this.publish({ cameraStatus: "starting", error: null });
    try {
      const options: VideoCaptureOptions | undefined = this.preferences.cameraDeviceId
        ? { deviceId: { exact: this.preferences.cameraDeviceId } }
        : undefined;
      const result = await room.localParticipant.setCameraEnabled(true, options);
      const publication = result ?? room.localParticipant.getTrackPublication(Track.Source.Camera);
      if (!publication || !isCameraPublication(publication) || !cameraTrack(publication))
        throw new Error("Camera track was not published");
      if (!this.current(epoch, room) || operation !== this.cameraEpoch) {
        await this.unpublishCamera(room, publication);
        return;
      }
      this.bindCamera(publication, room, epoch);
    } catch (error) {
      const publication = room.localParticipant.getTrackPublication(Track.Source.Camera);
      if (publication) await this.unpublishCamera(room, publication);
      if (this.current(epoch, room) && operation === this.cameraEpoch) {
        this.clearCamera();
        return this.fail(cameraError(error));
      }
    }
  }

  stopCamera = (): Promise<void> => {
    const epoch = this.epoch;
    ++this.cameraEpoch;
    return this.enqueueCamera(() => this.stopCameraInternal(epoch));
  };

  private enqueueCamera(operation: () => Promise<void>): Promise<void> {
    const result = this.cameraQueue ? this.cameraQueue.then(operation) : operation();
    let pending: Promise<void>;
    const exposed = result.finally(() => {
      if (this.cameraQueue === pending) this.cameraQueue = null;
    });
    pending = exposed.catch(() => undefined);
    this.cameraQueue = pending;
    return exposed;
  }

  private async stopCameraInternal(epoch: number): Promise<void> {
    if (epoch !== this.epoch) return;
    const room = this.room;
    const publication =
      this.cameraPublication ?? room?.localParticipant.getTrackPublication(Track.Source.Camera);
    this.publish({ cameraStatus: publication ? "stopping" : "off", error: null });
    this.clearCamera(publication ? "stopping" : "off");
    if (!room || !publication) return;
    try {
      await this.unpublishCamera(room, publication);
      if (this.current(epoch, room)) this.publish({ cameraStatus: "off" });
    } catch (error) {
      if (this.current(epoch, room)) {
        this.publish({ cameraStatus: "off" });
        return this.fail(`Could not stop camera: ${errorMessage(error, "Unknown error")}`);
      }
    }
  }

  private async unpublishCamera(room: Room, publication: LocalTrackPublication): Promise<void> {
    if (publication.track) await room.localParticipant.unpublishTrack(publication.track, true);
    else await room.localParticipant.setCameraEnabled(false);
    try {
      publication.track?.stop();
    } catch {
      /* best effort */
    }
  }

  private bindCamera(publication: LocalTrackPublication, room: Room, epoch: number): void {
    this.cameraEndedCleanup?.();
    this.cameraPublication = publication;
    const track = cameraTrack(publication);
    const media = publication.track?.mediaStreamTrack;
    if (media) {
      const ended = () => {
        if (this.current(epoch, room) && this.cameraPublication === publication)
          void this.stopCamera();
      };
      media.addEventListener("ended", ended, { once: true });
      this.cameraEndedCleanup = () => media.removeEventListener("ended", ended);
    }
    this.publish({ cameraStatus: "on", localCameraTrack: track });
  }

  private clearCamera(status: "off" | "stopping" = "off"): void {
    this.cameraEndedCleanup?.();
    this.cameraEndedCleanup = null;
    this.cameraPublication = undefined;
    this.publish({ cameraStatus: status, localCameraTrack: null });
  }

  startScreenShare = (): Promise<void> => {
    if (this.snapshot.screenShareStatus === "starting" && this.shareStartPromise)
      return this.shareStartPromise;
    const epoch = this.epoch;
    const operation = ++this.shareEpoch;
    const promise = this.enqueueShare(() => this.startScreenShareInternal(epoch, operation));
    this.shareStartPromise = promise;
    void promise.then(
      () => {
        if (this.shareStartPromise === promise) this.shareStartPromise = null;
      },
      () => {
        if (this.shareStartPromise === promise) this.shareStartPromise = null;
      },
    );
    return promise;
  };

  private async startScreenShareInternal(epoch: number, operation: number): Promise<void> {
    if (epoch !== this.epoch) return;
    const room = this.room;
    if (!room || this.snapshot.activeChannelId == null)
      return this.fail("Join a voice channel before sharing your screen.");
    if (this.snapshot.screenShareStatus !== "off") return;
    this.publish({ screenShareStatus: "starting", error: null });
    try {
      const result = await room.localParticipant.setScreenShareEnabled(
        true,
        SCREEN_SHARE_CAPTURE_OPTIONS,
      );
      const publication =
        result ?? room.localParticipant.getTrackPublication(Track.Source.ScreenShare);
      if (!publication || !isSharePublication(publication))
        throw new Error("Screen share track was not published");
      if (!this.current(epoch, room) || operation !== this.shareEpoch) {
        await room.localParticipant.setScreenShareEnabled(false).catch(() => undefined);
        return;
      }
      this.bindShare(publication, room, epoch);
    } catch (error) {
      await room.localParticipant.setScreenShareEnabled(false).catch(() => undefined);
      if (this.current(epoch, room) && operation === this.shareEpoch) {
        this.clearShare();
        return this.fail(shareError(error));
      }
    }
  }

  stopScreenShare = (): Promise<void> => {
    const epoch = this.epoch;
    ++this.shareEpoch;
    return this.enqueueShare(() => this.stopScreenShareInternal(epoch));
  };

  private enqueueShare(operation: () => Promise<void>): Promise<void> {
    const result = this.shareQueue ? this.shareQueue.then(operation) : operation();
    let pending: Promise<void>;
    const exposed = result.finally(() => {
      if (this.shareQueue === pending) this.shareQueue = null;
    });
    pending = exposed.catch(() => undefined);
    this.shareQueue = pending;
    return exposed;
  }

  private async stopScreenShareInternal(epoch: number): Promise<void> {
    if (epoch !== this.epoch) return;
    const room = this.room;
    const visible = Boolean(
      this.sharePublication ?? room?.localParticipant.getTrackPublication(Track.Source.ScreenShare),
    );
    this.publish({ screenShareStatus: visible ? "stopping" : "off", error: null });
    this.clearShare(visible ? "stopping" : "off");
    if (!room || !visible) return;
    try {
      await room.localParticipant.setScreenShareEnabled(false);
      if (this.current(epoch, room))
        this.publish({ screenShareStatus: "off", screenSharePublicationVisible: false });
    } catch (error) {
      if (this.current(epoch, room)) {
        this.publish({ screenShareStatus: "off", screenSharePublicationVisible: false });
        return this.fail(`Could not stop screen share: ${errorMessage(error, "Unknown error")}`);
      }
    }
  }

  private bindShare(publication: LocalTrackPublication, room: Room, epoch: number): void {
    this.shareEndedCleanup?.();
    this.sharePublication = publication;
    const media = publication.track?.mediaStreamTrack;
    if (media) {
      const ended = () => {
        if (this.current(epoch, room) && this.sharePublication === publication)
          void this.stopScreenShare();
      };
      media.addEventListener("ended", ended, { once: true });
      this.shareEndedCleanup = () => media.removeEventListener("ended", ended);
    }
    this.publish({ screenShareStatus: "on", screenSharePublicationVisible: true });
  }

  private clearShare(status: "off" | "stopping" = "off"): void {
    this.shareEndedCleanup?.();
    this.shareEndedCleanup = null;
    this.sharePublication = undefined;
    this.publish({
      screenShareStatus: status,
      screenSharePublicationVisible:
        status === "stopping" ? this.snapshot.screenSharePublicationVisible : false,
    });
  }

  syncRemoteCameraStreams = (channelId: number, streams: readonly CameraStream[]): void => {
    const room = this.room;
    if (!room || this.snapshot.activeChannelId !== channelId) return;
    const identity = room.localParticipant.identity;
    const byKey = new Map<string, CameraStream>();
    for (const stream of streams) {
      if (
        stream.channel_id === channelId &&
        stream.source === Track.Source.Camera &&
        stream.participant_identity !== identity
      )
        byKey.set(cameraKey(stream), stream);
    }
    this.remoteStreams = sortCameraStreams([...byKey.values()]);
    const retained = new Set(this.remoteStreams.map(cameraKey));
    for (const key of this.remoteTracks.keys())
      if (!retained.has(key)) this.remoteTracks.delete(key);
    this.updateAllSubscriptions(room);
    this.rebuildCameraTiles();
  };

  watchScreenShare = async (stream: ScreenShareStream): Promise<void> => {
    const room = this.room;
    if (!room || this.snapshot.activeChannelId !== stream.channel_id)
      return this.fail("Join the sharer's voice channel before watching their screen.");
    if (
      this.snapshot.watchedScreenShare &&
      !isSameScreenShare(this.snapshot.watchedScreenShare, stream)
    )
      this.stopWatchingScreenShareNow(room);
    this.publish({ watchedScreenShare: stream, watchedScreenShareTrack: null, error: null });
    this.updateAllSubscriptions(room);
  };

  stopWatchingScreenShare = async (): Promise<void> => {
    this.stopWatchingScreenShareNow(this.room);
  };

  private stopWatchingScreenShareNow(room: Room | null): void {
    this.publish({ watchedScreenShare: null, watchedScreenShareTrack: null });
    this.updateAllSubscriptions(room);
  }

  private updateAllSubscriptions(room: Room | null): void {
    room?.remoteParticipants.forEach((participant) =>
      participant.trackPublications.forEach((publication) =>
        this.updateSubscription(publication, participant),
      ),
    );
  }

  private updateSubscription(
    publication: RemoteTrackPublication,
    participant: RemoteParticipant,
  ): void {
    if (publication.kind === Track.Kind.Audio && publication.source === Track.Source.Microphone) {
      publication.setSubscribed(true);
      return;
    }
    if (publication.kind !== Track.Kind.Video) return;
    if (publication.source === Track.Source.ScreenShare) {
      const watched = this.snapshot.watchedScreenShare;
      const selected = Boolean(
        watched &&
        watched.participant_identity === participant.identity &&
        watched.track_sid === publication.trackSid,
      );
      publication.setEnabled(selected);
      publication.setSubscribed(selected);
      if (selected && publication.track instanceof RemoteVideoTrack)
        this.publish({ watchedScreenShareTrack: publication.track });
      return;
    }
    if (publication.source !== Track.Source.Camera) return;
    const stream = this.remoteStreams.find(
      (candidate) =>
        candidate.participant_identity === participant.identity &&
        candidate.track_sid === publication.trackSid,
    );
    const selected = stream != null;
    publication.setEnabled(selected);
    publication.setSubscribed(selected);
    if (stream && publication.track instanceof RemoteVideoTrack)
      this.remoteTracks.set(cameraKey(stream), publication.track);
    this.rebuildCameraTiles();
  }

  private acceptVideoTrack(
    track: RemoteVideoTrack,
    publication: RemoteTrackPublication,
    participant: RemoteParticipant,
  ): void {
    if (publication.source === Track.Source.ScreenShare) {
      const watched = this.snapshot.watchedScreenShare;
      if (
        watched?.participant_identity === participant.identity &&
        watched.track_sid === publication.trackSid
      )
        this.publish({ watchedScreenShareTrack: track });
    } else if (publication.source === Track.Source.Camera) {
      const stream = this.remoteStreams.find(
        (candidate) =>
          candidate.participant_identity === participant.identity &&
          candidate.track_sid === publication.trackSid,
      );
      if (stream) {
        this.remoteTracks.set(cameraKey(stream), track);
        this.rebuildCameraTiles();
      }
    }
  }

  private removePublicationTrack(
    publication: RemoteTrackPublication,
    participant: RemoteParticipant,
  ): void {
    if (publication.source === Track.Source.ScreenShare) {
      const watched = this.snapshot.watchedScreenShare;
      if (
        watched?.participant_identity === participant.identity &&
        watched.track_sid === publication.trackSid
      )
        this.publish({ watchedScreenShare: null, watchedScreenShareTrack: null });
    }
    if (publication.source === Track.Source.Camera) {
      for (const stream of this.remoteStreams)
        if (
          stream.participant_identity === participant.identity &&
          stream.track_sid === publication.trackSid
        )
          this.remoteTracks.delete(cameraKey(stream));
      this.rebuildCameraTiles();
    }
  }

  private removeVideoTrack(track: RemoteVideoTrack): void {
    if (this.snapshot.watchedScreenShareTrack === track)
      this.publish({ watchedScreenShareTrack: null });
    for (const [key, value] of this.remoteTracks)
      if (value === track) this.remoteTracks.delete(key);
    this.rebuildCameraTiles();
  }

  private rebuildCameraTiles(): void {
    const tiles: readonly RemoteCameraTile[] = this.remoteStreams.map((stream) => ({
      stream,
      track: this.remoteTracks.get(cameraKey(stream)) ?? null,
    }));
    this.publish({ remoteCameraTiles: tiles });
  }

  private fail(message: string): never {
    this.publish({ error: message });
    throw new Error(message);
  }
}
