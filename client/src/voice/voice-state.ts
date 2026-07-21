import type { CameraStream, ScreenShareStream } from "../api";
import { cameraKey } from "./camera";
import type { LocalVideoTrack, RemoteVideoTrack } from "livekit-client";

export type ConnectionStatus = "idle" | "connecting" | "connected";
export type MediaStatus = "off" | "starting" | "on" | "stopping";

export interface RemoteCameraTile {
  readonly stream: CameraStream;
  readonly track: RemoteVideoTrack | null;
}

/** The complete render-facing state published by a VoiceSession. */
export interface VoiceSnapshot {
  readonly activeChannelId: number | null;
  readonly connectionStatus: ConnectionStatus;
  readonly muted: boolean;
  readonly deafened: boolean;
  readonly screenShareStatus: MediaStatus;
  /** True only while LiveKit still exposes the local share publication. */
  readonly screenSharePublicationVisible: boolean;
  readonly cameraStatus: MediaStatus;
  readonly localCameraTrack: LocalVideoTrack | null;
  readonly remoteCameraTiles: readonly RemoteCameraTile[];
  readonly remoteCameraTilesByKey: ReadonlyMap<string, RemoteCameraTile>;
  readonly watchedScreenShare: ScreenShareStream | null;
  readonly watchedScreenShareTrack: RemoteVideoTrack | null;
  readonly speakingUserIds: ReadonlySet<number>;
  readonly error: string | null;
}

export interface VoiceSnapshotUpdate {
  readonly activeChannelId?: number | null;
  readonly connectionStatus?: ConnectionStatus;
  readonly muted?: boolean;
  readonly deafened?: boolean;
  readonly screenShareStatus?: MediaStatus;
  readonly screenSharePublicationVisible?: boolean;
  readonly cameraStatus?: MediaStatus;
  readonly localCameraTrack?: LocalVideoTrack | null;
  readonly remoteCameraTiles?: readonly RemoteCameraTile[];
  readonly watchedScreenShare?: ScreenShareStream | null;
  readonly watchedScreenShareTrack?: RemoteVideoTrack | null;
  readonly speakingUserIds?: ReadonlySet<number> | readonly number[];
  readonly error?: string | null;
}

const EMPTY_TILES: readonly RemoteCameraTile[] = Object.freeze([]);
const EMPTY_TILE_MAP: ReadonlyMap<string, RemoteCameraTile> = new Map();
const EMPTY_SPEAKERS: ReadonlySet<number> = new Set();

export function createVoiceSnapshot(): VoiceSnapshot {
  return Object.freeze({
    activeChannelId: null,
    connectionStatus: "idle",
    muted: false,
    deafened: false,
    screenShareStatus: "off",
    screenSharePublicationVisible: false,
    cameraStatus: "off",
    localCameraTrack: null,
    remoteCameraTiles: EMPTY_TILES,
    remoteCameraTilesByKey: EMPTY_TILE_MAP,
    watchedScreenShare: null,
    watchedScreenShareTrack: null,
    speakingUserIds: EMPTY_SPEAKERS,
    error: null,
  });
}

function sameStream(left: CameraStream, right: CameraStream): boolean {
  return (
    cameraKey(left) === cameraKey(right) &&
    left.username === right.username &&
    left.display_name === right.display_name &&
    left.avatar_url === right.avatar_url &&
    left.track_name === right.track_name &&
    left.source === right.source &&
    left.started_at === right.started_at
  );
}

function sameTiles(
  current: readonly RemoteCameraTile[],
  incoming: readonly RemoteCameraTile[],
): boolean {
  return (
    current.length === incoming.length &&
    current.every(
      (tile, index) =>
        tile.track === incoming[index]?.track &&
        incoming[index] != null &&
        sameStream(tile.stream, incoming[index].stream),
    )
  );
}

function sameSet(current: ReadonlySet<number>, incoming: ReadonlySet<number>): boolean {
  if (current.size !== incoming.size) return false;
  for (const value of current) if (!incoming.has(value)) return false;
  return true;
}

function copyTiles(tiles: readonly RemoteCameraTile[]): {
  list: readonly RemoteCameraTile[];
  byKey: ReadonlyMap<string, RemoteCameraTile>;
} {
  const byKey = new Map<string, RemoteCameraTile>();
  const list = tiles.map((tile) => {
    const copy: RemoteCameraTile = Object.freeze({ stream: tile.stream, track: tile.track });
    byKey.set(cameraKey(copy.stream), copy);
    return copy;
  });
  return { list: Object.freeze(list), byKey };
}

/**
 * Applies a session update without ever mutating a published snapshot. The
 * previous snapshot is returned when every supplied field is already current.
 */
export function updateVoiceSnapshot(
  snapshot: VoiceSnapshot,
  update: VoiceSnapshotUpdate,
): VoiceSnapshot {
  let changed = false;
  const next = { ...snapshot };

  const assign = <K extends keyof VoiceSnapshot>(key: K, value: VoiceSnapshot[K]): void => {
    if (snapshot[key] === value) return;
    next[key] = value;
    changed = true;
  };

  if (update.activeChannelId !== undefined) assign("activeChannelId", update.activeChannelId);
  if (update.connectionStatus !== undefined) assign("connectionStatus", update.connectionStatus);
  if (update.muted !== undefined) assign("muted", update.muted);
  if (update.deafened !== undefined) assign("deafened", update.deafened);
  if (update.screenShareStatus !== undefined) assign("screenShareStatus", update.screenShareStatus);
  if (update.screenSharePublicationVisible !== undefined)
    assign("screenSharePublicationVisible", update.screenSharePublicationVisible);
  if (update.cameraStatus !== undefined) assign("cameraStatus", update.cameraStatus);
  if (update.localCameraTrack !== undefined) assign("localCameraTrack", update.localCameraTrack);
  if (update.watchedScreenShare !== undefined)
    assign("watchedScreenShare", update.watchedScreenShare);
  if (update.watchedScreenShareTrack !== undefined)
    assign("watchedScreenShareTrack", update.watchedScreenShareTrack);
  if (update.error !== undefined) assign("error", update.error);

  if (
    update.remoteCameraTiles !== undefined &&
    !sameTiles(snapshot.remoteCameraTiles, update.remoteCameraTiles)
  ) {
    const copied = copyTiles(update.remoteCameraTiles);
    next.remoteCameraTiles = copied.list;
    next.remoteCameraTilesByKey = copied.byKey;
    changed = true;
  }

  if (update.speakingUserIds !== undefined) {
    const incoming =
      update.speakingUserIds instanceof Set
        ? update.speakingUserIds
        : new Set(update.speakingUserIds);
    if (!sameSet(snapshot.speakingUserIds, incoming)) {
      next.speakingUserIds = new Set(incoming);
      changed = true;
    }
  }

  return changed ? Object.freeze(next) : snapshot;
}

export function resetVoiceConnection(snapshot: VoiceSnapshot): VoiceSnapshot {
  return updateVoiceSnapshot(snapshot, {
    activeChannelId: null,
    connectionStatus: "idle",
    screenShareStatus: "off",
    screenSharePublicationVisible: false,
    cameraStatus: "off",
    localCameraTrack: null,
    remoteCameraTiles: EMPTY_TILES,
    watchedScreenShare: null,
    watchedScreenShareTrack: null,
    speakingUserIds: EMPTY_SPEAKERS,
  });
}
