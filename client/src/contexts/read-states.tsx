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
import { useAuth } from "./auth";
import { useOptionalEvents } from "./events";
import {
  applyIncomingTopLevelMessage,
  applyReadStateSnapshot,
  applyReadStateUpdate,
  channelHasUnread,
  channelMentionCount,
  readStateForChannel,
  type ReadStateByChannel,
} from "../read-states/read-state-transitions";

interface ReadStatesContextValue {
  states: ReadStateByChannel;
  readState: (channelId: number) => ReadStateSummary | undefined;
  hasUnread: (channelId: number) => boolean;
  mentionCount: (channelId: number) => number;
  markRead: (channelId: number, lastVisibleMessageId: number) => Promise<void>;
  refetchSnapshot: () => Promise<void>;
}

const ReadStatesContext = createContext<ReadStatesContextValue | undefined>(undefined);

export function ReadStatesProvider(props: { children: ReactNode }) {
  const auth = useAuth();
  const events = useOptionalEvents();
  const [states, setStates] = useState<ReadStateByChannel>({});
  const requestId = useRef(0);
  const currentUserRef = useRef(auth.user());
  currentUserRef.current = auth.user();

  const refetchSnapshot = useCallback(async (): Promise<void> => {
    const currentUser = currentUserRef.current;
    if (!currentUser) {
      setStates({});
      return;
    }

    const id = ++requestId.current;
    try {
      const snapshot = await listReadStates();
      if (id === requestId.current) setStates(applyReadStateSnapshot(snapshot));
    } catch (err) {
      console.warn("failed to load read-state snapshot", err);
    }
  }, []);

  const markRead = useCallback(
    async (channelId: number, lastVisibleMessageId: number): Promise<void> => {
      const currentUser = currentUserRef.current;
      if (!currentUser) return;

      try {
        const summary = await markChannelRead(channelId, lastVisibleMessageId);
        setStates((current) => applyReadStateUpdate(current, summary));
      } catch (err) {
        console.warn("failed to mark channel read", err);
      }
    },
    [],
  );

  useEffect(() => {
    const currentUser = auth.user();
    if (!currentUser) {
      requestId.current += 1;
      setStates({});
      return;
    }
    void refetchSnapshot();
  }, [auth.user(), refetchSnapshot]);

  useEffect(() => {
    const cleanupCallbacks: Array<() => void> = [];
    if (events) {
      cleanupCallbacks.push(
        events.onMessage((message) => {
          const currentUser = currentUserRef.current;
          if (!currentUser) return;
          if (message.created_at == null) {
            void refetchSnapshot();
            return;
          }
          setStates((current) => applyIncomingTopLevelMessage(current, message, currentUser.id));
        }),
      );
      cleanupCallbacks.push(
        events.onReadStateUpdated((summary) => {
          setStates((current) => applyReadStateUpdate(current, summary));
        }),
      );
      cleanupCallbacks.push(events.onConnected(() => void refetchSnapshot()));
      cleanupCallbacks.push(events.onMessageUpdated(() => void refetchSnapshot()));
      cleanupCallbacks.push(events.onMessageDeleted(() => void refetchSnapshot()));
      cleanupCallbacks.push(events.onThreadReplyDeleted(() => void refetchSnapshot()));
    }

    const recoverSnapshot = () => {
      if (document.visibilityState !== "hidden") void refetchSnapshot();
    };
    window.addEventListener("focus", recoverSnapshot);
    document.addEventListener("visibilitychange", recoverSnapshot);

    return () => {
      cleanupCallbacks.forEach((cleanup) => cleanup());
      window.removeEventListener("focus", recoverSnapshot);
      document.removeEventListener("visibilitychange", recoverSnapshot);
    };
  }, [events, refetchSnapshot]);

  const value: ReadStatesContextValue = useMemo(
    () => ({
      states,
      readState: (channelId) => readStateForChannel(states, channelId),
      hasUnread: (channelId) => channelHasUnread(states, channelId),
      mentionCount: (channelId) => channelMentionCount(states, channelId),
      markRead,
      refetchSnapshot,
    }),
    [markRead, refetchSnapshot, states],
  );

  return <ReadStatesContext.Provider value={value}>{props.children}</ReadStatesContext.Provider>;
}

export function useReadStates() {
  const ctx = useContext(ReadStatesContext);
  if (!ctx) throw new Error("useReadStates must be used inside ReadStatesProvider");
  return ctx;
}
