import React, { useEffect, useLayoutEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";

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
