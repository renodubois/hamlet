import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  type ReactNode,
} from "react";
import {
  messagesEventSource,
  type Channel,
  type CustomEmoji,
  type Message,
  type MessageDeleted,
  type ReadStateSummary,
  type MessageEmbedsUpdated,
  type CameraStream,
  type CameraVideoStopped,
  type MessageReactionsUpdated,
  type ScreenShareStopped,
  type ScreenShareStream,
  type SSEEvent,
  type ThreadReplyCreated,
  type ThreadReplyDeleted,
  type UserTyping,
  type VoiceParticipant,
  type VoiceParticipantLeft,
  type VoiceParticipantSpeaking,
  type VoiceParticipantStatus,
} from "../api";

type Listener<T> = (value: T) => void;

type EventKind = SSEEvent["kind"] | "connected";

// Look up the data type for a given SSEEvent kind.
type DataFor<K extends EventKind> = K extends "connected"
  ? void
  : Extract<SSEEvent, { kind: K }>["data"];

export interface EventsContextValue {
  onMessage: (cb: Listener<Message>) => () => void;
  onMessageUpdated: (cb: Listener<Message>) => () => void;
  onMessageDeleted: (cb: Listener<MessageDeleted>) => () => void;
  onMessageEmbedsUpdated: (cb: Listener<MessageEmbedsUpdated>) => () => void;
  onMessageReactionsUpdated: (cb: Listener<MessageReactionsUpdated>) => () => void;
  onChannelCreated: (cb: Listener<Channel>) => () => void;
  onChannelsReordered: (cb: Listener<Channel[]>) => () => void;
  onEmojiCreated: (cb: Listener<CustomEmoji>) => () => void;
  onEmojiUpdated: (cb: Listener<CustomEmoji>) => () => void;
  onEmojiDeleted: (cb: Listener<CustomEmoji>) => () => void;
  onVoiceParticipantJoined: (cb: Listener<VoiceParticipant>) => () => void;
  onVoiceParticipantLeft: (cb: Listener<VoiceParticipantLeft>) => () => void;
  onVoiceParticipantSpeakingChanged: (cb: Listener<VoiceParticipantSpeaking>) => () => void;
  onVoiceParticipantStatusChanged: (cb: Listener<VoiceParticipantStatus>) => () => void;
  onScreenShareStarted: (cb: Listener<ScreenShareStream>) => () => void;
  onScreenShareStopped: (cb: Listener<ScreenShareStopped>) => () => void;
  onCameraVideoStarted: (cb: Listener<CameraStream>) => () => void;
  onCameraVideoStopped: (cb: Listener<CameraVideoStopped>) => () => void;
  onUserTyping: (cb: Listener<UserTyping>) => () => void;
  onThreadReplyCreated: (cb: Listener<ThreadReplyCreated>) => () => void;
  onThreadReplyDeleted: (cb: Listener<ThreadReplyDeleted>) => () => void;
  onReadStateUpdated: (cb: Listener<ReadStateSummary>) => () => void;
  onConnected: (cb: Listener<void>) => () => void;
}

const EventsContext = createContext<EventsContextValue | undefined>(undefined);

