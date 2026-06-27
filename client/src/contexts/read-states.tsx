import {
  createContext,
  createEffect,
  createSignal,
  onCleanup,
  onMount,
  useContext,
  type Accessor,
  type JSX,
} from "solid-js";
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
  states: Accessor<ReadStateByChannel>;
  readState: (channelId: number) => ReadStateSummary | undefined;
  hasUnread: (channelId: number) => boolean;
  mentionCount: (channelId: number) => number;
  markRead: (channelId: number, lastVisibleMessageId: number) => Promise<void>;
  refetchSnapshot: () => Promise<void>;
}

const ReadStatesContext = createContext<ReadStatesContextValue>();

export function ReadStatesProvider(props: { children: JSX.Element }) {
  const auth = useAuth();
  const events = useOptionalEvents();
  const [states, setStates] = createSignal<ReadStateByChannel>({});
  let requestId = 0;

  async function refetchSnapshot(): Promise<void> {
    const currentUser = auth.user();
    if (!currentUser) {
      setStates({});
      return;
    }

    const id = ++requestId;
    try {
      const snapshot = await listReadStates();
      if (id === requestId) setStates(applyReadStateSnapshot(snapshot));
    } catch (err) {
      console.warn("failed to load read-state snapshot", err);
    }
  }

  async function markRead(channelId: number, lastVisibleMessageId: number): Promise<void> {
    const currentUser = auth.user();
    if (!currentUser) return;

    try {
      const summary = await markChannelRead(channelId, lastVisibleMessageId);
      setStates((current) => applyReadStateUpdate(current, summary));
    } catch (err) {
      console.warn("failed to mark channel read", err);
    }
  }

  createEffect(() => {
    const currentUser = auth.user();
    if (!currentUser) {
      requestId += 1;
      setStates({});
      return;
    }
    void refetchSnapshot();
  });

  onMount(() => {
    const cleanupCallbacks: Array<() => void> = [];
    if (events) {
      cleanupCallbacks.push(
        events.onMessage((message) => {
          const currentUser = auth.user();
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

    onCleanup(() => {
      cleanupCallbacks.forEach((cleanup) => cleanup());
      window.removeEventListener("focus", recoverSnapshot);
      document.removeEventListener("visibilitychange", recoverSnapshot);
    });
  });

  const value: ReadStatesContextValue = {
    states,
    readState: (channelId) => readStateForChannel(states(), channelId),
    hasUnread: (channelId) => channelHasUnread(states(), channelId),
    mentionCount: (channelId) => channelMentionCount(states(), channelId),
    markRead,
    refetchSnapshot,
  };

  return <ReadStatesContext.Provider value={value}>{props.children}</ReadStatesContext.Provider>;
}

export function useReadStates() {
  const ctx = useContext(ReadStatesContext);
  if (!ctx) throw new Error("useReadStates must be used inside ReadStatesProvider");
  return ctx;
}
