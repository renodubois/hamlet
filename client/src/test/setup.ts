import "@testing-library/jest-dom/vitest";
import "vitest-axe/extend-expect";
import { afterAll, afterEach, beforeAll, beforeEach, expect } from "vitest";
import { act, cleanup } from "./testing-library";
import * as axeMatchers from "vitest-axe/matchers";
import { clearCachedCsrfToken } from "../api/client";
import { resetMswState, server } from "./msw/server";
import { resetFakeEventSources } from "./msw/sse";

expect.extend(axeMatchers);

// React 19 emits noisy act diagnostics for async provider/SSE updates that these
// integration tests observe with Testing Library's async assertions. Keep stderr
// focused on actionable application warnings and errors.
const originalConsoleError = console.error.bind(console);
console.error = (...args: unknown[]) => {
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
