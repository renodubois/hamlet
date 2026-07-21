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
import { listReadStates, markChannelRead, type ReadStateSummary } from "../api";
import type { ResourceStatus } from "../hooks/use-resource";
import { useAuth } from "./auth";
import { useOptionalEvents } from "./events";
import {
  applyIncomingTopLevelMessage,
  applyReadStateUpdate,
  channelHasUnread,
  channelMentionCount,
  isReadStateSummaryCurrent,
  mergeReadStateSnapshot,
  readStateForChannel,
  type ReadStateByChannel,
  type ReadStateJournalEntry,
} from "../read-states/read-state-transitions";

export interface ReadStatesContextValue {
  states: ReadStateByChannel;
  status: ResourceStatus;
  // `null` is the explicit no-error sentinel in the public context contract.
  // oxlint-disable-next-line typescript/no-redundant-type-constituents
  error: unknown | null;
  readState: (channelId: number) => ReadStateSummary | undefined;
  hasUnread: (channelId: number) => boolean;
  mentionCount: (channelId: number) => number;
  markRead: (channelId: number, lastVisibleMessageId: number) => Promise<ReadStateSummary | null>;
  refresh: () => Promise<void>;
}

const ReadStatesContext = createContext<ReadStatesContextValue | undefined>(undefined);

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}

