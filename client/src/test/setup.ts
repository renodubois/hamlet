import "@testing-library/jest-dom/vitest";
import "vitest-axe/extend-expect";
import { afterAll, afterEach, beforeAll, beforeEach, expect } from "vitest";
import { act, cleanup } from "./testing-library";
import * as axeMatchers from "vitest-axe/matchers";
import { clearCachedCsrfToken } from "../api/client";
import { resetMswState, server } from "./msw/server";
import { resetFakeEventSources } from "./msw/sse";

expect.extend(axeMatchers);

export interface ReactDiagnosticsCapture {
  readonly diagnostics: readonly (readonly unknown[])[];
  stop(): void;
}

const diagnosticObservers = new Set<(args: readonly unknown[]) => void>();

/**
 * Observe console diagnostics before the temporary legacy act-warning filter.
 * Focused native-React tests use this to keep warnings visible during migration.
 */
export function captureReactDiagnostics(): ReactDiagnosticsCapture {
  const diagnostics: (readonly unknown[])[] = [];
  const observer = (args: readonly unknown[]) => diagnostics.push(args);
  diagnosticObservers.add(observer);
  return {
    diagnostics,
    stop: () => diagnosticObservers.delete(observer),
  };
}

// Phase 7 removes this broad suppression after the legacy static-signal tests
// migrate. Captures above still observe every diagnostic in focused tests.
const originalConsoleError = console.error.bind(console);
console.error = (...args: unknown[]) => {
  for (const observer of diagnosticObservers) observer(args);
  if (typeof args[0] === "string" && args[0].includes("not wrapped in act")) return;
  originalConsoleError(...args);
};

beforeAll(() => {
  server.listen({ onUnhandledRequest: "error" });
});

beforeEach(() => {
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
});

afterAll(() => {
  server.close();
});
