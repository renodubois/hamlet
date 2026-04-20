import { createContext, onCleanup, onMount, useContext, type JSX } from "solid-js";
import {
  messagesEventSource,
  type Channel,
  type Message,
  type MessageDeleted,
  type MessageEmbedsUpdated,
  type SSEEvent,
  type UserTyping,
  type VoiceParticipant,
  type VoiceParticipantLeft,
  type VoiceParticipantSpeaking,
} from "./api";

type Listener<T> = (value: T) => void;

export interface EventsContextValue {
  onMessage: (cb: Listener<Message>) => () => void;
  onMessageUpdated: (cb: Listener<Message>) => () => void;
  onMessageDeleted: (cb: Listener<MessageDeleted>) => () => void;
  onMessageEmbedsUpdated: (cb: Listener<MessageEmbedsUpdated>) => () => void;
  onChannelCreated: (cb: Listener<Channel>) => () => void;
  onChannelsReordered: (cb: Listener<Channel[]>) => () => void;
  onVoiceParticipantJoined: (cb: Listener<VoiceParticipant>) => () => void;
  onVoiceParticipantLeft: (cb: Listener<VoiceParticipantLeft>) => () => void;
  onVoiceParticipantSpeakingChanged: (cb: Listener<VoiceParticipantSpeaking>) => () => void;
  onUserTyping: (cb: Listener<UserTyping>) => () => void;
}

const EventsContext = createContext<EventsContextValue>();

export function EventsProvider(props: { children: JSX.Element }) {
  const messageListeners = new Set<Listener<Message>>();
  const messageUpdatedListeners = new Set<Listener<Message>>();
  const messageDeletedListeners = new Set<Listener<MessageDeleted>>();
  const messageEmbedsUpdatedListeners = new Set<Listener<MessageEmbedsUpdated>>();
  const channelCreatedListeners = new Set<Listener<Channel>>();
  const channelsReorderedListeners = new Set<Listener<Channel[]>>();
  const voiceJoinedListeners = new Set<Listener<VoiceParticipant>>();
  const voiceLeftListeners = new Set<Listener<VoiceParticipantLeft>>();
  const voiceSpeakingListeners = new Set<Listener<VoiceParticipantSpeaking>>();
  const userTypingListeners = new Set<Listener<UserTyping>>();

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
      if (parsed.kind === "message") {
        messageListeners.forEach((cb) => cb(parsed.data));
      } else if (parsed.kind === "message_updated") {
        messageUpdatedListeners.forEach((cb) => cb(parsed.data));
      } else if (parsed.kind === "message_deleted") {
        messageDeletedListeners.forEach((cb) => cb(parsed.data));
      } else if (parsed.kind === "message_embeds_updated") {
        messageEmbedsUpdatedListeners.forEach((cb) => cb(parsed.data));
      } else if (parsed.kind === "channel_created") {
        channelCreatedListeners.forEach((cb) => cb(parsed.data));
      } else if (parsed.kind === "channels_reordered") {
        channelsReorderedListeners.forEach((cb) => cb(parsed.data));
      } else if (parsed.kind === "voice_participant_joined") {
        voiceJoinedListeners.forEach((cb) => cb(parsed.data));
      } else if (parsed.kind === "voice_participant_left") {
        voiceLeftListeners.forEach((cb) => cb(parsed.data));
      } else if (parsed.kind === "voice_participant_speaking_changed") {
        voiceSpeakingListeners.forEach((cb) => cb(parsed.data));
      } else if (parsed.kind === "user_typing") {
        userTypingListeners.forEach((cb) => cb(parsed.data));
      }
    };
    eventSource = es;
  });

  onCleanup(() => {
    eventSource?.close();
    eventSource = null;
    messageListeners.clear();
    messageUpdatedListeners.clear();
    messageDeletedListeners.clear();
    messageEmbedsUpdatedListeners.clear();
    channelCreatedListeners.clear();
    channelsReorderedListeners.clear();
    voiceJoinedListeners.clear();
    voiceLeftListeners.clear();
    voiceSpeakingListeners.clear();
    userTypingListeners.clear();
  });

  const value: EventsContextValue = {
    onMessage(cb) {
      messageListeners.add(cb);
      return () => messageListeners.delete(cb);
    },
    onMessageUpdated(cb) {
      messageUpdatedListeners.add(cb);
      return () => messageUpdatedListeners.delete(cb);
    },
    onMessageDeleted(cb) {
      messageDeletedListeners.add(cb);
      return () => messageDeletedListeners.delete(cb);
    },
    onMessageEmbedsUpdated(cb) {
      messageEmbedsUpdatedListeners.add(cb);
      return () => messageEmbedsUpdatedListeners.delete(cb);
    },
    onChannelCreated(cb) {
      channelCreatedListeners.add(cb);
      return () => channelCreatedListeners.delete(cb);
    },
    onChannelsReordered(cb) {
      channelsReorderedListeners.add(cb);
      return () => channelsReorderedListeners.delete(cb);
    },
    onVoiceParticipantJoined(cb) {
      voiceJoinedListeners.add(cb);
      return () => voiceJoinedListeners.delete(cb);
    },
    onVoiceParticipantLeft(cb) {
      voiceLeftListeners.add(cb);
      return () => voiceLeftListeners.delete(cb);
    },
    onVoiceParticipantSpeakingChanged(cb) {
      voiceSpeakingListeners.add(cb);
      return () => voiceSpeakingListeners.delete(cb);
    },
    onUserTyping(cb) {
      userTypingListeners.add(cb);
      return () => userTypingListeners.delete(cb);
    },
  };

  return <EventsContext.Provider value={value}>{props.children}</EventsContext.Provider>;
}

export function useEvents() {
  const ctx = useContext(EventsContext);
  if (!ctx) throw new Error("useEvents must be used inside EventsProvider");
  return ctx;
}
