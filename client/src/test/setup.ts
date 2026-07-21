import "@testing-library/jest-dom/vitest";
import "vitest-axe/extend-expect";
import { afterAll, afterEach, beforeAll, beforeEach, expect } from "vitest";
import { act, cleanup } from "@testing-library/react";
import * as axeMatchers from "vitest-axe/matchers";
import { clearCachedCsrfToken } from "../api/client";
import { resetMswState, server } from "./msw/server";
import { resetFakeEventSources } from "./msw/sse";

expect.extend(axeMatchers);

export type ConsoleDiagnosticLevel = "error" | "warn";

export interface ConsoleDiagnosticsCapture {
  readonly diagnostics: readonly (readonly unknown[])[];
  stop(): void;
}

export type ReactDiagnosticsCapture = ConsoleDiagnosticsCapture;

type Diagnostic = {
  readonly level: ConsoleDiagnosticLevel;
  readonly args: readonly unknown[];
};

const diagnosticObservers: Record<
  ConsoleDiagnosticLevel,
  Set<(args: readonly unknown[]) => void>
> = {
  error: new Set(),
  warn: new Set(),
};
let unexpectedDiagnostics: Diagnostic[] = [];

/**
 * Consume diagnostics that are an explicit part of a test scenario. Keep this
 * capture tightly scoped and assert on `diagnostics` before calling `stop()`.
 */
export function captureExpectedConsoleDiagnostics(
  level: ConsoleDiagnosticLevel,
): ConsoleDiagnosticsCapture {
  const diagnostics: (readonly unknown[])[] = [];
  const observer = (args: readonly unknown[]) => diagnostics.push(args);
  diagnosticObservers[level].add(observer);
  let active = true;
  return {
    diagnostics,
    stop: () => {
      if (!active) return;
      active = false;
      diagnosticObservers[level].delete(observer);
    },
  };
}

/** Capture expected React diagnostics, which React reports through console.error. */
export function captureReactDiagnostics(): ConsoleDiagnosticsCapture {
  return captureExpectedConsoleDiagnostics("error");
}

const originalConsoleError = console.error.bind(console);
const originalConsoleWarn = console.warn.bind(console);

function reportDiagnostic(level: ConsoleDiagnosticLevel, args: readonly unknown[]) {
  const observers = diagnosticObservers[level];
  if (observers.size > 0) {
    for (const observer of observers) observer(args);
    return;
  }

  unexpectedDiagnostics.push({ level, args });
  if (level === "error") originalConsoleError(...args);
  else originalConsoleWarn(...args);
}

console.error = (...args: unknown[]) => reportDiagnostic("error", args);
console.warn = (...args: unknown[]) => reportDiagnostic("warn", args);

function formatDiagnostic({ level, args }: Diagnostic) {
  const message = args
    .map((arg) => {
      if (arg instanceof Error) return arg.stack ?? arg.message;
      if (typeof arg === "string") return arg;
      try {
        return JSON.stringify(arg);
      } catch {
        return String(arg);
      }
    })
    .join(" ");
  return `console.${level}: ${message}`;
}

beforeAll(() => {
  server.listen({ onUnhandledRequest: "error" });
});

beforeEach(() => {
  assertNoUnexpectedDiagnostics();
  diagnosticObservers.error.clear();
  diagnosticObservers.warn.clear();
  localStorage.clear();
  clearCachedCsrfToken();
  resetMswState();
  resetFakeEventSources();
});

afterEach(async () => {
  await act(async () => {
    await Promise.resolve();
  });
  cleanup();
  server.resetHandlers();

  assertNoUnexpectedDiagnostics();
});

afterAll(() => {
  server.close();
  assertNoUnexpectedDiagnostics();
});

function assertNoUnexpectedDiagnostics() {
  if (unexpectedDiagnostics.length === 0) return;

  const pendingDiagnostics = unexpectedDiagnostics;
  unexpectedDiagnostics = [];
  const diagnostics = pendingDiagnostics.map(formatDiagnostic).join("\n\n");
  throw new Error(
    `Unexpected console diagnostics. Capture expected diagnostics explicitly:\n\n${diagnostics}`,
  );
}
