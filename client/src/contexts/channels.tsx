import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { listChannels, reorderChannels, type Channel } from "../api";
import type { ResourceStatus } from "../hooks/use-resource";
import { useAuth } from "./auth";
import { useEvents } from "./events";

export interface ChannelsContextValue {
  readonly channels: readonly Channel[];
  status: ResourceStatus;
  // `null` is the explicit no-error sentinel in the public context contract.
  // oxlint-disable-next-line typescript/no-redundant-type-constituents
  error: unknown | null;
  reordering: boolean;
  refresh: () => Promise<void>;
  reorder: (ids: readonly number[]) => Promise<void>;
}

interface ChannelsState {
  key: number | null;
  channels: readonly Channel[];
  status: ResourceStatus;
  // oxlint-disable-next-line typescript/no-redundant-type-constituents
  error: unknown | null;
}

type JournalInput =
  | { type: "created"; channel: Channel }
  | { type: "reordered"; channels: readonly Channel[] };
type JournalAction = JournalInput & { sequence: number };

const ChannelsContext = createContext<ChannelsContextValue | undefined>(undefined);

function sortByPosition(list: readonly Channel[]): Channel[] {
  return [...list].sort((a, b) => a.position - b.position || a.id - b.id);
}

function upsertChannel(list: readonly Channel[], channel: Channel): Channel[] {
  const existing = list.findIndex((item) => item.id === channel.id);
  if (existing < 0) return sortByPosition([...list, channel]);
  if (list[existing] === channel) return sortByPosition(list);
  const next = [...list];
  next[existing] = channel;
  return sortByPosition(next);
}

function applyJournal(list: readonly Channel[], journal: readonly JournalAction[]): Channel[] {
  return journal.reduce<Channel[]>((current, action) => {
    return action.type === "created"
      ? upsertChannel(current, action.channel)
      : sortByPosition(action.channels);
  }, sortByPosition(list));
}

function isPermutation(ids: readonly number[], channels: readonly Channel[]): boolean {
  if (ids.length !== channels.length || new Set(ids).size !== ids.length) return false;
  const currentIds = new Set(channels.map((channel) => channel.id));
  return ids.every((id) => currentIds.has(id));
}

function optimisticOrder(ids: readonly number[], channels: readonly Channel[]): Channel[] {
  const byId = new Map(channels.map((channel) => [channel.id, channel]));
  const ordered = ids.map((id) => {
    const channel = byId.get(id);
    if (!channel) throw new Error("Channel reorder IDs must be a permutation");
    byId.delete(id);
    return channel;
  });
  // A channel created while a local reorder is pending was not part of the
  // validated permutation. Keep it after the locally ordered channels.
  ordered.push(...sortByPosition([...byId.values()]));
  return ordered.map((channel, position) => ({ ...channel, position }));
}

interface PendingReorder {
  generation: number;
  ids: readonly number[];
}

