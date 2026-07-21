import { useCallback, useEffect, useMemo, useRef, useState } from "react";

export type ResourceStatus = "idle" | "loading" | "ready" | "error";

export interface ResourceState<T> {
  data: T | undefined;
  status: ResourceStatus;
  // `null` is the explicit no-error sentinel in the public resource contract.
  // oxlint-disable-next-line typescript/no-redundant-type-constituents
  error: unknown | null;
  loading: boolean;
  refreshing: boolean;
}

export interface ResourceControls<T> {
  refetch(): Promise<T | undefined>;
  update(value: T | undefined | ((current: T | undefined) => T | undefined)): void;
  invalidate(value?: T): void;
}

interface StoredResourceState<K, T> extends ResourceState<T> {
  key: K | null;
}

interface ResourceOptions<K, T> {
  key: K | null;
  load: (key: K, signal: AbortSignal) => Promise<T>;
  keepDataOnRefetch?: boolean;
}

function idleState<K, T>(): StoredResourceState<K, T> {
  return {
    key: null,
    data: undefined,
    status: "idle",
    error: null,
    loading: false,
    refreshing: false,
  };
}

function loadingState<K, T>(key: K): StoredResourceState<K, T> {
  return {
    key,
    data: undefined,
    status: "loading",
    error: null,
    loading: true,
    refreshing: false,
  };
}

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException
    ? error.name === "AbortError"
    : typeof error === "object" && error !== null && "name" in error && error.name === "AbortError";
}

export function useResource<K extends string | number, T>({
  key,
  load,
  keepDataOnRefetch = false,
}: ResourceOptions<K, T>): [ResourceState<T>, ResourceControls<T>] {
  const [stored, setStored] = useState<StoredResourceState<K, T>>(() =>
    key === null ? idleState() : loadingState(key),
  );
  const generationRef = useRef(0);
  const activeControllerRef = useRef<AbortController | null>(null);
  const mountedRef = useRef(false);
  const keyRef = useRef(key);
  const loadRef = useRef(load);
  const keepDataOnRefetchRef = useRef(keepDataOnRefetch);

  keyRef.current = key;
  loadRef.current = load;
  keepDataOnRefetchRef.current = keepDataOnRefetch;

  const cancelActive = useCallback(() => {
    ++generationRef.current;
    activeControllerRef.current?.abort();
    activeControllerRef.current = null;
  }, []);

  const startRequest = useCallback(async (requestKey: K): Promise<T | undefined> => {
    const generation = ++generationRef.current;
    activeControllerRef.current?.abort();
    const controller = new AbortController();
    activeControllerRef.current = controller;

    setStored((current) => {
      const retainData =
        Object.is(current.key, requestKey) &&
        keepDataOnRefetchRef.current &&
        current.data !== undefined;
      return retainData
        ? {
            ...current,
            status: "loading",
            error: null,
            loading: false,
            refreshing: true,
          }
        : loadingState(requestKey);
    });

    try {
      const next = await loadRef.current(requestKey, controller.signal);
      const isCurrent =
        mountedRef.current &&
        generation === generationRef.current &&
        Object.is(keyRef.current, requestKey);
      if (!isCurrent) return undefined;

      setStored({
        key: requestKey,
        data: next,
        status: "ready",
        error: null,
        loading: false,
        refreshing: false,
      });
      return next;
    } catch (error) {
      if (
        mountedRef.current &&
        generation === generationRef.current &&
        Object.is(keyRef.current, requestKey) &&
        !isAbortError(error)
      ) {
        setStored((current) => ({
          ...current,
          key: requestKey,
          status: "error",
          error,
          loading: false,
          refreshing: false,
        }));
      }
      return undefined;
    } finally {
      if (generation === generationRef.current) {
        activeControllerRef.current = null;
      }
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;

    if (key === null) {
      cancelActive();
      setStored((current) =>
        current.key === null && current.status === "idle" ? current : idleState(),
      );
    } else {
      void startRequest(key);
    }

    return () => {
      mountedRef.current = false;
      cancelActive();
    };
  }, [cancelActive, key, startRequest]);

  const refetch = useCallback((): Promise<T | undefined> => {
    const currentKey = keyRef.current;
    return currentKey === null ? Promise.resolve(undefined) : startRequest(currentKey);
  }, [startRequest]);

  const update = useCallback(
    (value: T | undefined | ((current: T | undefined) => T | undefined)): void => {
      setStored((current) => {
        const currentKey = keyRef.current;
        if (currentKey === null) return idleState();
        const currentData = Object.is(current.key, currentKey) ? current.data : undefined;
        const nextData =
          typeof value === "function"
            ? (value as (current: T | undefined) => T | undefined)(currentData)
            : value;
        return { ...current, key: currentKey, data: nextData };
      });
    },
    [],
  );

  const invalidate = useCallback(
    (value?: T): void => {
      cancelActive();
      const currentKey = keyRef.current;
      setStored(
        currentKey === null
          ? idleState()
          : {
              key: currentKey,
              data: value,
              status: "ready",
              error: null,
              loading: false,
              refreshing: false,
            },
      );
    },
    [cancelActive],
  );

  const state = useMemo<ResourceState<T>>(() => {
    if (key === null) {
      return idleState<K, T>();
    }
    if (!Object.is(stored.key, key)) {
      return loadingState(key);
    }
    const { data, status, error, loading, refreshing } = stored;
    return { data, status, error, loading, refreshing };
  }, [key, stored]);

  const controls = useMemo<ResourceControls<T>>(
    () => ({ refetch, update, invalidate }),
    [invalidate, refetch, update],
  );

  return [state, controls];
}
