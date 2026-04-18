import { createContext, onCleanup, onMount, useContext, type JSX } from "solid-js";
import { messagesEventSource, type Channel, type Message, type SSEEvent } from "./api";

type Listener<T> = (value: T) => void;

interface EventsContextValue {
  onMessage: (cb: Listener<Message>) => () => void;
  onChannelCreated: (cb: Listener<Channel>) => () => void;
  onChannelsReordered: (cb: Listener<Channel[]>) => () => void;
}

const EventsContext = createContext<EventsContextValue>();

export function EventsProvider(props: { children: JSX.Element }) {
  const messageListeners = new Set<Listener<Message>>();
  const channelCreatedListeners = new Set<Listener<Channel>>();
  const channelsReorderedListeners = new Set<Listener<Channel[]>>();

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
      } else if (parsed.kind === "channel_created") {
        channelCreatedListeners.forEach((cb) => cb(parsed.data));
      } else if (parsed.kind === "channels_reordered") {
        channelsReorderedListeners.forEach((cb) => cb(parsed.data));
      }
    };
    eventSource = es;
  });

  onCleanup(() => {
    eventSource?.close();
    eventSource = null;
    messageListeners.clear();
    channelCreatedListeners.clear();
    channelsReorderedListeners.clear();
  });

  const value: EventsContextValue = {
    onMessage(cb) {
      messageListeners.add(cb);
      return () => messageListeners.delete(cb);
    },
    onChannelCreated(cb) {
      channelCreatedListeners.add(cb);
      return () => channelCreatedListeners.delete(cb);
    },
    onChannelsReordered(cb) {
      channelsReorderedListeners.add(cb);
      return () => channelsReorderedListeners.delete(cb);
    },
  };

  return <EventsContext.Provider value={value}>{props.children}</EventsContext.Provider>;
}

export function useEvents() {
  const ctx = useContext(EventsContext);
  if (!ctx) throw new Error("useEvents must be used inside EventsProvider");
  return ctx;
}
