import { createContext, useContext, useEffect, type ReactNode } from "react";
import { useCallableResource, type CallableResource } from "../hooks/react-state";
import { listChannels, reorderChannels, type Channel } from "../api";
import { useAuth } from "./auth";
import { useEvents } from "./events";

interface ChannelsContextValue {
  channels: CallableResource<Channel[]>;
  refetch: () => void;
  reorder: (ids: number[]) => Promise<void>;
}

const ChannelsContext = createContext<ChannelsContextValue | undefined>(undefined);

function sortByPosition(list: Channel[]): Channel[] {
  return [...list].sort((a, b) => a.position - b.position || a.id - b.id);
}

export function ChannelsProvider(props: { children: ReactNode }) {
  const auth = useAuth();
  const events = useEvents();
  const [channels, { refetch, mutate }] = useCallableResource(
    () => auth.user()?.id ?? null,
    async () => sortByPosition(await listChannels()),
  );

  useEffect(() => {
    const unsubCreated = events.onChannelCreated(() => {
      void refetch();
    });
    const unsubReordered = events.onChannelsReordered((list) => {
      mutate(sortByPosition(list));
    });
    return () => {
      unsubCreated();
      unsubReordered();
    };
  }, [events, mutate, refetch]);

  async function reorder(ids: number[]): Promise<void> {
    const previous = channels();
    if (previous) {
      const byId = new Map(previous.map((c) => [c.id, c]));
      const optimistic = ids
        .map((id, i) => {
          const c = byId.get(id);
          return c ? { ...c, position: i } : null;
        })
        .filter((c): c is Channel => c !== null);
      if (optimistic.length === ids.length) {
        mutate(optimistic);
      }
    }
    try {
      const updated = await reorderChannels(ids);
      mutate(sortByPosition(updated));
    } catch (err) {
      // Roll back on failure by re-fetching from the server.
      void refetch();
      throw err;
    }
  }

  return (
    <ChannelsContext.Provider value={{ channels, refetch: () => void refetch(), reorder }}>
      {props.children}
    </ChannelsContext.Provider>
  );
}

export function useOptionalChannels() {
  return useContext(ChannelsContext);
}

export function useChannels() {
  const ctx = useOptionalChannels();
  if (!ctx) throw new Error("useChannels must be used inside ChannelsProvider");
  return ctx;
}
