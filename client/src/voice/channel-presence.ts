import type {
  CameraStream,
  CameraVideoStopped,
  ScreenShareStopped,
  ScreenShareStream,
  VoiceParticipant,
  VoiceParticipantLeft,
  VoiceParticipantSpeaking,
  VoiceParticipantStatus,
} from "../api";
import { cameraKey, sortCameraStreams } from "./camera";
import { screenShareKey } from "./screen-share";

export type ChannelPresenceStatus = "idle" | "loading" | "ready" | "error";

export type ScreenShareAnnouncement = Readonly<{
  kind: "started" | "stopped";
  stream: ScreenShareStream;
}>;

export type CameraAnnouncement = Readonly<{
  kind: "started" | "stopped";
  stream: CameraStream;
}>;

export type ChannelPresenceLiveAction =
  | {
      readonly type: "participantJoined";
      readonly generation: number;
      readonly participant: VoiceParticipant;
    }
  | {
      readonly type: "participantLeft";
      readonly generation: number;
      readonly participant: VoiceParticipantLeft;
    }
  | {
      readonly type: "participantStatusChanged";
      readonly generation: number;
      readonly status: VoiceParticipantStatus;
    }
  | {
      readonly type: "participantSpeakingChanged";
      readonly generation: number;
      readonly speaking: VoiceParticipantSpeaking;
    }
  | {
      readonly type: "screenShareStarted";
      readonly generation: number;
      readonly stream: ScreenShareStream;
    }
  | {
      readonly type: "screenShareStopped";
      readonly generation: number;
      readonly stopped: ScreenShareStopped;
      /** Optional metadata retained by an imperative watcher after discovery disappeared. */
      readonly stream?: ScreenShareStream;
    }
  | { readonly type: "cameraStarted"; readonly generation: number; readonly stream: CameraStream }
  | {
      readonly type: "cameraStopped";
      readonly generation: number;
      readonly stopped: CameraVideoStopped;
    };

export type ChannelPresenceAction =
  | { readonly type: "bootstrapStarted"; readonly channelId: number; readonly generation: number }
  | {
      readonly type: "bootstrapSucceeded";
      readonly channelId: number;
      readonly generation: number;
      readonly participants: readonly VoiceParticipant[];
      readonly screenShares: readonly ScreenShareStream[];
      readonly cameraStreams: readonly CameraStream[];
    }
  | {
      readonly type: "bootstrapFailed";
      readonly channelId: number;
      readonly generation: number;
      readonly error: unknown;
      readonly participants?: readonly VoiceParticipant[];
      readonly screenShares?: readonly ScreenShareStream[];
      readonly cameraStreams?: readonly CameraStream[];
    }
  | ChannelPresenceLiveAction;

export interface ChannelPresenceState {
  readonly channelId: number;
  readonly generation: number;
  readonly status: ChannelPresenceStatus;
  readonly error: unknown;
  readonly participants: readonly VoiceParticipant[];
  readonly screenShares: readonly ScreenShareStream[];
  readonly cameraStreams: readonly CameraStream[];
  readonly speakingUserIds: ReadonlySet<number>;
  readonly journal: readonly ChannelPresenceLiveAction[];
  readonly departedParticipantIds: ReadonlySet<number>;
  readonly stoppedScreenShareKeys: ReadonlySet<string>;
  readonly stoppedCameraKeys: ReadonlySet<string>;
  readonly screenShareAnnouncement: ScreenShareAnnouncement | null;
  readonly cameraAnnouncement: CameraAnnouncement | null;
}

const EMPTY_ARRAY: readonly never[] = Object.freeze([]);
const EMPTY_SET: ReadonlySet<never> = new Set();

export function createChannelPresenceState(channelId: number): ChannelPresenceState {
  return Object.freeze({
    channelId,
    generation: 0,
    status: "idle",
    error: null,
    participants: EMPTY_ARRAY,
    screenShares: EMPTY_ARRAY,
    cameraStreams: EMPTY_ARRAY,
    speakingUserIds: EMPTY_SET,
    journal: EMPTY_ARRAY,
    departedParticipantIds: EMPTY_SET,
    stoppedScreenShareKeys: EMPTY_SET,
    stoppedCameraKeys: EMPTY_SET,
    screenShareAnnouncement: null,
    cameraAnnouncement: null,
  });
}

