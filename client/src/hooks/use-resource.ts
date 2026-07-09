import { useCallback, useEffect, useRef, useState } from "react";

export interface ResourceState<T> {
  data: T | undefined;
  latest: T | undefined;
  loading: boolean;
  error: unknown;
}

export interface ResourceControls<T> {
  refetch: () => Promise<T | undefined>;
  mutate: (value: T | ((current: T | undefined) => T)) => void;
}

export function useResource<T>(fetcher: () => Promise<T>): [ResourceState<T>, ResourceControls<T>];
export function useResource<S, T>(
  source: () => S,
  fetcher: (source: S) => Promise<T>,
): [ResourceState<T>, ResourceControls<T>];
export function useResource<S, T>(
  sourceOrFetcher: (() => S) | (() => Promise<T>),
  maybeFetcher?: (source: S) => Promise<T>,
): [ResourceState<T>, ResourceControls<T>] {
  const [data, setData] = useState<T | undefined>(undefined);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(undefined);
  const [reloadTick, setReloadTick] = useState(0);
  const runId = useRef(0);
  const sourceOrFetcherRef = useRef(sourceOrFetcher);
  const maybeFetcherRef = useRef(maybeFetcher);
  sourceOrFetcherRef.current = sourceOrFetcher;
  maybeFetcherRef.current = maybeFetcher;
  const sourceValue = maybeFetcher ? (sourceOrFetcher as () => S)() : undefined;

  const load = useCallback(async (source?: S): Promise<T | undefined> => {
    const id = ++runId.current;
    setLoading(true);
    setError(undefined);
    try {
      const fetcher = maybeFetcherRef.current;
      const next = fetcher
        ? await fetcher(source as S)
        : await (sourceOrFetcherRef.current as () => Promise<T>)();
      if (id === runId.current) setData(next);
      return next;
    } catch (err) {
      if (id === runId.current) setError(err);
      return undefined;
    } finally {
      if (id === runId.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load(sourceValue as S);
  }, [load, sourceValue, reloadTick]);

  const refetch = useCallback(async (): Promise<T | undefined> => {
    const nextSource = maybeFetcherRef.current
      ? (sourceOrFetcherRef.current as () => S)()
      : undefined;
    const result = await load(nextSource as S);
    setReloadTick((current) => current + 1);
    return result;
  }, [load]);

  const mutate = useCallback((value: T | ((current: T | undefined) => T)) => {
    setData((current) =>
      typeof value === "function" ? (value as (current: T | undefined) => T)(current) : value,
    );
  }, []);

  return [
    { data, latest: data, loading, error },
    { refetch, mutate },
  ];
}
