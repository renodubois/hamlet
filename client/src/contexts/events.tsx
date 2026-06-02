import { createContext, onCleanup, onMount, useContext, type JSX } from "solid-js";
import {
  messagesEventSource,
  type Channel,
  type CustomEmoji,
  type Message,
  type MessageDeleted,
  type MessageEmbedsUpdated,
  type MessageReactionsUpdated,
  type SSEEvent,
  type ThreadReplyCreated,
  type ThreadReplyDeleted,
  type UserTyping,
  type VoiceParticipant,
  type VoiceParticipantLeft,
  type VoiceParticipantSpeaking,
} from "../api";

type Listener<T> = (value: T) => void;

// Look up the data type for a given SSEEvent kind.
type DataFor<K extends SSEEvent["kind"]> = Extract<SSEEvent, { kind: K }>["data"];

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
  onUserTyping: (cb: Listener<UserTyping>) => () => void;
  onThreadReplyCreated: (cb: Listener<ThreadReplyCreated>) => () => void;
  onThreadReplyDeleted: (cb: Listener<ThreadReplyDeleted>) => () => void;
}

const EventsContext = createContext<EventsContextValue>();

export function EventsProvider(props: { children: JSX.Element }) {
  // One Set per SSEEvent kind. Callbacks are stored as type-erased functions;
  // the typed `subscribe` wrapper below preserves the kind→data correspondence
  // from the SSEEvent discriminated union at the boundary.
  const listeners = new Map<SSEEvent["kind"], Set<Listener<unknown>>>();

  function subscribe<K extends SSEEvent["kind"]>(kind: K, cb: Listener<DataFor<K>>): () => void {
    let set = listeners.get(kind);
    if (!set) {
      set = new Set();
      listeners.set(kind, set);
    }
    const captured = set;
    captured.add(cb as Listener<unknown>);
    return () => captured.delete(cb as Listener<unknown>);
  }

  let eventSource: EventSource | null = null;

  onMount(() => {
    const es = messagesEventSource();
    es.onmessage = (m) => {
      if (m.data === "connected") return;
      let parsed: SSEEvent;
      try {
        parsed = JSON.parse(m.data) as SSEEvent;
      } catch (e) {
        console.warn("bad SSE payload", e, m.data);
        return;
      }
      const set = listeners.get(parsed.kind);
      if (!set) return;
      // Safe: `subscribe` only feeds each Set with callbacks whose data type
      // matches the key, so for any given Set every callback can accept
      // `parsed.data` when `parsed.kind === key`.
      set.forEach((cb) => cb(parsed.data));
    };
    eventSource = es;
  });

  onCleanup(() => {
    eventSource?.close();
    eventSource = null;
    listeners.clear();
  });

  const value: EventsContextValue = {
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
    onVoiceParticipantSpeakingChanged: (cb) => subscribe("voice_participant_speaking_changed", cb),
    onUserTyping: (cb) => subscribe("user_typing", cb),
    onThreadReplyCreated: (cb) => subscribe("thread_reply_created", cb),
    onThreadReplyDeleted: (cb) => subscribe("thread_reply_deleted", cb),
  };

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