function actionChannelId(action: ChannelPresenceLiveAction): number {
  switch (action.type) {
    case "participantJoined":
      return action.participant.channel_id;
    case "participantLeft":
      return action.participant.channel_id;
    case "participantStatusChanged":
      return action.status.channel_id;
    case "participantSpeakingChanged":
      return action.speaking.channel_id;
    case "screenShareStarted":
      return action.stream.channel_id;
    case "screenShareStopped":
      return action.stopped.channel_id;
    case "cameraStarted":
      return action.stream.channel_id;
    case "cameraStopped":
      return action.stopped.channel_id;
  }
}

function sameParticipant(left: VoiceParticipant, right: VoiceParticipant): boolean {
  return (
    left.user_id === right.user_id &&
    left.channel_id === right.channel_id &&
    left.username === right.username &&
    left.avatar_url === right.avatar_url &&
    left.muted === right.muted &&
    left.deafened === right.deafened
  );
}

function sortParticipants(participants: readonly VoiceParticipant[]): readonly VoiceParticipant[] {
  return [...participants].sort((left, right) => left.user_id - right.user_id);
}

function sortScreenShares(streams: readonly ScreenShareStream[]): readonly ScreenShareStream[] {
  return [...streams].sort((left, right) => {
    if (left.started_at !== right.started_at) return left.started_at - right.started_at;
    return screenShareKey(left).localeCompare(screenShareKey(right));
  });
}

function addToSet<T>(values: ReadonlySet<T>, value: T): ReadonlySet<T> {
  if (values.has(value)) return values;
  const next = new Set(values);
  next.add(value);
  return next;
}

function removeFromSet<T>(values: ReadonlySet<T>, value: T): ReadonlySet<T> {
  if (!values.has(value)) return values;
  const next = new Set(values);
  next.delete(value);
  return next;
}

