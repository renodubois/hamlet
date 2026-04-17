import {
  createContext,
  createResource,
  onCleanup,
  onMount,
  useContext,
  type JSX,
  type Resource,
} from "solid-js";
import { listChannels, type Channel } from "./api";
import { useAuth } from "./auth_context";
import { useEvents } from "./events_context";

interface ChannelsContextValue {
  channels: Resource<Channel[]>;
  refetch: () => void;
}

const ChannelsContext = createContext<ChannelsContextValue>();

export function ChannelsProvider(props: { children: JSX.Element }) {
  const auth = useAuth();
  const events = useEvents();
  const [channels, { refetch }] = createResource(
    () => auth.user() || null,
    () => listChannels(),
  );

  onMount(() => {
    const unsub = events.onChannelCreated(() => {
      void refetch();
    });
    onCleanup(unsub);
  });

  return (
    <ChannelsContext.Provider value={{ channels, refetch: () => void refetch() }}>
      {props.children}
    </ChannelsContext.Provider>
  );
}

export function useChannels() {
  const ctx = useContext(ChannelsContext);
  if (!ctx) throw new Error("useChannels must be used inside ChannelsProvider");
  return ctx;
}
