import { describe, expect, test, vi } from "vitest";
import {
  getServerUrl,
  getThread,
  listMessages,
  listScreenShareStreams,
  sendMessage,
  sendThreadReply,
  type Message,
} from "../../api";
import { makeAttachment, makeScreenShareStream } from "../fixtures";
import { tinyPngFile, tinyWebpFile } from "../image-fixtures";
import { DEV_USER } from "./handlers";
import { startMswScreenShare, stoppedScreenShareFrom, stopMswScreenShare } from "./screen-share";
import { resetMswState } from "./server";
import { FakeEventSource } from "./sse";

function seedThreadRoot(): { state: ReturnType<typeof resetMswState>; root: Message } {
  const state = resetMswState({ me: DEV_USER });
  const root: Message = {
    id: 42,
    user_id: DEV_USER.id,
    channel_id: 100,
    parent_id: null,
    text: "root for uploaded replies",
    username: DEV_USER.username,
    display_name: null,
    avatar_url: null,
    suppress_embeds: false,
    attachments: [],
    embeds: [],
    reactions: [],
  };
  state.messages["100"] = [root];
  return { state, root };
}

describe("MSW screen share handlers", () => {
  test("return current streams and support channel filtering", async () => {
    const first = makeScreenShareStream({ channel_id: 100, sharer_user_id: 1, track_sid: "TR_a" });
    const second = makeScreenShareStream({ channel_id: 200, sharer_user_id: 2, track_sid: "TR_b" });
    resetMswState({ me: DEV_USER, screenShareStreams: [first, second] });

    await expect(listScreenShareStreams()).resolves.toEqual([first, second]);
    await expect(listScreenShareStreams(100)).resolves.toEqual([first]);
  });

  test("fixtures keep active streams and SSE start/stop events in sync", async () => {
    const state = resetMswState({ me: DEV_USER });
    const stream = makeScreenShareStream({
      channel_id: 100,
      sharer_user_id: 1,
      track_sid: "TR_fixture",
    });
    const eventSource = new FakeEventSource("http://127.0.0.1:3030/messages/subscribe");
    const onmessage = vi.fn<(event: MessageEvent<string>) => void>();
    eventSource.onmessage = onmessage;

    startMswScreenShare(state, stream);

    await expect(listScreenShareStreams(100)).resolves.toEqual([stream]);
    expect(onmessage).toHaveBeenLastCalledWith(
      expect.objectContaining({
        data: JSON.stringify({ kind: "screen_share_started", data: stream }),
      }),
    );

    const stopped = stoppedScreenShareFrom(stream);
    stopMswScreenShare(state, stopped);

    await expect(listScreenShareStreams(100)).resolves.toEqual([]);
    expect(onmessage).toHaveBeenLastCalledWith(
      expect.objectContaining({
        data: JSON.stringify({ kind: "screen_share_stopped", data: stopped }),
      }),
    );
  });
});

