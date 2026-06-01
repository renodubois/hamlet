import {
  createContext,
  createMemo,
  createResource,
  createSignal,
  onCleanup,
  useContext,
  type Accessor,
  type JSX,
  type Resource,
} from "solid-js";
import {
  deleteCustomEmoji,
  listCustomEmojis,
  renameCustomEmoji,
  restoreCustomEmoji,
  uploadCustomEmoji,
  type CustomEmoji,
} from "../api";
import { useAuth } from "./auth";
import { useOptionalEvents } from "./events";

export interface CustomEmojisContextValue {
  allEmojis: Resource<CustomEmoji[]>;
  activeEmojis: Accessor<CustomEmoji[]>;
  byId: (id: number) => CustomEmoji | null;
  error: Accessor<Error | null>;
  refresh: () => void;
  create: (name: string, file: Blob | File) => Promise<CustomEmoji>;
  rename: (id: number, name: string) => Promise<CustomEmoji>;
  remove: (id: number) => Promise<CustomEmoji>;
  restore: (id: number) => Promise<CustomEmoji>;
}

const CustomEmojisContext = createContext<CustomEmojisContextValue>();

function upsertEmoji(list: CustomEmoji[] | undefined, emoji: CustomEmoji): CustomEmoji[] {
  const current = list ?? [];
  const existing = current.findIndex((item) => item.id === emoji.id);
  if (existing >= 0) {
    return current.map((item) => (item.id === emoji.id ? emoji : item));
  }
  return [emoji, ...current];
}

export function CustomEmojisProvider(props: { children: JSX.Element }) {
  const auth = useAuth();
  const events = useOptionalEvents();
  const [error, setError] = createSignal<Error | null>(null);
  const [allEmojis, { refetch, mutate }] = createResource(
    () => auth.user()?.id ?? null,
    async () => {
      setError(null);
      try {
        return await listCustomEmojis();
      } catch (err) {
        setError(err instanceof Error ? err : new Error(String(err)));
        return [];
      }
    },
  );

  const activeEmojis = createMemo(() => (allEmojis() ?? []).filter((e) => e.deleted_at === null));
  const byIdMap = createMemo(() => new Map((allEmojis() ?? []).map((e) => [e.id, e])));

  if (events) {
    const eventUnsubscribers = [
      events.onEmojiCreated((emoji) => {
        mutate((current) => upsertEmoji(current, emoji));
      }),
      events.onEmojiUpdated((emoji) => {
        mutate((current) => upsertEmoji(current, emoji));
      }),
      events.onEmojiDeleted((emoji) => {
        mutate((current) => upsertEmoji(current, emoji));
      }),
    ];
    onCleanup(() => {
      eventUnsubscribers.forEach((unsubscribe) => unsubscribe());
    });
  }

  const create = async (name: string, file: Blob | File) => {
    const emoji = await uploadCustomEmoji(name, file);
    mutate((current) => upsertEmoji(current, emoji));
    return emoji;
  };

  const rename = async (id: number, name: string) => {
    const emoji = await renameCustomEmoji(id, name);
    mutate((current) => upsertEmoji(current, emoji));
    return emoji;
  };

  const remove = async (id: number) => {
    const emoji = await deleteCustomEmoji(id);
    mutate((current) => upsertEmoji(current, emoji));
    return emoji;
  };

  const restore = async (id: number) => {
    const emoji = await restoreCustomEmoji(id);
    mutate((current) => upsertEmoji(current, emoji));
    return emoji;
  };

  const value: CustomEmojisContextValue = {
    allEmojis,
    activeEmojis,
    byId: (id) => byIdMap().get(id) ?? null,
    error,
    refresh: () => void refetch(),
    create,
    rename,
    remove,
    restore,
  };

  return (
    <CustomEmojisContext.Provider value={value}>{props.children}</CustomEmojisContext.Provider>
  );
}

export function useOptionalCustomEmojis() {
  return useContext(CustomEmojisContext);
}

export function useCustomEmojis() {
  const ctx = useOptionalCustomEmojis();
  if (!ctx) throw new Error("useCustomEmojis must be used inside CustomEmojisProvider");
  return ctx;
}
