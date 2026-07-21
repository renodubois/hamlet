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
import {
  deleteCustomEmoji,
  listCustomEmojis,
  renameCustomEmoji,
  restoreCustomEmoji,
  uploadCustomEmoji,
  type CustomEmoji,
} from "../api";
import type { ResourceStatus } from "../hooks/use-resource";
import { useAuth } from "./auth";
import { useOptionalEvents } from "./events";

export interface CustomEmojisContextValue {
  readonly allEmojis: readonly CustomEmoji[];
  readonly activeEmojis: readonly CustomEmoji[];
  byId: (id: number) => CustomEmoji | null;
  status: ResourceStatus;
  error: Error | null;
  refresh: () => Promise<void>;
  create: (name: string, file: Blob | File) => Promise<CustomEmoji>;
  rename: (id: number, name: string) => Promise<CustomEmoji>;
  remove: (id: number) => Promise<CustomEmoji>;
  restore: (id: number) => Promise<CustomEmoji>;
}

interface CustomEmojisState {
  key: number | null;
  emojis: readonly CustomEmoji[];
  status: ResourceStatus;
  error: Error | null;
}

interface JournalEntry {
  key: number;
  sequence: number;
  emoji: CustomEmoji;
}

const CustomEmojisContext = createContext<CustomEmojisContextValue | undefined>(undefined);

function toError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
}

function emojiVersion(emoji: CustomEmoji): number {
  // updated_at is the only version field present for every lifecycle state;
  // deleted_at disappears on restore and therefore cannot be a monotonic key.
  return emoji.updated_at;
}

function sameEmoji(left: CustomEmoji, right: CustomEmoji): boolean {
  return (
    left.id === right.id &&
    left.name === right.name &&
    left.image_url === right.image_url &&
    left.animated === right.animated &&
    left.created_by_user_id === right.created_by_user_id &&
    left.created_at === right.created_at &&
    left.updated_at === right.updated_at &&
    left.deleted_at === right.deleted_at
  );
}

function upsertEmoji(list: readonly CustomEmoji[], emoji: CustomEmoji): readonly CustomEmoji[] {
  const existing = list.findIndex((item) => item.id === emoji.id);
  if (existing < 0) return [emoji, ...list];

  const current = list[existing];
  if (emojiVersion(emoji) < emojiVersion(current) || sameEmoji(current, emoji)) return list;

  // The server's second-resolution timestamps cannot order distinct mutations in
  // the same second. For that tie, preserve transport delivery order (SSE itself
  // is ordered); local overlapping HTTP mutations are additionally guarded by
  // their provider operation sequence below.
  const next = [...list];
  next[existing] = emoji;
  return next;
}

function mergeSnapshot(
  snapshot: readonly CustomEmoji[],
  current: readonly CustomEmoji[],
): readonly CustomEmoji[] {
  return current.reduce<readonly CustomEmoji[]>(
    (merged, emoji) => upsertEmoji(merged, emoji),
    snapshot,
  );
}

function applyJournal(
  list: readonly CustomEmoji[],
  journal: readonly JournalEntry[],
): readonly CustomEmoji[] {
  return journal.reduce<readonly CustomEmoji[]>(
    (current, entry) => upsertEmoji(current, entry.emoji),
    list,
  );
}