describe("MSW message upload handlers", () => {
  test("preserve JSON text-only channel sends while storing the returned message", async () => {
    const state = resetMswState({ me: DEV_USER });

    const response = await sendMessage("100", "json text only");
    expect(response.ok).toBe(true);
    const created = (await response.json()) as Message;

    expect(created).toMatchObject({
      channel_id: 100,
      parent_id: null,
      text: "json text only",
      attachments: [],
      embeds: [],
    });
    expect(state.sentMessages).toContainEqual({ channel: "100", text: "json text only" });
    expect(state.sentMessagePhotos).toEqual([]);
    await expect(listMessages("100")).resolves.toContainEqual(created);
  });

  test("tracks JSON inline reply targets and returns compact reply metadata", async () => {
    const state = resetMswState({ me: DEV_USER });
    const target: Message = {
      id: 41,
      user_id: 2,
      channel_id: 100,
      parent_id: null,
      created_at: 1_700_000_000_000_000,
      text: "target text",
      username: "bob",
      display_name: "Bobby",
      avatar_url: null,
      suppress_embeds: false,
      attachments: [],
      embeds: [],
      reactions: [],
    };
    state.messages["100"] = [target];

    const response = await sendMessage("100", "inline body", [], { replyToMessageId: target.id });
    expect(response.ok).toBe(true);
    const created = (await response.json()) as Message;

    expect(state.sentInlineReplies).toEqual([
      { channel: "100", text: "inline body", replyToMessageId: target.id },
    ]);
    expect(created).toMatchObject({
      channel_id: 100,
      parent_id: null,
      reply_to_message_id: target.id,
      text: "inline body",
    });
    expect(created.reply_to).toEqual({
      id: target.id,
      user_id: target.user_id,
      channel_id: target.channel_id,
      created_at: target.created_at,
      deleted_at: null,
      text: target.text,
      attachment_count: 0,
      username: target.username,
      display_name: target.display_name,
      avatar_url: target.avatar_url,
    });
    await expect(listMessages("100")).resolves.toContainEqual(created);
  });

  test("tracks multipart inline reply targets and persists attachment-shaped message payloads", async () => {
    const state = resetMswState({ me: DEV_USER });
    const target: Message = {
      id: 51,
      user_id: 2,
      channel_id: 100,
      parent_id: null,
      created_at: 1_700_000_000_000_000,
      text: "photo target text",
      username: "bob",
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      attachments: [],
      embeds: [],
      reactions: [],
    };
    state.messages["100"] = [target];
    const photo = tinyPngFile("reply-photo.png");

    const response = await sendMessage("100", "photo inline body", [photo], {
      replyToMessageId: target.id,
    });
    expect(response.ok).toBe(true);
    const created = (await response.json()) as Message;

    expect(state.sentInlineReplies).toEqual([
      { channel: "100", text: "photo inline body", replyToMessageId: target.id },
    ]);
    expect(state.sentMessagePhotos).toEqual([
      {
        channel: "100",
        text: "photo inline body",
        photos: [{ name: "reply-photo.png", size: photo.size, type: "image/png" }],
      },
    ]);
    expect(created).toMatchObject({
      channel_id: 100,
      parent_id: null,
      reply_to_message_id: target.id,
      text: "photo inline body",
    });
    expect(created.reply_to).toEqual({
      id: target.id,
      user_id: target.user_id,
      channel_id: target.channel_id,
      created_at: target.created_at,
      deleted_at: null,
      text: target.text,
      attachment_count: 0,
      username: target.username,
      display_name: target.display_name,
      avatar_url: target.avatar_url,
    });
    expect(created.attachments).toHaveLength(1);
    await expect(listMessages("100")).resolves.toContainEqual(created);
  });

  test("rejects invalid inline reply targets and thread reply inline targets", async () => {
    const state = resetMswState({ me: DEV_USER });
    const root: Message = {
      id: 61,
      user_id: DEV_USER.id,
      channel_id: 100,
      parent_id: null,
      created_at: 1_700_000_000_000_000,
      text: "root",
      username: DEV_USER.username,
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      attachments: [],
      embeds: [],
      reactions: [],
    };
    const threadReply: Message = {
      ...root,
      id: 62,
      parent_id: root.id,
      text: "thread reply cannot be an inline target",
    };
    state.messages["100"] = [root];
    state.threadReplies[String(root.id)] = [threadReply];

    const jsonInvalid = await fetch(`${getServerUrl()}/message/100`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ text: "bad", reply_to_message_id: threadReply.id }),
    });
    expect(jsonInvalid.status).toBe(400);
    await expect(jsonInvalid.json()).resolves.toMatchObject({
      error: { kind: "reply_target_not_top_level" },
    });

    const form = new FormData();
    form.append("text", "missing target");
    form.append("reply_to_message_id", "9000000000000000");
    const multipartMissing = await fetch(`${getServerUrl()}/message/100`, {
      method: "POST",
      body: form,
    });
    expect(multipartMissing.status).toBe(404);
    await expect(multipartMissing.json()).resolves.toMatchObject({
      error: { kind: "reply_target_not_found" },
    });

    const threadJson = await fetch(`${getServerUrl()}/thread/${root.id}/reply`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ text: "bad thread", reply_to_message_id: root.id }),
    });
    expect(threadJson.status).toBe(400);
    await expect(threadJson.json()).resolves.toMatchObject({
      error: { kind: "thread_inline_reply_not_allowed" },
    });

    const threadForm = new FormData();
    threadForm.append("text", "bad thread multipart");
    threadForm.append("reply_to_message_id", String(root.id));
    const threadMultipart = await fetch(`${getServerUrl()}/thread/${root.id}/reply`, {
      method: "POST",
      body: threadForm,
    });
    expect(threadMultipart.status).toBe(400);
    await expect(threadMultipart.json()).resolves.toMatchObject({
      error: { kind: "thread_inline_reply_not_allowed" },
    });
    expect(state.sentMessages).toEqual([]);
    expect(state.sentThreadReplies).toEqual([]);
  });

  test("tracks multipart channel photos and persists attachment-shaped message payloads", async () => {
    const state = resetMswState({ me: DEV_USER });
    const first = tinyPngFile("channel-cat.png");
    const second = tinyWebpFile("channel-dog.webp");

    const response = await sendMessage("100", "photo caption", [first, second]);
    expect(response.ok).toBe(true);
    const created = (await response.json()) as Message;

    expect(state.sentMessages).toContainEqual({ channel: "100", text: "photo caption" });
    expect(state.sentMessagePhotos).toEqual([
      {
        channel: "100",
        text: "photo caption",
        photos: [
          { name: "channel-cat.png", size: first.size, type: "image/png" },
          { name: "channel-dog.webp", size: second.size, type: "image/webp" },
        ],
      },
    ]);
    expect(created.attachments).toHaveLength(2);
    expect(created.attachments[0]).toMatchObject({
      message_id: created.id,
      position: 0,
      content_type: "image/webp",
      thumbnail_content_type: "image/webp",
      url: expect.stringMatching(/^\/attachments\/\d+$/),
      thumbnail_url: expect.stringMatching(/^\/attachments\/\d+\/thumbnail$/),
    });
    expect(created.attachments[1]).toMatchObject({
      message_id: created.id,
      position: 1,
      content_type: "image/webp",
      thumbnail_content_type: "image/webp",
    });

    const persisted = (await listMessages("100")).find((message) => message.id === created.id);
    expect(persisted?.attachments).toEqual(created.attachments);
  });

  test("tracks multipart inline reply photos and returns compact attachment metadata", async () => {
    const state = resetMswState({ me: DEV_USER });
    const target: Message = {
      id: 45,
      user_id: 2,
      channel_id: 100,
      parent_id: null,
      created_at: 1_700_000_000_000_000,
      text: "",
      username: "bob",
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      attachments: [makeAttachment({ id: 7001, message_id: 45 })],
      embeds: [],
      reactions: [],
    };
    state.messages["100"] = [target];
    const photo = tinyPngFile("inline-reply.png");

    const response = await sendMessage("100", "caption with photo", [photo], {
      replyToMessageId: target.id,
    });
    expect(response.ok).toBe(true);
    const created = (await response.json()) as Message;

    expect(state.sentInlineReplies).toEqual([
      { channel: "100", text: "caption with photo", replyToMessageId: target.id },
    ]);
    expect(state.sentMessagePhotos).toEqual([
      {
        channel: "100",
        text: "caption with photo",
        photos: [{ name: "inline-reply.png", size: photo.size, type: "image/png" }],
      },
    ]);
    expect(created).toMatchObject({
      channel_id: 100,
      parent_id: null,
      reply_to_message_id: target.id,
      text: "caption with photo",
      reply_to: {
        id: target.id,
        text: "",
        attachment_count: 1,
        username: "bob",
      },
    });
    expect(created.attachments).toHaveLength(1);
    await expect(listMessages("100")).resolves.toContainEqual(created);
  });

  test("tracks multipart thread reply photos and returns attachment-shaped replies", async () => {
    const { state, root } = seedThreadRoot();
    const photo = tinyPngFile("thread-cat.png");

    const reply = await sendThreadReply(root.id, "thread photo caption", [photo]);

    expect(state.sentThreadReplies).toContainEqual({
      rootId: root.id,
      text: "thread photo caption",
    });
    expect(state.sentThreadReplyPhotos).toEqual([
      {
        rootId: root.id,
        text: "thread photo caption",
        photos: [{ name: "thread-cat.png", size: photo.size, type: "image/png" }],
      },
    ]);
    expect(reply).toMatchObject({
      channel_id: root.channel_id,
      parent_id: root.id,
      text: "thread photo caption",
    });
    expect(reply.attachments).toHaveLength(1);
    expect(reply.attachments[0]).toMatchObject({
      message_id: reply.id,
      position: 0,
      content_type: "image/webp",
      thumbnail_content_type: "image/webp",
      url: expect.stringMatching(/^\/attachments\/\d+$/),
      thumbnail_url: expect.stringMatching(/^\/attachments\/\d+\/thumbnail$/),
    });

    const thread = await getThread(root.id);
    expect(thread.replies.find((message) => message.id === reply.id)?.attachments).toEqual(
      reply.attachments,
    );
  });
});
