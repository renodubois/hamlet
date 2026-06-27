import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import {
  getServerUrl,
  setServerUrl,
  login,
  searchUsers,
  listChannels,
  reorderChannels,
  listReadStates,
  markChannelRead,
  sendMessage,
  listParticipatedThreads,
  getThread,
  sendThreadReply,
  editMessage,
  deleteMessage,
  addMessageReaction,
  removeMessageReaction,
  messageDisplayName,
  updateDisplayName,
  MessagePhotoValidationError,
  listCustomEmojis,
  uploadCustomEmoji,
  renameCustomEmoji,
  deleteCustomEmoji,
  restoreCustomEmoji,
  listCameraStreams,
  listScreenShareStreams,
  postVoiceStatus,
  type CameraStream,
  type Channel,
  type CustomEmoji,
  type PublicUser,
  type ReadStateSummary,
  type ScreenShareStream,
} from "./api";
import { tinyPngFile, tinyWebpFile } from "./test/image-fixtures";

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

  test("searchUsers serializes empty query and limit", async () => {
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify([]), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(searchUsers({ query: "", limit: 5 })).resolves.toEqual([]);

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][0]).toBe(`${DEFAULT_SERVER}/users?q=&limit=5`);
    expect(fetchMock.mock.calls[0][1].credentials).toBe("include");
  });

  test("searchUsers serializes typed query and parses public user DTOs", async () => {
    const users: PublicUser[] = [
      {
        id: 1,
        username: "alice",
        display_name: "Alice",
        avatar_url: "/uploads/avatars/1.webp?v=1",
      },
    ];
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(users), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(searchUsers({ query: "Ali & Bob" })).resolves.toEqual(users);

    const [url] = fetchMock.mock.calls[0];
    const parsed = new URL(url);
    expect(`${parsed.origin}${parsed.pathname}`).toBe(`${DEFAULT_SERVER}/users`);
    expect(parsed.searchParams.get("q")).toBe("Ali & Bob");
    expect(parsed.searchParams.has("limit")).toBe(false);
  });

  test("searchUsers throws on non-2xx", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 401 }));
    await expect(searchUsers({ query: "alice" })).rejects.toThrow(/401/);
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

  test("listReadStates fetches the authenticated read-state snapshot", async () => {
    const snapshot: ReadStateSummary[] = [
      {
        channel_id: 10,
        has_unread: true,
        mention_count: 0,
        last_read_created_at: 100,
        last_read_message_id: 20,
        updated_at: 200,
      },
    ];
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(snapshot), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(listReadStates()).resolves.toEqual(snapshot);
    expect(fetchMock.mock.calls[0][0]).toBe(`${DEFAULT_SERVER}/read-states`);
    expect(fetchMock.mock.calls[0][1].credentials).toBe("include");
  });

  test("listReadStates throws on non-2xx", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 401 }));
    await expect(listReadStates()).rejects.toThrow(/401/);
  });

  test("markChannelRead sends the last visible message id and parses the summary", async () => {
    const summary: ReadStateSummary = {
      channel_id: 10,
      has_unread: false,
      mention_count: 0,
      last_read_created_at: 100,
      last_read_message_id: 20,
      updated_at: 300,
    };
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(summary), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(markChannelRead(10, 20)).resolves.toEqual(summary);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`${DEFAULT_SERVER}/channels/10/read-state`);
    expect(init.method).toBe("PUT");
    expect(init.headers).toEqual({ "Content-Type": "application/json" });
    expect(init.credentials).toBe("include");
    expect(JSON.parse(init.body)).toEqual({ last_visible_message_id: 20 });
  });

  test("markChannelRead throws on non-2xx", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 400 }));
    await expect(markChannelRead(10, 20)).rejects.toThrow(/400/);
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

  test("sendMessage uses JSON for text-only channel messages", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 200 }));
    await sendMessage("42", "hi");

    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`${DEFAULT_SERVER}/message/42`);
    expect(init.method).toBe("POST");
    expect(init.headers).toEqual({ "Content-Type": "application/json" });
    expect(JSON.parse(init.body)).toEqual({ text: "hi" });
  });

  test("sendMessage includes the inline reply target only when provided", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 200 }));
    await sendMessage("42", "reply body", [], { replyToMessageId: 7 });

    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`${DEFAULT_SERVER}/message/42`);
    expect(init.method).toBe("POST");
    expect(init.headers).toEqual({ "Content-Type": "application/json" });
    expect(JSON.parse(init.body)).toEqual({ text: "reply body", reply_to_message_id: 7 });
  });

  test("sendMessage uses FormData with repeated photos when photos are supplied", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 200 }));
    const first = tinyPngFile("first.png");
    const second = tinyWebpFile("second.webp");

    await sendMessage("42", "caption", [first, second]);

    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`${DEFAULT_SERVER}/message/42`);
    expect(init.method).toBe("POST");
    expect(init.headers).toBeUndefined();
    expect(init.body).toBeInstanceOf(FormData);
    const body = init.body as FormData;
    expect(body.get("text")).toBe("caption");
    expect(body.get("reply_to_message_id")).toBeNull();
    expect(body.getAll("photos")).toEqual([first, second]);
  });

  test("sendMessage includes inline reply targets in multipart photo sends", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 200 }));
    const photo = tinyPngFile("reply.png");

    await sendMessage("42", "", [photo], { replyToMessageId: 7 });

    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`${DEFAULT_SERVER}/message/42`);
    expect(init.method).toBe("POST");
    expect(init.headers).toBeUndefined();
    expect(init.body).toBeInstanceOf(FormData);
    const body = init.body as FormData;
    expect(body.get("text")).toBe("");
    expect(body.get("reply_to_message_id")).toBe("7");
    expect(body.getAll("photos")).toEqual([photo]);
  });

  test("sendMessage rejects invalid photos before making a request", async () => {
    const invalid = new File(["not a photo"], "not-a-photo.gif", { type: "image/gif" });

    await expect(sendMessage("42", "caption", [invalid])).rejects.toBeInstanceOf(
      MessagePhotoValidationError,
    );
    expect(fetchMock).not.toHaveBeenCalled();
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
          mentions: [],
          attachments: [],
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
        mentions: [],
        attachments: [],
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
      mentions: [],
      attachments: [],
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

  test("sendThreadReply uses FormData with repeated photos when photos are supplied", async () => {
    const reply = {
      id: 8,
      user_id: 1,
      channel_id: 100,
      parent_id: 7,
      text: "caption",
      username: "alice",
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [],
      attachments: [],
      embeds: [],
    };
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(reply), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );
    const first = tinyPngFile("first.png");
    const second = tinyWebpFile("second.webp");

    await expect(sendThreadReply(7, "caption", [first, second])).resolves.toEqual(reply);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`${DEFAULT_SERVER}/thread/7/reply`);
    expect(init.method).toBe("POST");
    expect(init.headers).toBeUndefined();
    expect(init.body).toBeInstanceOf(FormData);
    const body = init.body as FormData;
    expect(body.get("text")).toBe("caption");
    expect(body.getAll("photos")).toEqual([first, second]);
  });

  test("sendThreadReply rejects invalid photos before making a request", async () => {
    const invalid = new File(["not a photo"], "not-a-photo.gif", { type: "image/gif" });

    await expect(sendThreadReply(7, "caption", [invalid])).rejects.toBeInstanceOf(
      MessagePhotoValidationError,
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  test("editMessage sends PUT with text body and parses message response", async () => {
    const updated = {
      id: 7,
      user_id: 1,
      channel_id: 100,
      text: "fixed typo",
      username: "alice",
      avatar_url: null,
      attachments: [],
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

  test("addMessageReaction posts native reaction body and parses summaries", async () => {
    const summaries = [{ kind: "native", emoji: "👍", count: 1, me_reacted: true }];
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(summaries), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(addMessageReaction(42, { kind: "native", emoji: "👍" })).resolves.toEqual(
      summaries,
    );
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`${DEFAULT_SERVER}/message/42/reactions`);
    expect(init.method).toBe("POST");
    expect(init.headers).toEqual({ "Content-Type": "application/json" });
    expect(JSON.parse(init.body)).toEqual({ kind: "native", emoji: "👍" });
  });

  test("addMessageReaction strips custom display fields and parses custom summaries", async () => {
    const summaries = [
      {
        kind: "custom",
        emoji_id: 123,
        name: "party",
        image_url: "/uploads/emojis/123.webp?v=2",
        animated: true,
        count: 2,
        me_reacted: true,
      },
    ];
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(summaries), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(
      addMessageReaction(42, {
        kind: "custom",
        emoji_id: 123,
        name: "party",
        image_url: "/uploads/emojis/123.webp?v=1",
        animated: true,
      }),
    ).resolves.toEqual(summaries);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`${DEFAULT_SERVER}/message/42/reactions`);
    expect(init.method).toBe("POST");
    expect(JSON.parse(init.body)).toEqual({ kind: "custom", emoji_id: 123 });
  });

  test("removeMessageReaction sends DELETE with native reaction body", async () => {
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify([]), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(removeMessageReaction(42, { kind: "native", emoji: "👍" })).resolves.toEqual([]);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`${DEFAULT_SERVER}/message/42/reactions`);
    expect(init.method).toBe("DELETE");
    expect(init.headers).toEqual({ "Content-Type": "application/json" });
    expect(JSON.parse(init.body)).toEqual({ kind: "native", emoji: "👍" });
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

  test("listScreenShareStreams fetches active streams for a channel", async () => {
    const streams: ScreenShareStream[] = [
      {
        channel_id: 42,
        sharer_user_id: 7,
        username: "alice",
        display_name: "Alice",
        avatar_url: null,
        participant_identity: "7",
        track_sid: "TR_screen",
        track_name: "screen",
        source: "screen_share",
        started_at: 1_700_000_000,
      },
    ];
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(streams), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(listScreenShareStreams(42)).resolves.toEqual(streams);
    expect(fetchMock.mock.calls[0][0]).toBe(`${DEFAULT_SERVER}/voice/screen-shares?channel_id=42`);
    expect(fetchMock.mock.calls[0][1].credentials).toBe("include");
  });

  test("listScreenShareStreams throws on non-2xx", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 401 }));
    await expect(listScreenShareStreams()).rejects.toThrow(/401/);
    expect(fetchMock.mock.calls[0][0]).toBe(`${DEFAULT_SERVER}/voice/screen-shares`);
  });

  test("listCameraStreams fetches active cameras for a channel", async () => {
    const streams: CameraStream[] = [
      {
        channel_id: 42,
        sharer_user_id: 7,
        username: "alice",
        display_name: "Alice",
        avatar_url: null,
        participant_identity: "7",
        track_sid: "TR_camera",
        track_name: "camera",
        source: "camera",
        started_at: 1_700_000_000,
      },
    ];
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(streams), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(listCameraStreams(42)).resolves.toEqual(streams);
    expect(fetchMock.mock.calls[0][0]).toBe(`${DEFAULT_SERVER}/voice/cameras?channel_id=42`);
    expect(fetchMock.mock.calls[0][1].credentials).toBe("include");
  });

  test("listCameraStreams throws on non-2xx", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 401 }));
    await expect(listCameraStreams()).rejects.toThrow(/401/);
    expect(fetchMock.mock.calls[0][0]).toBe(`${DEFAULT_SERVER}/voice/cameras`);
  });

  test("postVoiceStatus sends current mute and deafen bits", async () => {
    fetchMock.mockResolvedValue(new Response(null, { status: 204 }));

    await expect(postVoiceStatus(true, false)).resolves.toBeUndefined();

    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(`${DEFAULT_SERVER}/voice/status`);
    expect(init.method).toBe("POST");
    expect(init.headers).toEqual({ "Content-Type": "application/json" });
    expect(init.credentials).toBe("include");
    expect(JSON.parse(init.body)).toEqual({ muted: true, deafened: false });
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
