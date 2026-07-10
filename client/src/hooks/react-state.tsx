import React, {
  createContext,
  lazy,
  useContext,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createPortal, flushSync } from "react-dom";

export { createContext, lazy, useContext };
export namespace JSX {
  export type Element = React.ReactNode;
  export type CSSProperties = React.CSSProperties;
  export type HTMLAttributes<T> = React.HTMLAttributes<T>;
  export type EventHandler<T, E extends Event> = {
    bivarianceHack(
      event: React.SyntheticEvent<T> &
        Partial<Pick<KeyboardEvent, "key" | "altKey" | "ctrlKey" | "metaKey" | "shiftKey">> & {
          currentTarget: T;
          isComposing?: boolean;
          __event?: E;
        },
    ): void;
  }["bivarianceHack"];
}
export type Component<P = Record<string, never>> = (props: P) => ReactNode;
export type Getter<T> = () => T;
export type ValueUpdater<T> = (value: T | ((current: T) => T)) => void;

const staticSignalListeners = new Set<() => void>();

type ReactClientInternals = {
  __CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE?: { H: unknown };
};

function canUseReactHooks(): boolean {
  return Boolean(
    (React as unknown as ReactClientInternals)
      .__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE?.H,
  );
}

function notifyStaticSignalListeners(): void {
  flushSync(() => {
    staticSignalListeners.forEach((listener) => listener());
  });
}

export function useStaticSignalRerender(): void {
  const [, setVersion] = useState(0);
  useEffect(() => {
    const listener = () => setVersion((version) => version + 1);
    staticSignalListeners.add(listener);
    return () => {
      staticSignalListeners.delete(listener);
    };
  }, []);
}

function shallowEqual(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true;
  if (Array.isArray(a) && Array.isArray(b)) {
    return a.length === b.length && a.every((item, index) => Object.is(item, b[index]));
  }
  if (
    a &&
    b &&
    typeof a === "object" &&
    typeof b === "object" &&
    Object.getPrototypeOf(a) === Object.prototype &&
    Object.getPrototypeOf(b) === Object.prototype
  ) {
    const aEntries = Object.entries(a);
    const bObject = b as Record<string, unknown>;
    return (
      aEntries.length === Object.keys(bObject).length &&
      aEntries.every(([key, value]) => Object.is(value, bObject[key]))
    );
  }
  return false;
}

function useStaticSignalState<T>(initial: T): [Getter<T>, ValueUpdater<T>] {
  let value = initial;
  const setStaticValue: ValueUpdater<T> = (nextValue) => {
    const next =
      typeof nextValue === "function" ? (nextValue as (current: T) => T)(value) : nextValue;
    const resolved = shallowEqual(value, next) ? value : next;
    if (Object.is(resolved, value)) return;
    value = resolved;
    notifyStaticSignalListeners();
  };
  return [() => value, setStaticValue];
}

const shouldSuppressInvalidHookWarnings = import.meta.env.MODE === "test";

function suppressInvalidHookWarning<T>(fn: () => T): T {
  if (!shouldSuppressInvalidHookWarnings) return fn();

  const previousConsoleError = console.error;
  console.error = (...args: unknown[]) => {
    if (typeof args[0] === "string" && args[0].includes("Invalid hook call")) return;
    previousConsoleError(...args);
  };
  try {
    return fn();
  } finally {
    console.error = previousConsoleError;
  }
}

export function useSignalState<T>(initial: T): [Getter<T>, ValueUpdater<T>] {
  if (!canUseReactHooks()) return useStaticSignalState(initial);

  let state: [T, React.Dispatch<React.SetStateAction<T>>];
  let valueRef: React.RefObject<T>;
  try {
    state = suppressInvalidHookWarning(() => useState(initial));
    valueRef = suppressInvalidHookWarning(() => useRef(state[0])) as React.RefObject<T>;
  } catch {
    return useStaticSignalState(initial);
  }
  const [value, setValue] = state;
  valueRef.current = value;
  const setComparableValue: ValueUpdater<T> = (nextValue) => {
    const next =
      typeof nextValue === "function"
        ? (nextValue as (current: T) => T)(valueRef.current)
        : nextValue;
    const resolved = shallowEqual(valueRef.current, next) ? valueRef.current : next;
    if (Object.is(resolved, valueRef.current)) return;
    valueRef.current = resolved;
    setValue(resolved);
  };
  return [() => valueRef.current, setComparableValue];
}