export function CustomEmojisProvider(props: { children: ReactNode }) {
  const auth = useAuth();
  const events = useOptionalEvents();
  const authenticatedKey = auth.status === "authenticated" ? (auth.user?.id ?? null) : null;
  const keyRef = useRef(authenticatedKey);
  keyRef.current = authenticatedKey;
  const [state, setState] = useState<CustomEmojisState>({
    key: null,
    emojis: [],
    status: "idle",
    error: null,
  });
  const mountedRef = useRef(false);
  const requestGenerationRef = useRef(0);
  const requestControllerRef = useRef<AbortController | null>(null);
  const journalRef = useRef<JournalEntry[]>([]);
  const journalSequenceRef = useRef(0);
  const mutationSequenceRef = useRef(0);
  const latestMutationByIdRef = useRef(new Map<number, number>());

  const cancelRequest = useCallback(() => {
    ++requestGenerationRef.current;
    requestControllerRef.current?.abort();
    requestControllerRef.current = null;
  }, []);

  const recordAndApply = useCallback((emoji: CustomEmoji, expectedKey = keyRef.current) => {
    if (expectedKey === null || keyRef.current !== expectedKey) return;
    const entry: JournalEntry = {
      key: expectedKey,
      sequence: ++journalSequenceRef.current,
      emoji,
    };
    journalRef.current.push(entry);
    setState((current) => {
      if (current.key !== expectedKey) return current;
      const emojis = upsertEmoji(current.emojis, emoji);
      return emojis === current.emojis ? current : { ...current, emojis };
    });
  }, []);

  const refresh = useCallback(async (): Promise<void> => {
    const key = keyRef.current;
    if (key === null) return;

    const generation = ++requestGenerationRef.current;
    requestControllerRef.current?.abort();
    const controller = new AbortController();
    requestControllerRef.current = controller;
    const startedAfterSequence = journalSequenceRef.current;

    setState((current) => ({
      key,
      emojis: current.key === key ? current.emojis : [],
      status: "loading",
      error: null,
    }));

    try {
      const snapshot = await listCustomEmojis(controller.signal);
      if (
        !mountedRef.current ||
        controller.signal.aborted ||
        generation !== requestGenerationRef.current ||
        keyRef.current !== key
      ) {
        return;
      }
      const pendingJournal = journalRef.current.filter(
        (entry) => entry.key === key && entry.sequence > startedAfterSequence,
      );
      setState((current) => ({
        key,
        emojis: applyJournal(
          mergeSnapshot(snapshot, current.key === key ? current.emojis : []),
          pendingJournal,
        ),
        status: "ready",
        error: null,
      }));
      journalRef.current = [];
    } catch (error) {
      if (
        controller.signal.aborted ||
        !mountedRef.current ||
        generation !== requestGenerationRef.current ||
        keyRef.current !== key
      ) {
        return;
      }
      setState((current) => ({
        key,
        emojis: current.key === key ? current.emojis : [],
        status: "error",
        error: toError(error),
      }));
    } finally {
      if (generation === requestGenerationRef.current) requestControllerRef.current = null;
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    if (!events) {
      return () => {
        mountedRef.current = false;
        cancelRequest();
      };
    }

    const upsertFromEvent = (emoji: CustomEmoji) => recordAndApply(emoji);
    const unsubscribers = [
      events.onEmojiCreated(upsertFromEvent),
      events.onEmojiUpdated(upsertFromEvent),
      events.onEmojiDeleted(upsertFromEvent),
      events.onConnected(() => {
        // A new logical SSE connection is a freshness barrier. `refresh`
        // aborts and invalidates every snapshot begun before this connection.
        void refresh();
      }),
    ];
    return () => {
      mountedRef.current = false;
      unsubscribers.forEach((unsubscribe) => unsubscribe());
      cancelRequest();
    };
  }, [cancelRequest, events, recordAndApply, refresh]);

  useEffect(() => {
    cancelRequest();
    journalRef.current = [];
    latestMutationByIdRef.current.clear();

    if (authenticatedKey === null) {
      setState({ key: null, emojis: [], status: "idle", error: null });
      return;
    }

    setState({ key: authenticatedKey, emojis: [], status: "loading", error: null });
    void refresh();
  }, [authenticatedKey, cancelRequest, refresh]);

  const create = useCallback(
    async (name: string, file: Blob | File): Promise<CustomEmoji> => {
      const key = keyRef.current;
      const emoji = await uploadCustomEmoji(name, file);
      recordAndApply(emoji, key);
      return emoji;
    },
    [recordAndApply],
  );

  const applyMutation = useCallback(
    async (id: number, request: () => Promise<CustomEmoji>): Promise<CustomEmoji> => {
      const key = keyRef.current;
      const sequence = ++mutationSequenceRef.current;
      latestMutationByIdRef.current.set(id, sequence);
      const emoji = await request();
      if (latestMutationByIdRef.current.get(id) === sequence) recordAndApply(emoji, key);
      return emoji;
    },
    [recordAndApply],
  );

  const rename = useCallback(
    (id: number, name: string): Promise<CustomEmoji> =>
      applyMutation(id, () => renameCustomEmoji(id, name)),
    [applyMutation],
  );

  const remove = useCallback(
    (id: number): Promise<CustomEmoji> => applyMutation(id, () => deleteCustomEmoji(id)),
    [applyMutation],
  );

  const restore = useCallback(
    (id: number): Promise<CustomEmoji> => applyMutation(id, () => restoreCustomEmoji(id)),
    [applyMutation],
  );

  const visibleState =
    state.key === authenticatedKey
      ? state
      : {
          key: authenticatedKey,
          emojis: [] as readonly CustomEmoji[],
          status: authenticatedKey === null ? ("idle" as const) : ("loading" as const),
          error: null,
        };
  const allEmojis = visibleState.emojis;
  const activeEmojis = useMemo(
    () => allEmojis.filter((emoji) => emoji.deleted_at === null),
    [allEmojis],
  );
  const byIdMap = useMemo(
    () => new Map<number, CustomEmoji>(allEmojis.map((emoji) => [emoji.id, emoji])),
    [allEmojis],
  );
  const byIdMapRef = useRef(byIdMap);
  byIdMapRef.current = byIdMap;
  const byId = useCallback((id: number) => byIdMapRef.current.get(id) ?? null, []);

  const value = useMemo<CustomEmojisContextValue>(
    () => ({
      allEmojis,
      activeEmojis,
      byId,
      status: visibleState.status,
      error: visibleState.error,
      refresh,
      create,
      rename,
      remove,
      restore,
    }),
    [
      activeEmojis,
      allEmojis,
      byId,
      create,
      refresh,
      remove,
      rename,
      restore,
      visibleState.error,
      visibleState.status,
    ],
  );

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
