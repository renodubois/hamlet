import { describe, expect, test } from "vitest";
import { getThread, listMessages, sendMessage, sendThreadReply, type Message } from "../../api";
import { tinyPngFile, tinyWebpFile } from "../image-fixtures";
import { DEV_USER } from "./handlers";
import { resetMswState } from "./server";

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