export function useComputedValue<T>(fn: () => T): Getter<T> {
  const valueRef = useRef<T>(undefined as T);
  valueRef.current = fn();
  return () => valueRef.current;
}

type Cleanup = () => void;
const cleanupStack: Cleanup[][] = [];

export function registerCleanup(cleanup: Cleanup): void {
  const current = cleanupStack[cleanupStack.length - 1];
  if (current) {
    current.push(cleanup);
    return;
  }
  const cleanupRef = useRef(cleanup);
  cleanupRef.current = cleanup;
  useEffect(() => () => cleanupRef.current(), []);
}

function runWithCleanup(fn: () => void | Cleanup): Cleanup | undefined {
  const cleanups: Cleanup[] = [];
  cleanupStack.push(cleanups);
  const returned = fn();
  cleanupStack.pop();
  return () => {
    if (typeof returned === "function") returned();
    for (const cleanup of cleanups.reverse()) cleanup();
  };
}

export function useAfterRenderEffect(fn: () => void | Cleanup): void {
  useEffect(() => runWithCleanup(fn));
}

export function useMountEffect(fn: () => void | Cleanup): void {
  const fnRef = useRef(fn);
  fnRef.current = fn;
  useLayoutEffect(() => runWithCleanup(() => fnRef.current()), []);
}

export function useStableDomId(): string {
  return useId().replace(/:/g, "");
}

export function ignoreReactiveTracking<T>(fn: () => T): T {
  return fn();
}

export function PortalRoot(props: { children?: ReactNode; mount?: globalThis.Element }) {
  return createPortal(props.children, props.mount ?? document.body);
}

export type CallableResource<T> = Getter<T | undefined> & {
  loading: boolean;
  error: unknown;
  latest?: T;
};

function useStaticCallableResource<S, T>(
  sourceOrFetcher: (() => S) | (() => Promise<T>),
  maybeFetcher?: (source: S) => Promise<T>,
) {
  let data: T | undefined;
  let loading = true;
  let error: unknown;
  const resource = (() => data) as CallableResource<T>;
  Object.defineProperties(resource, {
    loading: { get: () => loading },
    error: { get: () => error },
    latest: { get: () => data },
  });
  const refetch = async () => {
    loading = true;
    try {
      const sourceValue = maybeFetcher ? (sourceOrFetcher as () => S)() : undefined;
      data = maybeFetcher
        ? await maybeFetcher(sourceValue as S)
        : await (sourceOrFetcher as () => Promise<T>)();
      error = undefined;
      return data;
    } catch (err) {
      error = err;
      return undefined;
    } finally {
      loading = false;
    }
  };
  void refetch();
  return [
    resource,
    {
      refetch,
      mutate: (value: T | ((current: T | undefined) => T)) => {
        data = typeof value === "function" ? (value as (current: T | undefined) => T)(data) : value;
      },
    },
  ] as const;
}

