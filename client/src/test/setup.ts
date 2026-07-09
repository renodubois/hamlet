import "@testing-library/jest-dom/vitest";
import "vitest-axe/extend-expect";
import { afterAll, afterEach, beforeAll, beforeEach, expect } from "vitest";
import { cleanup } from "./testing-library";
import * as axeMatchers from "vitest-axe/matchers";
import { clearCachedCsrfToken } from "../api/client";
import { resetMswState, server } from "./msw/server";
import { resetFakeEventSources } from "./msw/sse";

expect.extend(axeMatchers);

beforeAll(() => {
  server.listen({ onUnhandledRequest: "error" });
});

beforeEach(() => {
  localStorage.clear();
  clearCachedCsrfToken();
  resetMswState();
  resetFakeEventSources();
});

afterEach(() => {
  cleanup();
  server.resetHandlers();
});

afterAll(() => {
  server.close();
});
