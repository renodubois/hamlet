import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import {
  getServerUrl,
  setServerUrl,
  login,
  listChannels,
  reorderChannels,
  sendMessage,
  editMessage,
  type Channel,
} from "./api";

const DEFAULT_SERVER = "http://localhost:3030";

describe("server url", () => {
  test("defaults when nothing is stored", () => {
    expect(getServerUrl()).toBe(DEFAULT_SERVER);
  });

  test("round-trips through localStorage", () => {
    setServerUrl("http://example.test:9000");
    expect(getServerUrl()).toBe("http://example.test:9000");
    expect(localStorage.getItem("hamlet.serverUrl")).toBe("http://example.test:9000");
  });
});

describe("apiFetch behavior", () => {
  const fetchMock = vi.fn();

  beforeEach(() => {
    fetchMock.mockReset();
    vi.stubGlobal("fetch", fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  test("login posts JSON with credentials included", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 200 }));
    await login("alice", "hunter2");

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`${DEFAULT_SERVER}/login`);
    expect(init.method).toBe("POST");
    expect(init.credentials).toBe("include");
    expect(init.headers).toEqual({ "Content-Type": "application/json" });
    expect(JSON.parse(init.body)).toEqual({ username: "alice", password: "hunter2" });
  });

  test("listChannels parses JSON array", async () => {
    const channels: Channel[] = [
      { id: 1, name: "general", position: 0 },
      { id: 2, name: "random", position: 1 },
    ];
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(channels), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(listChannels()).resolves.toEqual(channels);
    expect(fetchMock.mock.calls[0][0]).toBe(`${DEFAULT_SERVER}/channels`);
  });

  test("reorderChannels sends PUT with ids body and parses response", async () => {
    const updated: Channel[] = [
      { id: 2, name: "random", position: 0 },
      { id: 1, name: "general", position: 1 },
    ];
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(updated), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(reorderChannels([2, 1])).resolves.toEqual(updated);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`${DEFAULT_SERVER}/channels/order`);
    expect(init.method).toBe("PUT");
    expect(init.headers).toEqual({ "Content-Type": "application/json" });
    expect(JSON.parse(init.body)).toEqual({ ids: [2, 1] });
  });

  test("reorderChannels throws on non-2xx", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 400 }));
    await expect(reorderChannels([2, 1])).rejects.toThrow(/400/);
  });

  test("sendMessage targets the channel-specific endpoint", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 200 }));
    await sendMessage("42", "hi");

    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`${DEFAULT_SERVER}/message/42`);
    expect(init.method).toBe("POST");
    expect(JSON.parse(init.body)).toEqual({ text: "hi" });
  });

  test("editMessage sends PUT with text body and parses message response", async () => {
    const updated = {
      id: 7,
      user_id: 1,
      channel_id: 100,
      text: "fixed typo",
      username: "alice",
      avatar_url: null,
    };
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(updated), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(editMessage(7, "fixed typo")).resolves.toEqual(updated);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`${DEFAULT_SERVER}/message/7`);
    expect(init.method).toBe("PUT");
    expect(init.headers).toEqual({ "Content-Type": "application/json" });
    expect(JSON.parse(init.body)).toEqual({ text: "fixed typo" });
  });

  test("editMessage throws on non-2xx", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 403 }));
    await expect(editMessage(7, "nope")).rejects.toThrow(/403/);
  });

  test("uses the stored server URL for subsequent calls", async () => {
    setServerUrl("http://example.test:9000");
    fetchMock.mockResolvedValue(new Response(null, { status: 200 }));
    await login("a", "b");
    expect(fetchMock.mock.calls[0][0]).toBe("http://example.test:9000/login");
  });
});