export function useCallableResource<S, T>(
  sourceOrFetcher: (() => S) | (() => Promise<T>),
  maybeFetcher?: (source: S) => Promise<T>,
): any {
  if (!canUseReactHooks()) return useStaticCallableResource(sourceOrFetcher, maybeFetcher) as any;

  let dataState: [T | undefined, React.Dispatch<React.SetStateAction<T | undefined>>];
  try {
    dataState = suppressInvalidHookWarning(() => useState<T | undefined>(undefined));
  } catch {
    return useStaticCallableResource(sourceOrFetcher, maybeFetcher) as any;
  }
  const [data, setData] = dataState;
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(undefined);
  const lastSource = useRef<S | symbol>(Symbol("unset"));
  const loadedWithoutSource = useRef(false);
  const runId = useRef(0);

  const load = async (sourceValue?: S): Promise<T | undefined> => {
    const id = ++runId.current;
    setLoading(true);
    setError(undefined);
    try {
      const next = maybeFetcher
        ? await maybeFetcher(sourceValue as S)
        : await (sourceOrFetcher as () => Promise<T>)();
      if (id === runId.current) setData(next);
      return next;
    } catch (err) {
      if (id === runId.current) setError(err);
      return undefined;
    } finally {
      if (id === runId.current) setLoading(false);
    }
  };

  useEffect(() => {
    const sourceValue = maybeFetcher ? (sourceOrFetcher as () => S)() : undefined;
    if (maybeFetcher && Object.is(lastSource.current, sourceValue)) return;
    if (!maybeFetcher && loadedWithoutSource.current) return;
    if (maybeFetcher) lastSource.current = sourceValue as S;
    else loadedWithoutSource.current = true;
    void load(sourceValue as S);
  });

  const resource = (() => data) as CallableResource<T>;
  resource.loading = loading;
  resource.error = error;
  resource.latest = data;

  return [
    resource,
    {
      refetch: () => load(maybeFetcher ? (sourceOrFetcher as () => S)() : undefined),
      mutate: (value: T | ((current: T | undefined) => T | undefined)) => {
        setData((current) =>
          typeof value === "function"
            ? (value as (current: T | undefined) => T | undefined)(current)
            : value,
        );
      },
    },
  ];
}

export function If<T>(props: {
  when: T | false | null | undefined;
  fallback?: ReactNode;
  keyed?: boolean;
  children?: any;
}) {
  if (!props.when) return <>{props.fallback ?? null}</>;
  if (typeof props.children === "function") {
    const value = () => props.when as NonNullable<T>;
    return (
      <>
        {props.keyed
          ? (props.children as (value: NonNullable<T>) => ReactNode)(props.when as NonNullable<T>)
          : (props.children as (value: Getter<NonNullable<T>>) => ReactNode)(value)}
      </>
    );
  }
  return <>{props.children}</>;
}

export function List(props: {
  each: readonly any[] | undefined | null;
  fallback?: ReactNode;
  children: (item: any, index: Getter<number>) => any;
}) {
  const list = props.each ?? [];
  if (list.length === 0) return <>{props.fallback ?? null}</>;
  return (
    <>
      {list.map((item, index) => (
        <React.Fragment key={(item as { id?: unknown }).id?.toString() ?? index}>
          {props.children(item, () => index)}
        </React.Fragment>
      ))}
    </>
  );
}

export function Case(props: { when: unknown; children?: any }) {
  if (!props.when) return null;
  if (typeof props.children === "function") {
    return <>{props.children(() => props.when)}</>;
  }
  return <>{props.children}</>;
}

export function Choose(props: { fallback?: ReactNode; children?: ReactNode }) {
  const childrenArray = React.Children.toArray(props.children) as React.ReactElement<{
    when?: unknown;
  }>[];
  for (const child of childrenArray) {
    if (React.isValidElement(child) && child.props.when) return child;
  }
  return <>{props.fallback ?? null}</>;
}

type StoreUpdater<T> = (
  first: T | ((current: T) => T) | ((item: any) => boolean) | number,
  second?: any,
) => void;

export function useStoreState<T>(initial: T): [T, StoreUpdater<T>] {
  const [state, setState] = useState(initial);
  const updater = function (first: unknown, second?: unknown) {
    if (arguments.length === 1) {
      setState((current) => {
        const next =
          typeof first === "function" ? (first as (current: T) => T)(current) : (first as T);
        return shallowEqual(current, next) ? current : next;
      });
      return;
    }
    setState((current) => {
      if (Array.isArray(current) && typeof first === "number") {
        const next = [...current];
        next[first] = second;
        return next as T;
      }
      if (Array.isArray(current) && typeof first === "function") {
        const next = current.map((item) => {
          if (!(first as (item: unknown) => boolean)(item)) return item;
          const patch =
            typeof second === "function" ? (second as (item: unknown) => unknown)(item) : second;
          const nextItem =
            typeof patch === "object" && patch !== null
              ? { ...(item as object), ...(patch as object) }
              : patch;
          return shallowEqual(item, nextItem) ? item : nextItem;
        });
        return shallowEqual(current, next) ? current : (next as T);
      }
      return current;
    });
  } as StoreUpdater<T>;
  return [state, updater];
}

export function preserveIdentity<T>(value: T, _options?: unknown): T {
  return value;
}
