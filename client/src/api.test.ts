import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import {
  getServerUrl,
  setServerUrl,
  login,
  listChannels,
  reorderChannels,
  sendMessage,
  listParticipatedThreads,
  getThread,
  sendThreadReply,
  editMessage,
  deleteMessage,
  messageDisplayName,
  updateDisplayName,
  listCustomEmojis,
  uploadCustomEmoji,
  renameCustomEmoji,
  deleteCustomEmoji,
  restoreCustomEmoji,
  type Channel,
  type CustomEmoji,
} from "./api";

const DEFAULT_SERVER = import.meta.env.VITE_HAMLET_DEFAULT_SERVER_URL ?? "http://127.0.0.1:3030";

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
      { id: 1, name: "general", position: 0, type: "text" },
      { id: 2, name: "random", position: 1, type: "text" },
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
      { id: 2, name: "random", position: 0, type: "text" },
      { id: 1, name: "general", position: 1, type: "text" },
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

  test("listCustomEmojis parses the registry array", async () => {
    const emojis: CustomEmoji[] = [
      {
        id: 1,
        name: "party",
        image_url: "/uploads/emojis/1.webp?v=10",
        animated: false,
        created_by_user_id: 1,
        created_at: 10,
        updated_at: 10,
        deleted_at: null,
      },
    ];
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(emojis), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(listCustomEmojis()).resolves.toEqual(emojis);
    expect(fetchMock.mock.calls[0][0]).toBe(`${DEFAULT_SERVER}/emojis`);
  });

  test("listCustomEmojis throws on non-2xx", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 500 }));
    await expect(listCustomEmojis()).rejects.toThrow(/500/);
  });

  test("uploadCustomEmoji posts multipart form data and parses response", async () => {
    const created: CustomEmoji = {
      id: 9,
      name: "party",
      image_url: "/uploads/emojis/9.webp?v=10",
      animated: false,
      created_by_user_id: 1,
      created_at: 10,
      updated_at: 10,
      deleted_at: null,
    };
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(created), {
        status: 201,
        headers: { "Content-Type": "application/json" },
      }),
    );

    const file = new File([new Uint8Array([1, 2, 3])], "party.png", { type: "image/png" });
    await expect(uploadCustomEmoji("party", file)).resolves.toEqual(created);

    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`${DEFAULT_SERVER}/emojis`);
    expect(init.method).toBe("POST");
    expect(init.credentials).toBe("include");
    expect(init.body).toBeInstanceOf(FormData);
    const body = init.body as FormData;
    expect(body.get("name")).toBe("party");
    expect(body.get("file")).toBe(file);
  });

  test("uploadCustomEmoji surfaces server validation messages", async () => {
    fetchMock.mockResolvedValue(
      new Response(
        JSON.stringify({
          error: { kind: "emoji_name_taken", message: "custom emoji name already exists" },
        }),
        { status: 409, headers: { "Content-Type": "application/json" } },
      ),
    );

    const file = new File([new Uint8Array([1])], "party.png", { type: "image/png" });
    await expect(uploadCustomEmoji("party", file)).rejects.toThrow(
      /custom emoji name already exists/i,
    );
  });

  test("renameCustomEmoji patches a new name and parses response", async () => {
    const updated: CustomEmoji = {
      id: 9,
      name: "renamed_party",
      image_url: "/uploads/emojis/9.webp?v=11",
      animated: false,
      created_by_user_id: 1,
      created_at: 10,
      updated_at: 11,
      deleted_at: null,
    };
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(updated), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(renameCustomEmoji(9, "renamed_party")).resolves.toEqual(updated);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`${DEFAULT_SERVER}/emojis/9`);
    expect(init.method).toBe("PATCH");
    expect(init.headers).toEqual({ "Content-Type": "application/json" });
    expect(JSON.parse(init.body)).toEqual({ name: "renamed_party" });
  });

  test("renameCustomEmoji surfaces server validation messages", async () => {
    fetchMock.mockResolvedValue(
      new Response(
        JSON.stringify({
          error: { kind: "emoji_name_taken", message: "custom emoji name already exists" },
        }),
        { status: 409, headers: { "Content-Type": "application/json" } },
      ),
    );

    await expect(renameCustomEmoji(9, "party")).rejects.toThrow(
      /custom emoji name already exists/i,
    );
  });

  test("deleteCustomEmoji soft-deletes and parses response", async () => {
    const deleted: CustomEmoji = {
      id: 9,
      name: "party",
      image_url: "/uploads/emojis/9.webp?v=12",
      animated: false,
      created_by_user_id: 1,
      created_at: 10,
      updated_at: 12,
      deleted_at: 12,
    };
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(deleted), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(deleteCustomEmoji(9)).resolves.toEqual(deleted);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`${DEFAULT_SERVER}/emojis/9`);
    expect(init.method).toBe("DELETE");
  });

  test("restoreCustomEmoji restores and surfaces conflicts", async () => {
    const restored: CustomEmoji = {
      id: 9,
      name: "party",
      image_url: "/uploads/emojis/9.webp?v=13",
      animated: false,
      created_by_user_id: 1,
      created_at: 10,
      updated_at: 13,
      deleted_at: null,
    };
    fetchMock.mockResolvedValueOnce(
      new Response(JSON.stringify(restored), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );
    await expect(restoreCustomEmoji(9)).resolves.toEqual(restored);
    expect(fetchMock.mock.calls[0][0]).toBe(`${DEFAULT_SERVER}/emojis/9/restore`);
    expect(fetchMock.mock.calls[0][1].method).toBe("POST");

    fetchMock.mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          error: { kind: "emoji_name_taken", message: "custom emoji name already exists" },
        }),
        { status: 409, headers: { "Content-Type": "application/json" } },
      ),
    );
    await expect(restoreCustomEmoji(9)).rejects.toThrow(/custom emoji name already exists/i);
  });

  test("sendMessage targets the channel-specific endpoint", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 200 }));
    await sendMessage("42", "hi");

    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`${DEFAULT_SERVER}/message/42`);
    expect(init.method).toBe("POST");
    expect(JSON.parse(init.body)).toEqual({ text: "hi" });
  });

  test("listParticipatedThreads fetches the global participation endpoint", async () => {
    const previews = [
      {
        channel: { id: 100, name: "general", position: 0, type: "text" },
        root: {
          id: 7,
          user_id: 1,
          channel_id: 100,
          parent_id: null,
          text: "root",
          username: "alice",
          display_name: null,
          avatar_url: null,
          suppress_embeds: false,
          embeds: [],
        },
        reply_count: 2,
        last_reply_created_at: 1_700_000_000_000_000,
        recent_replies: [],
      },
    ];
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(previews), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(listParticipatedThreads()).resolves.toEqual(previews);
    expect(fetchMock.mock.calls[0][0]).toBe(`${DEFAULT_SERVER}/threads/participated`);
  });

  test("getThread fetches the root-specific thread endpoint", async () => {
    const thread = {
      root: {
        id: 7,
        user_id: 1,
        channel_id: 100,
        parent_id: null,
        text: "root",
        username: "alice",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        embeds: [],
      },
      replies: [],
      has_more_replies: false,
    };
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(thread), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(getThread(7)).resolves.toEqual(thread);
    expect(fetchMock.mock.calls[0][0]).toBe(`${DEFAULT_SERVER}/thread/7`);
  });

  test("getThread can request an older bounded replies page", async () => {
    const thread = { root: {}, replies: [], has_more_replies: false };
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(thread), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(
      getThread(7, { limit: 25, beforeCreatedAt: 1_700_000_000_000_000, beforeId: 42 }),
    ).resolves.toEqual(thread);
    expect(fetchMock.mock.calls[0][0]).toBe(
      `${DEFAULT_SERVER}/thread/7?limit=25&before_created_at=1700000000000000&before_id=42`,
    );
  });

  test("sendThreadReply posts text and parses the created reply", async () => {
    const reply = {
      id: 8,
      user_id: 1,
      channel_id: 100,
      parent_id: 7,
      text: "reply",
      username: "alice",
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      embeds: [],
    };
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(reply), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(sendThreadReply(7, "reply")).resolves.toEqual(reply);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`${DEFAULT_SERVER}/thread/7/reply`);
    expect(init.method).toBe("POST");
    expect(init.headers).toEqual({ "Content-Type": "application/json" });
    expect(JSON.parse(init.body)).toEqual({ text: "reply" });
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

  test("deleteMessage sends DELETE with credentials", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 204 }));
    await expect(deleteMessage(42)).resolves.toBeUndefined();

    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`${DEFAULT_SERVER}/message/42`);
    expect(init.method).toBe("DELETE");
    expect(init.credentials).toBe("include");
  });

  test("deleteMessage throws on non-2xx", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 403 }));
    await expect(deleteMessage(42)).rejects.toThrow(/403/);
  });

  test("uses the stored server URL for subsequent calls", async () => {
    setServerUrl("http://example.test:9000");
    fetchMock.mockResolvedValue(new Response(null, { status: 200 }));
    await login("a", "b");
    expect(fetchMock.mock.calls[0][0]).toBe("http://example.test:9000/login");
  });

  test("updateDisplayName sends PUT /me with display_name body", async () => {
    const me = {
      id: 1,
      username: "alice",
      display_name: "Ally",
      email: null,
      email_verified: false,
      avatar_url: null,
    };
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(me), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(updateDisplayName("Ally")).resolves.toEqual(me);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`${DEFAULT_SERVER}/me`);
    expect(init.method).toBe("PUT");
    expect(init.headers).toEqual({ "Content-Type": "application/json" });
    expect(JSON.parse(init.body)).toEqual({ display_name: "Ally" });
  });

  test("updateDisplayName passes null to clear the display name", async () => {
    fetchMock.mockResolvedValue(
      new Response(
        JSON.stringify({
          id: 1,
          username: "alice",
          display_name: null,
          email: null,
          email_verified: false,
          avatar_url: null,
        }),
        { status: 200, headers: { "Content-Type": "application/json" } },
      ),
    );
    await updateDisplayName(null);
    expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toEqual({ display_name: null });
  });

  test("updateDisplayName throws on non-2xx", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 400 }));
    await expect(updateDisplayName("too long")).rejects.toThrow(/400/);
  });
});

describe("messageDisplayName", () => {
  test("prefers display_name when present", () => {
    expect(messageDisplayName({ username: "alice", display_name: "Ally" })).toBe("Ally");
  });

  test("falls back to username when display_name is null", () => {
    expect(messageDisplayName({ username: "alice", display_name: null })).toBe("alice");
  });
});