function applyLiveAction(
  state: ChannelPresenceState,
  action: ChannelPresenceLiveAction,
  announce: boolean,
): ChannelPresenceState {
  switch (action.type) {
    case "participantJoined": {
      const participant = action.participant;
      const index = state.participants.findIndex(
        (current) => current.user_id === participant.user_id,
      );
      const departedParticipantIds = removeFromSet(
        state.departedParticipantIds,
        participant.user_id,
      );
      const existing = state.participants[index];
      if (
        existing &&
        sameParticipant(existing, participant) &&
        departedParticipantIds === state.departedParticipantIds
      )
        return state;
      const participants =
        index < 0
          ? sortParticipants([...state.participants, participant])
          : state.participants.map((current, currentIndex) =>
              currentIndex === index ? participant : current,
            );
      return { ...state, participants, departedParticipantIds };
    }

    case "participantLeft": {
      const userId = action.participant.user_id;
      const participants = state.participants.filter(
        (participant) => participant.user_id !== userId,
      );
      const screenShares = state.screenShares.filter((stream) => stream.sharer_user_id !== userId);
      const cameraStreams = state.cameraStreams.filter(
        (stream) => stream.sharer_user_id !== userId,
      );
      let stoppedScreenShareKeys = state.stoppedScreenShareKeys;
      let stoppedCameraKeys = state.stoppedCameraKeys;
      for (const stream of state.screenShares) {
        if (stream.sharer_user_id === userId)
          stoppedScreenShareKeys = addToSet(stoppedScreenShareKeys, screenShareKey(stream));
      }
      for (const stream of state.cameraStreams) {
        if (stream.sharer_user_id === userId)
          stoppedCameraKeys = addToSet(stoppedCameraKeys, cameraKey(stream));
      }
      const departedParticipantIds = addToSet(state.departedParticipantIds, userId);
      const speakingUserIds = removeFromSet(state.speakingUserIds, userId);
      if (
        participants.length === state.participants.length &&
        screenShares.length === state.screenShares.length &&
        cameraStreams.length === state.cameraStreams.length &&
        departedParticipantIds === state.departedParticipantIds &&
        speakingUserIds === state.speakingUserIds
      )
        return state;
      return {
        ...state,
        participants,
        screenShares,
        cameraStreams,
        speakingUserIds,
        departedParticipantIds,
        stoppedScreenShareKeys,
        stoppedCameraKeys,
      };
    }

    case "participantStatusChanged": {
      const status = action.status;
      const index = state.participants.findIndex(
        (participant) => participant.user_id === status.user_id,
      );
      if (index < 0) return state;
      const current = state.participants[index];
      if (!current) return state;
      if (current.muted === status.muted && current.deafened === status.deafened) return state;
      const participants = state.participants.map((participant, currentIndex) =>
        currentIndex === index
          ? { ...participant, muted: status.muted, deafened: status.deafened }
          : participant,
      );
      return { ...state, participants };
    }

    case "participantSpeakingChanged": {
      const { user_id: userId, speaking } = action.speaking;
      const speakingUserIds = speaking
        ? addToSet(state.speakingUserIds, userId)
        : removeFromSet(state.speakingUserIds, userId);
      return speakingUserIds === state.speakingUserIds ? state : { ...state, speakingUserIds };
    }

    case "screenShareStarted": {
      const stream = action.stream;
      const key = screenShareKey(stream);
      if (
        state.departedParticipantIds.has(stream.sharer_user_id) ||
        state.stoppedScreenShareKeys.has(key)
      )
        return state;
      if (state.screenShares.some((current) => screenShareKey(current) === key)) return state;
      return {
        ...state,
        screenShares: sortScreenShares([...state.screenShares, stream]),
        screenShareAnnouncement: announce
          ? { kind: "started", stream }
          : state.screenShareAnnouncement,
      };
    }

    case "screenShareStopped": {
      const key = screenShareKey(action.stopped);
      const wasStopped = state.stoppedScreenShareKeys.has(key);
      const removed = state.screenShares.find((stream) => screenShareKey(stream) === key);
      const stoppedScreenShareKeys = addToSet(state.stoppedScreenShareKeys, key);
      const ended = removed ?? (!wasStopped ? action.stream : undefined);
      if (!removed && !ended) {
        return stoppedScreenShareKeys === state.stoppedScreenShareKeys
          ? state
          : { ...state, stoppedScreenShareKeys };
      }
      return {
        ...state,
        screenShares: removed
          ? state.screenShares.filter((stream) => screenShareKey(stream) !== key)
          : state.screenShares,
        stoppedScreenShareKeys,
        screenShareAnnouncement:
          announce && ended ? { kind: "stopped", stream: ended } : state.screenShareAnnouncement,
      };
    }

    case "cameraStarted": {
      const stream = action.stream;
      const key = cameraKey(stream);
      if (
        state.departedParticipantIds.has(stream.sharer_user_id) ||
        state.stoppedCameraKeys.has(key)
      )
        return state;
      if (state.cameraStreams.some((current) => cameraKey(current) === key)) return state;
      return {
        ...state,
        cameraStreams: sortCameraStreams([...state.cameraStreams, stream]),
        cameraAnnouncement: announce ? { kind: "started", stream } : state.cameraAnnouncement,
      };
    }

    case "cameraStopped": {
      const key = cameraKey(action.stopped);
      const removed = state.cameraStreams.find((stream) => cameraKey(stream) === key);
      const stoppedCameraKeys = addToSet(state.stoppedCameraKeys, key);
      if (!removed) {
        return stoppedCameraKeys === state.stoppedCameraKeys
          ? state
          : { ...state, stoppedCameraKeys };
      }
      return {
        ...state,
        cameraStreams: state.cameraStreams.filter((stream) => cameraKey(stream) !== key),
        stoppedCameraKeys,
        cameraAnnouncement: announce
          ? { kind: "stopped", stream: removed }
          : state.cameraAnnouncement,
      };
    }
  }
}