export function ChannelsProvider(props: { children: ReactNode }) {
  const auth = useAuth();
  const events = useEvents();
  const authenticatedKey = auth.status === "authenticated" ? (auth.user?.id ?? null) : null;
  const keyRef = useRef(authenticatedKey);
  keyRef.current = authenticatedKey;
  const [state, setState] = useState<ChannelsState>({
    key: null,
    channels: [],
    status: "idle",
    error: null,
  });
  const stateRef = useRef(state);
  stateRef.current = state;
  const [reordering, setReordering] = useState(false);
  const mountedRef = useRef(false);
  const requestGenerationRef = useRef(0);
  const requestControllerRef = useRef<AbortController | null>(null);
  const journalRef = useRef<JournalAction[]>([]);
  const journalSequenceRef = useRef(0);
  const confirmedChannelsRef = useRef<readonly Channel[]>([]);
  const pendingReordersRef = useRef<PendingReorder[]>([]);
  const reorderGenerationRef = useRef(0);
  const latestReorderIntentRef = useRef(0);
  const confirmedReorderGenerationRef = useRef(0);

  const withNewestOptimisticOrder = useCallback((channels: readonly Channel[]) => {
    const newest = pendingReordersRef.current.find(
      (pending) => pending.generation === latestReorderIntentRef.current,
    );
    return newest ? optimisticOrder(newest.ids, channels) : sortByPosition(channels);
  }, []);

  const cancelRequest = useCallback(() => {
    ++requestGenerationRef.current;
    requestControllerRef.current?.abort();
    requestControllerRef.current = null;
  }, []);

  const recordAndApply = useCallback(
    (action: JournalInput) => {
      const entry = { ...action, sequence: ++journalSequenceRef.current } as JournalAction;
      journalRef.current.push(entry);
      confirmedChannelsRef.current = applyJournal(confirmedChannelsRef.current, [entry]);
      setState((current) => {
        if (current.key !== keyRef.current || current.key === null) return current;
        return {
          ...current,
          channels: withNewestOptimisticOrder(confirmedChannelsRef.current),
        };
      });
    },
    [withNewestOptimisticOrder],
  );

  const refresh = useCallback(async (): Promise<void> => {
    const key = keyRef.current;
    if (key === null) return;

    const generation = ++requestGenerationRef.current;
    requestControllerRef.current?.abort();
    const controller = new AbortController();
    requestControllerRef.current = controller;
    const startedAfterSequence = journalSequenceRef.current;

    setState((current) => ({
      key,
      channels: current.key === key ? current.channels : [],
      status: "loading",
      error: null,
    }));

    try {
      const snapshot = await listChannels(controller.signal);
      if (
        !mountedRef.current ||
        generation !== requestGenerationRef.current ||
        keyRef.current !== key
      ) {
        return;
      }
      const pendingJournal = journalRef.current.filter(
        (entry) => entry.sequence > startedAfterSequence,
      );
      confirmedChannelsRef.current = applyJournal(snapshot, pendingJournal);
      setState({
        key,
        channels: withNewestOptimisticOrder(confirmedChannelsRef.current),
        status: "ready",
        error: null,
      });
      journalRef.current = [];
    } catch (error) {
      if (
        controller.signal.aborted ||
        !mountedRef.current ||
        generation !== requestGenerationRef.current ||
        keyRef.current !== key
      ) {
        return;
      }
      setState((current) => ({
        key,
        channels: current.key === key ? current.channels : [],
        status: "error",
        error,
      }));
    } finally {
      if (generation === requestGenerationRef.current) requestControllerRef.current = null;
    }
  }, [withNewestOptimisticOrder]);

  useEffect(() => {
    mountedRef.current = true;
    const unsubscribeCreated = events.onChannelCreated((channel) => {
      if (keyRef.current !== null) recordAndApply({ type: "created", channel });
    });
    const unsubscribeReordered = events.onChannelsReordered((channels) => {
      if (keyRef.current !== null) recordAndApply({ type: "reordered", channels });
    });
    const unsubscribeConnected = events.onConnected(() => {
      // A new connection is a freshness barrier. `refresh` aborts and
      // invalidates any snapshot begun before the connection evidence.
      void refresh();
    });
    return () => {
      mountedRef.current = false;
      unsubscribeCreated();
      unsubscribeReordered();
      unsubscribeConnected();
      cancelRequest();
    };
  }, [cancelRequest, events, recordAndApply, refresh]);

  useEffect(() => {
    cancelRequest();
    journalRef.current = [];
    confirmedChannelsRef.current = [];
    pendingReordersRef.current = [];
    ++reorderGenerationRef.current;
    latestReorderIntentRef.current = reorderGenerationRef.current;
    confirmedReorderGenerationRef.current = 0;
    setReordering(false);

    if (authenticatedKey === null) {
      setState({ key: null, channels: [], status: "idle", error: null });
      return;
    }

    setState({ key: authenticatedKey, channels: [], status: "loading", error: null });
    void refresh();
  }, [authenticatedKey, cancelRequest, refresh]);

  const reorder = useCallback(
    async (ids: readonly number[]): Promise<void> => {
      const key = keyRef.current;
      const current = stateRef.current.key === key ? stateRef.current.channels : [];
      if (key === null || !isPermutation(ids, current)) {
        throw new Error("Channel reorder IDs must be a permutation of the current channels");
      }

      const generation = ++reorderGenerationRef.current;
      latestReorderIntentRef.current = generation;
      pendingReordersRef.current.push({ generation, ids: [...ids] });
      setReordering(true);
      setState((existing) => ({
        ...existing,
        channels: withNewestOptimisticOrder(confirmedChannelsRef.current),
      }));

      try {
        const updated = await reorderChannels([...ids]);
        if (keyRef.current === key && generation > confirmedReorderGenerationRef.current) {
          confirmedReorderGenerationRef.current = generation;
          recordAndApply({ type: "reordered", channels: updated });
        }
      } finally {
        if (keyRef.current === key) {
          pendingReordersRef.current = pendingReordersRef.current.filter(
            (pending) => pending.generation !== generation,
          );
          setReordering(pendingReordersRef.current.length > 0);
          setState((existing) => ({
            ...existing,
            channels: withNewestOptimisticOrder(confirmedChannelsRef.current),
          }));
        }
      }
    },
    [recordAndApply, withNewestOptimisticOrder],
  );

  const visibleState =
    state.key === authenticatedKey
      ? state
      : {
          key: authenticatedKey,
          channels: [] as readonly Channel[],
          status: authenticatedKey === null ? ("idle" as const) : ("loading" as const),
          error: null,
        };
  const value = useMemo<ChannelsContextValue>(
    () => ({
      channels: visibleState.channels,
      status: visibleState.status,
      error: visibleState.error,
      reordering,
      refresh,
      reorder,
    }),
    [refresh, reorder, reordering, visibleState.channels, visibleState.error, visibleState.status],
  );

  return <ChannelsContext.Provider value={value}>{props.children}</ChannelsContext.Provider>;
}

export function useOptionalChannels() {
  return useContext(ChannelsContext);
}

export function useChannels() {
  const ctx = useOptionalChannels();
  if (!ctx) throw new Error("useChannels must be used inside ChannelsProvider");
  return ctx;
}