export function ReadStatesProvider(props: { children: ReactNode }) {
  const { user, status: authStatus } = useAuth();
  const events = useOptionalEvents();
  const [states, setStates] = useState<ReadStateByChannel>({});
  const [status, setStatus] = useState<ResourceStatus>("idle");
  const [error, setError] = useState<unknown>(null);
  const statesRef = useRef(states);
  statesRef.current = states;
  const currentUserRef = useRef(user);
  currentUserRef.current = user;
  const authGenerationRef = useRef(0);
  const requestIdRef = useRef(0);
  const requestControllerRef = useRef<AbortController | null>(null);
  const markReadControllersRef = useRef(new Set<AbortController>());
  const eventVersionRef = useRef(0);
  const journalRef = useRef<Array<{ version: number; entry: ReadStateJournalEntry }>>([]);

  const refresh = useCallback(async (): Promise<void> => {
    const currentUser = currentUserRef.current;
    if (!currentUser) return;

    const requestId = ++requestIdRef.current;
    const authGeneration = authGenerationRef.current;
    const startVersion = eventVersionRef.current;
    requestControllerRef.current?.abort();
    const controller = new AbortController();
    requestControllerRef.current = controller;
    setStatus("loading");
    setError(null);

    try {
      const snapshot = await listReadStates(controller.signal);
      if (
        controller.signal.aborted ||
        requestId !== requestIdRef.current ||
        authGeneration !== authGenerationRef.current ||
        currentUser.id !== currentUserRef.current?.id
      ) {
        return;
      }
      const endVersion = eventVersionRef.current;
      const entries = journalRef.current
        .filter(({ version }) => version > startVersion && version <= endVersion)
        .map(({ entry }) => entry);
      setStates(mergeReadStateSnapshot(snapshot, entries));
      journalRef.current = journalRef.current.filter(({ version }) => version > endVersion);
      setStatus("ready");
    } catch (caught) {
      if (
        controller.signal.aborted ||
        isAbortError(caught) ||
        requestId !== requestIdRef.current ||
        authGeneration !== authGenerationRef.current
      ) {
        return;
      }
      console.warn("failed to load read-state snapshot", caught);
      setError(caught);
      setStatus("error");
    } finally {
      if (requestControllerRef.current === controller) requestControllerRef.current = null;
    }
  }, []);

  const markRead = useCallback(
    async (channelId: number, lastVisibleMessageId: number): Promise<ReadStateSummary | null> => {
      const currentUser = currentUserRef.current;
      if (!currentUser) return null;
      const authGeneration = authGenerationRef.current;
      const controller = new AbortController();
      markReadControllersRef.current.add(controller);

      try {
        const summary = await markChannelRead(channelId, lastVisibleMessageId, controller.signal);
        if (
          controller.signal.aborted ||
          authGeneration !== authGenerationRef.current ||
          currentUser.id !== currentUserRef.current?.id
        ) {
          return null;
        }
        const current = statesRef.current[summary.channel_id];
        // Only an accepted response proves that this request marked the
        // requested point read. Callers must keep stale responses retryable.
        if (!isReadStateSummaryCurrent(current, summary)) return null;
        setStates((accepted) => applyReadStateUpdate(accepted, summary));
        return summary;
      } catch (caught) {
        if (!controller.signal.aborted && !isAbortError(caught)) {
          console.warn("failed to mark channel read", caught);
        }
        return null;
      } finally {
        markReadControllersRef.current.delete(controller);
      }
    },
    [],
  );

  useEffect(() => {
    authGenerationRef.current += 1;
    requestIdRef.current += 1;
    requestControllerRef.current?.abort();
    requestControllerRef.current = null;
    for (const controller of markReadControllersRef.current) controller.abort();
    markReadControllersRef.current.clear();
    eventVersionRef.current = 0;
    journalRef.current = [];
    setStates({});
    setError(null);
    setStatus("idle");
    if (authStatus === "authenticated" && currentUserRef.current) void refresh();
  }, [authStatus, refresh, user?.id]);

  useEffect(() => {
    if (!events) return;

    const record = (entry: ReadStateJournalEntry) => {
      journalRef.current.push({ version: ++eventVersionRef.current, entry });
    };
    const cleanupCallbacks = [
      events.onMessage((message) => {
        const currentUser = currentUserRef.current;
        if (!currentUser) return;
        if (message.created_at == null) {
          void refresh();
          return;
        }
        record({ kind: "message", message, currentUserId: currentUser.id });
        setStates((current) => applyIncomingTopLevelMessage(current, message, currentUser.id));
      }),
      events.onReadStateUpdated((summary) => {
        if (!currentUserRef.current) return;
        record({ kind: "summary", summary });
        setStates((current) => applyReadStateUpdate(current, summary));
      }),
      // A new connection is a freshness barrier. `refresh` aborts any snapshot
      // begun before this connection and starts a post-connection snapshot.
      events.onConnected(() => void refresh()),
      events.onMessageUpdated(() => void refresh()),
      events.onMessageDeleted(() => void refresh()),
      events.onThreadReplyDeleted(() => void refresh()),
    ];

    return () => cleanupCallbacks.forEach((cleanup) => cleanup());
  }, [events, refresh]);

  useEffect(() => {
    const recoverSnapshot = () => {
      if (document.visibilityState !== "hidden") void refresh();
    };
    window.addEventListener("focus", recoverSnapshot);
    document.addEventListener("visibilitychange", recoverSnapshot);
    return () => {
      window.removeEventListener("focus", recoverSnapshot);
      document.removeEventListener("visibilitychange", recoverSnapshot);
    };
  }, [refresh]);

  useEffect(
    () => () => {
      requestIdRef.current += 1;
      requestControllerRef.current?.abort();
      for (const controller of markReadControllersRef.current) controller.abort();
      markReadControllersRef.current.clear();
    },
    [],
  );

  const readState = useCallback(
    (channelId: number) => readStateForChannel(statesRef.current, channelId),
    [],
  );
  const hasUnread = useCallback(
    (channelId: number) => channelHasUnread(statesRef.current, channelId),
    [],
  );
  const mentionCount = useCallback(
    (channelId: number) => channelMentionCount(statesRef.current, channelId),
    [],
  );

  const value = useMemo<ReadStatesContextValue>(
    () => ({
      states,
      status,
      error,
      readState,
      hasUnread,
      mentionCount,
      markRead,
      refresh,
    }),
    [error, hasUnread, markRead, mentionCount, readState, refresh, states, status],
  );

  return <ReadStatesContext.Provider value={value}>{props.children}</ReadStatesContext.Provider>;
}

export function useReadStates() {
  const ctx = useContext(ReadStatesContext);
  if (!ctx) throw new Error("useReadStates must be used inside ReadStatesProvider");
  return ctx;
}
