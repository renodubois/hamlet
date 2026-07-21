import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { getMe, logout } from "./auth";
import { listChannels } from "./channels";
import { clearCachedCsrfToken } from "./client";
import { getPublicServerConfig } from "./config";
import { listCustomEmojis } from "./emojis";
import { getThread, listMessages, listParticipatedThreads } from "./messages";
import { listReadStates } from "./read-states";
import { searchUsers } from "./users";
import { listCameraStreams, listScreenShareStreams, listVoiceParticipants } from "./voice";

const fetchMock = vi.fn();

type ReadCall = (signal?: AbortSignal) => Promise<unknown>;

const readCalls: Array<[name: string, call: ReadCall]> = [
  ["getMe", (signal) => getMe(signal)],
  ["listChannels", (signal) => listChannels(signal)],
  ["listCustomEmojis", (signal) => listCustomEmojis(signal)],
  ["listReadStates", (signal) => listReadStates(signal)],
  ["listMessages", (signal) => listMessages("42", signal)],
  ["listParticipatedThreads", (signal) => listParticipatedThreads(signal)],
  ["getThread", (signal) => getThread(7, {}, signal)],
  ["getPublicServerConfig", (signal) => getPublicServerConfig(undefined, signal)],
  ["searchUsers", (signal) => searchUsers({}, signal)],
  ["listVoiceParticipants", (signal) => listVoiceParticipants(42, signal)],
  ["listScreenShareStreams", (signal) => listScreenShareStreams(42, signal)],
  ["listCameraStreams", (signal) => listCameraStreams(42, signal)],
];

beforeEach(() => {
  fetchMock.mockReset();
  clearCachedCsrfToken();
  vi.stubGlobal("fetch", fetchMock);
});

afterEach(() => {
  clearCachedCsrfToken();
  vi.unstubAllGlobals();
});

describe("read API request plumbing", () => {
  test.each(readCalls)("%s propagates its AbortSignal", async (_name, call) => {
    const controller = new AbortController();
    fetchMock.mockResolvedValue(
      new Response("{}", { status: 200, headers: { "Content-Type": "application/json" } }),
    );

    await call(controller.signal);

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][1].signal).toBe(controller.signal);
  });

  test.each(readCalls)("%s rejects non-2xx before parsing", async (_name, call) => {
    fetchMock.mockResolvedValue(
      new Response("not valid JSON", {
        status: 503,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(call()).rejects.toThrow(/503/);
  });
});

describe("logout error handling", () => {
  test("rejects non-2xx responses", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 503 }));

    await expect(logout()).rejects.toThrow("Logout failed (503)");
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});

describe("getMe error distinctions", () => {
  test("returns null only for HTTP 401", async () => {
    fetchMock.mockResolvedValue(new Response("not valid JSON", { status: 401 }));

    await expect(getMe()).resolves.toBeNull();
  });

  test("rejects abort errors", async () => {
    const abortError = new DOMException("The operation was aborted", "AbortError");
    fetchMock.mockRejectedValue(abortError);

    await expect(getMe(new AbortController().signal)).rejects.toBe(abortError);
  });

  test("rejects network errors", async () => {
    const networkError = new TypeError("Failed to fetch");
    fetchMock.mockRejectedValue(networkError);

    await expect(getMe()).rejects.toBe(networkError);
  });

  test("rejects malformed successful responses", async () => {
    fetchMock.mockResolvedValue(
      new Response("not valid JSON", {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(getMe()).rejects.toBeInstanceOf(SyntaxError);
  });

  test("rejects non-401 HTTP errors", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 500 }));

    await expect(getMe()).rejects.toThrow(/500/);
  });
});