export function EventsProvider(props: { children: ReactNode }) {
  // One Set per SSEEvent kind. Callbacks are stored as type-erased functions;
  // the typed `subscribe` wrapper below preserves the kind→data correspondence
  // from the SSEEvent discriminated union at the boundary.
  const listenersRef = useRef(new Map<EventKind, Set<Listener<unknown>>>());
  const eventSourceRef = useRef<EventSource | null>(null);

  const subscribe = useCallback(
    <K extends EventKind>(kind: K, cb: Listener<DataFor<K>>): (() => void) => {
      const listeners = listenersRef.current;
      let set = listeners.get(kind);
      if (!set) {
        set = new Set();
        listeners.set(kind, set);
      }
      const captured = set;
      const listener = cb as Listener<unknown>;
      captured.add(listener);
      return () => {
        captured.delete(listener);
      };
    },
    [],
  );

  useEffect(() => {
    const listeners = listenersRef.current;
    const es = messagesEventSource();
    let connected = false;

    const emit = (kind: EventKind, data: unknown) => {
      // Snapshot the Set so one listener can unsubscribe without changing which
      // listeners receive the event currently being dispatched.
      for (const listener of Array.from(listeners.get(kind) ?? [])) {
        try {
          listener(data);
        } catch (error) {
          console.error(`SSE listener for ${kind} threw`, error);
        }
      }
    };
    const reportConnectionEvidence = () => {
      // EventSource reports an error before reconnecting. The first onopen or
      // server sentinel after that error starts a new logical connection; the
      // other signal is duplicate evidence for the same connection.
      if (connected) return;
      connected = true;
      emit("connected", undefined);
    };

    es.onmessage = (message) => {
      if (message.data === "connected") {
        reportConnectionEvidence();
        return;
      }

      let parsed: unknown;
      try {
        parsed = JSON.parse(message.data) as unknown;
      } catch (error) {
        console.warn("bad SSE payload", error, message.data);
        return;
      }
      if (
        typeof parsed !== "object" ||
        parsed === null ||
        typeof (parsed as { kind?: unknown }).kind !== "string" ||
        !("data" in parsed)
      ) {
        console.warn("bad SSE payload", parsed, message.data);
        return;
      }

      const event = parsed as SSEEvent;
      // Safe: `subscribe` only feeds each Set with callbacks whose data type
      // matches the key, so callbacks for this discriminant accept its data.
      emit(event.kind, event.data);
    };
    es.onopen = reportConnectionEvidence;
    es.onerror = () => {
      connected = false;
    };
    eventSourceRef.current = es;

    return () => {
      es.onmessage = null;
      es.onopen = null;
      es.onerror = null;
      es.close();
      if (eventSourceRef.current === es) eventSourceRef.current = null;
      listeners.clear();
    };
  }, []);

  const value: EventsContextValue = useMemo(
    () => ({
      onMessage: (cb) => subscribe("message", cb),
      onMessageUpdated: (cb) => subscribe("message_updated", cb),
      onMessageDeleted: (cb) => subscribe("message_deleted", cb),
      onMessageEmbedsUpdated: (cb) => subscribe("message_embeds_updated", cb),
      onMessageReactionsUpdated: (cb) => subscribe("message_reactions_updated", cb),
      onChannelCreated: (cb) => subscribe("channel_created", cb),
      onChannelsReordered: (cb) => subscribe("channels_reordered", cb),
      onEmojiCreated: (cb) => subscribe("emoji_created", cb),
      onEmojiUpdated: (cb) => subscribe("emoji_updated", cb),
      onEmojiDeleted: (cb) => subscribe("emoji_deleted", cb),
      onVoiceParticipantJoined: (cb) => subscribe("voice_participant_joined", cb),
      onVoiceParticipantLeft: (cb) => subscribe("voice_participant_left", cb),
      onVoiceParticipantSpeakingChanged: (cb) =>
        subscribe("voice_participant_speaking_changed", cb),
      onVoiceParticipantStatusChanged: (cb) => subscribe("voice_participant_status_changed", cb),
      onScreenShareStarted: (cb) => subscribe("screen_share_started", cb),
      onScreenShareStopped: (cb) => subscribe("screen_share_stopped", cb),
      onCameraVideoStarted: (cb) => subscribe("camera_video_started", cb),
      onCameraVideoStopped: (cb) => subscribe("camera_video_stopped", cb),
      onUserTyping: (cb) => subscribe("user_typing", cb),
      onThreadReplyCreated: (cb) => subscribe("thread_reply_created", cb),
      onThreadReplyDeleted: (cb) => subscribe("thread_reply_deleted", cb),
      onReadStateUpdated: (cb) => subscribe("read_state_updated", cb),
      onConnected: (cb) => subscribe("connected", cb),
    }),
    [subscribe],
  );

  return <EventsContext.Provider value={value}>{props.children}</EventsContext.Provider>;
}

export function useOptionalEvents() {
  return useContext(EventsContext);
}

export function useEvents() {
  const ctx = useOptionalEvents();
  if (!ctx) throw new Error("useEvents must be used inside EventsProvider");
  return ctx;
}