function applyJournal(
  state: ChannelPresenceState,
  journal: readonly ChannelPresenceLiveAction[],
): ChannelPresenceState {
  return journal.reduce((current, action) => applyLiveAction(current, action, false), state);
}

export function channelPresenceReducer(
  state: ChannelPresenceState,
  action: ChannelPresenceAction,
): ChannelPresenceState {
  if (action.type === "bootstrapStarted") {
    if (action.channelId !== state.channelId || action.generation <= state.generation) return state;
    return {
      ...state,
      generation: action.generation,
      status: "loading",
      error: null,
      journal: EMPTY_ARRAY,
      departedParticipantIds: EMPTY_SET,
      stoppedScreenShareKeys: EMPTY_SET,
      stoppedCameraKeys: EMPTY_SET,
    };
  }

  if (action.type === "bootstrapSucceeded" || action.type === "bootstrapFailed") {
    if (
      action.channelId !== state.channelId ||
      action.generation !== state.generation ||
      state.status !== "loading"
    )
      return state;

    const reconcileParticipants = (snapshot: readonly VoiceParticipant[] | undefined) => {
      if (snapshot === undefined) return state.participants;
      const byId = new Map<number, VoiceParticipant>();
      for (const participant of snapshot) {
        if (
          participant.channel_id === state.channelId &&
          !state.departedParticipantIds.has(participant.user_id)
        )
          byId.set(participant.user_id, participant);
      }
      return sortParticipants([...byId.values()]);
    };
    const reconcileScreenShares = (snapshot: readonly ScreenShareStream[] | undefined) => {
      if (snapshot === undefined) return state.screenShares;
      const byKey = new Map<string, ScreenShareStream>();
      for (const stream of snapshot) {
        const key = screenShareKey(stream);
        if (
          stream.channel_id === state.channelId &&
          !state.departedParticipantIds.has(stream.sharer_user_id) &&
          !state.stoppedScreenShareKeys.has(key)
        )
          byKey.set(key, stream);
      }
      return sortScreenShares([...byKey.values()]);
    };
    const reconcileCameras = (snapshot: readonly CameraStream[] | undefined) => {
      if (snapshot === undefined) return state.cameraStreams;
      const byKey = new Map<string, CameraStream>();
      for (const stream of snapshot) {
        const key = cameraKey(stream);
        if (
          stream.channel_id === state.channelId &&
          !state.departedParticipantIds.has(stream.sharer_user_id) &&
          !state.stoppedCameraKeys.has(key)
        )
          byKey.set(key, stream);
      }
      return sortCameraStreams([...byKey.values()]);
    };

    const failed = action.type === "bootstrapFailed";
    const baseline: ChannelPresenceState = {
      ...state,
      status: failed ? "error" : "ready",
      error: failed ? action.error : null,
      participants: reconcileParticipants(action.participants),
      screenShares: reconcileScreenShares(action.screenShares),
      cameraStreams: reconcileCameras(action.cameraStreams),
      speakingUserIds: failed ? state.speakingUserIds : EMPTY_SET,
      journal: EMPTY_ARRAY,
    };
    const replayed = applyJournal(baseline, state.journal);
    return {
      ...replayed,
      journal: EMPTY_ARRAY,
      screenShareAnnouncement: state.screenShareAnnouncement,
      cameraAnnouncement: state.cameraAnnouncement,
    };
  }

  if (action.generation !== state.generation || actionChannelId(action) !== state.channelId)
    return state;

  const next = applyLiveAction(state, action, true);
  if (state.status !== "loading") return next;
  return { ...next, journal: [...state.journal, action] };
}

export function beginChannelPresenceBootstrap(
  state: ChannelPresenceState,
  generation: number,
): ChannelPresenceState {
  return channelPresenceReducer(state, {
    type: "bootstrapStarted",
    channelId: state.channelId,
    generation,
  });
}
