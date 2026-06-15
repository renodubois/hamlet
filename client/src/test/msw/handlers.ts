import { http, HttpResponse } from "msw";
import type {
  CameraStream,
  Channel,
  CustomEmoji,
  Message,
  MessageAttachment,
  ParticipatedThreadPreview,
  ScreenShareStream,
  Thread,
  User,
  VoiceParticipant,
} from "../../api";

const BASE = import.meta.env.VITE_HAMLET_DEFAULT_SERVER_URL ?? "http://127.0.0.1:3030";

export const DEV_USER: User = {
  id: 1,
  username: "baipas",
  display_name: null,
  email: null,
  email_verified: false,
  avatar_url: null,
};

export const DEFAULT_CHANNELS: Channel[] = [
  { id: 100, name: "general", position: 0, type: "text" },
];

export interface HandlerState {
  me: User | null;
  channels: Channel[];
  messages: Record<string, Message[]>;
  validCredentials: { username: string; password: string };
  sentMessages: { channel: string; text: string }[];
  sentMessagePhotos: {
    channel: string;
    text: string;
    photos: { name: string; size: number; type: string }[];
  }[];
  threadReplies: Record<string, Message[]>;
  sentThreadReplies: { rootId: number; text: string }[];
  sentThreadReplyPhotos: {
    rootId: number;
    text: string;
    photos: { name: string; size: number; type: string }[];
  }[];
  threadFetches: number[];
  threadRequests: {
    rootId: number;
    limit: number;
    beforeCreatedAt: number | null;
    beforeId: number | null;
  }[];
  editedMessages: { id: number; text: string }[];
  deletedMessageIds: number[];
  createdChannels: { name: string; type?: string }[];
  reorderedIds: number[] | null;
  uploadedAvatars: { size: number; type: string }[];
  deletedAvatar: boolean;
  displayNameUpdates: (string | null)[];
  voiceParticipants: Record<string, VoiceParticipant[]>;
  voiceTokensMinted: number[];
  voiceStatusUpdates: { muted: boolean; deafened: boolean }[];
  screenShareStreams: ScreenShareStream[];
  cameraStreams: CameraStream[];
  customEmojis: CustomEmoji[];
  uploadedCustomEmojis: { name: string; size: number; type: string }[];
  renamedCustomEmojis: { id: number; name: string }[];
  deletedCustomEmojiIds: number[];
  restoredCustomEmojiIds: number[];
  typingPings: string[];
  suppressedEmbeds: { id: number; suppress: boolean }[];
}

export function createState(overrides: Partial<HandlerState> = {}): HandlerState {
  return {
    me: null,
    channels: [...DEFAULT_CHANNELS],
    messages: { "100": [] },
    validCredentials: { username: "baipas", password: "password" },
    sentMessages: [],
    sentMessagePhotos: [],
    threadReplies: {},
    sentThreadReplies: [],
    sentThreadReplyPhotos: [],
    threadFetches: [],
    threadRequests: [],
    editedMessages: [],
    deletedMessageIds: [],
    createdChannels: [],
    reorderedIds: null,
    uploadedAvatars: [],
    deletedAvatar: false,
    displayNameUpdates: [],
    voiceParticipants: {},
    voiceTokensMinted: [],
    voiceStatusUpdates: [],
    screenShareStreams: [],
    cameraStreams: [],
    customEmojis: [],
    uploadedCustomEmojis: [],
    renamedCustomEmojis: [],
    deletedCustomEmojiIds: [],
    restoredCustomEmojiIds: [],
    typingPings: [],
    suppressedEmbeds: [],
    ...overrides,
  };
}

function attachmentFromUploadedPhoto(
  photo: Blob,
  messageId: number,
  position: number,
): MessageAttachment {
  const id = Math.floor(Math.random() * 1000) + 5000 + position;
  return {
    id,
    message_id: messageId,
    position,
    content_type: "image/webp",
    byte_size: Math.max(1, photo.size),
    width: 1,
    height: 1,
    url: `/attachments/${id}`,
    thumbnail_url: `/attachments/${id}/thumbnail`,
    thumbnail_content_type: "image/webp",
    thumbnail_byte_size: Math.max(1, Math.ceil(photo.size / 2)),
    thumbnail_width: 1,
    thumbnail_height: 1,
  };
}

function findMessageById(currentState: HandlerState, id: number): Message | undefined {
  for (const list of Object.values(currentState.messages)) {
    const message = list.find((m) => m.id === id);
    if (message) return message;
  }
  for (const list of Object.values(currentState.threadReplies)) {
    const message = list.find((m) => m.id === id);
    if (message) return message;
  }
  return undefined;
}

export function createHandlers(state: HandlerState) {
  return [
    http.get(`${BASE}/me`, () => {
      if (!state.me) return new HttpResponse(null, { status: 401 });
      return HttpResponse.json(state.me);
    }),

    http.put(`${BASE}/me`, async ({ request }) => {
      if (!state.me) return new HttpResponse(null, { status: 401 });
      const body = (await request.json()) as { display_name: string | null };
      const raw = body.display_name;
      const trimmed = typeof raw === "string" ? raw.trim() : null;
      const next = trimmed && trimmed.length > 0 ? trimmed : null;
      if (next !== null && next.length > 64) return new HttpResponse(null, { status: 400 });
      state.displayNameUpdates.push(next);
      state.me = { ...state.me, display_name: next };
      return HttpResponse.json(state.me);
    }),

    http.post(`${BASE}/login`, async ({ request }) => {
      const body = (await request.json()) as { username: string; password: string };
      if (
        body.username === state.validCredentials.username &&
        body.password === state.validCredentials.password
      ) {
        state.me = DEV_USER;
        return new HttpResponse(null, { status: 200 });
      }
      return new HttpResponse(null, { status: 401 });
    }),

    http.post(`${BASE}/logout`, () => {
      state.me = null;
      return new HttpResponse(null, { status: 200 });
    }),

    http.post(`${BASE}/register`, async ({ request }) => {
      const body = (await request.json()) as { username: string; password: string };
      state.me = { ...DEV_USER, username: body.username };
      return new HttpResponse(null, { status: 200 });
    }),

    http.get(`${BASE}/channels`, () => HttpResponse.json(state.channels)),

    http.get(`${BASE}/emojis`, () => HttpResponse.json(state.customEmojis)),

    http.post(`${BASE}/emojis`, async ({ request }) => {
      if (!state.me) return new HttpResponse(null, { status: 401 });
      const form = await request.formData();
      const rawName = form.get("name");
      const name = typeof rawName === "string" ? rawName : "";
      const file = form.get("file");
      if (!/^[A-Za-z0-9_]{2,32}$/.test(name)) {
        return HttpResponse.json(
          { error: { kind: "invalid_emoji_name", message: "invalid emoji name" } },
          { status: 400 },
        );
      }
      if (!(file instanceof Blob)) {
        return HttpResponse.json(
          { error: { kind: "emoji_file_required", message: "emoji image file is required" } },
          { status: 400 },
        );
      }
      state.uploadedCustomEmojis.push({ name, size: file.size, type: file.type });
      const ts = Math.floor(Date.now() / 1000);
      const animated = file.type === "image/gif" || file.type === "image/webp+animated";
      const extension = animated && file.type === "image/gif" ? "gif" : "webp";
      const created: CustomEmoji = {
        id: Math.floor(Math.random() * 1000) + 500,
        name,
        image_url: `/uploads/emojis/${name}.${extension}?v=${ts}`,
        animated,
        created_by_user_id: state.me.id,
        created_at: ts,
        updated_at: ts,
        deleted_at: null,
      };
      state.customEmojis = [created, ...state.customEmojis];
      return HttpResponse.json(created, { status: 201 });
    }),

    http.patch(`${BASE}/emojis/:id`, async ({ request, params }) => {
      if (!state.me) return new HttpResponse(null, { status: 401 });
      const id = Number(params.id);
      const body = (await request.json()) as { name?: string };
      const name = typeof body.name === "string" ? body.name.trim() : "";
      if (!/^[A-Za-z0-9_]{2,32}$/.test(name)) {
        return HttpResponse.json(
          { error: { kind: "invalid_emoji_name", message: "invalid emoji name" } },
          { status: 400 },
        );
      }
      const existing = state.customEmojis.find((emoji) => emoji.id === id);
      if (!existing) return new HttpResponse(null, { status: 404 });
      if (
        state.customEmojis.some(
          (emoji) =>
            emoji.id !== id &&
            emoji.deleted_at === null &&
            emoji.name.toLocaleLowerCase() === name.toLocaleLowerCase(),
        )
      ) {
        return HttpResponse.json(
          { error: { kind: "emoji_name_taken", message: "custom emoji name already exists" } },
          { status: 409 },
        );
      }
      state.renamedCustomEmojis.push({ id, name });
      const updated: CustomEmoji = {
        ...existing,
        name,
        image_url: existing.image_url.replace(/\?v=\d+$/, `?v=${existing.updated_at + 1}`),
        updated_at: existing.updated_at + 1,
      };
      state.customEmojis = state.customEmojis.map((emoji) => (emoji.id === id ? updated : emoji));
      return HttpResponse.json(updated);
    }),

    http.delete(`${BASE}/emojis/:id`, ({ params }) => {
      if (!state.me) return new HttpResponse(null, { status: 401 });
      const id = Number(params.id);
      const existing = state.customEmojis.find((emoji) => emoji.id === id);
      if (!existing) return new HttpResponse(null, { status: 404 });
      const ts = Math.floor(Date.now() / 1000);
      const deleted: CustomEmoji = { ...existing, updated_at: ts, deleted_at: ts };
      state.customEmojis = state.customEmojis.map((emoji) => (emoji.id === id ? deleted : emoji));
      state.deletedCustomEmojiIds.push(id);
      return HttpResponse.json(deleted);
    }),

    http.post(`${BASE}/emojis/:id/restore`, ({ params }) => {
      if (!state.me) return new HttpResponse(null, { status: 401 });
      const id = Number(params.id);
      const existing = state.customEmojis.find((emoji) => emoji.id === id);
      if (!existing) return new HttpResponse(null, { status: 404 });
      const conflict = state.customEmojis.some(
        (emoji) =>
          emoji.id !== id &&
          emoji.deleted_at === null &&
          emoji.name.toLocaleLowerCase() === existing.name.toLocaleLowerCase(),
      );
      if (conflict) {
        return HttpResponse.json(
          { error: { kind: "emoji_name_taken", message: "custom emoji name already exists" } },
          { status: 409 },
        );
      }
      const restored: CustomEmoji = {
        ...existing,
        updated_at: Math.floor(Date.now() / 1000),
        deleted_at: null,
      };
      state.customEmojis = state.customEmojis.map((emoji) => (emoji.id === id ? restored : emoji));
      state.restoredCustomEmojiIds.push(id);
      return HttpResponse.json(restored);
    }),

    http.get(`${BASE}/messages/:id`, ({ params }) => {
      const id = String(params.id);
      return HttpResponse.json(state.messages[id] ?? []);
    }),

    http.post(`${BASE}/message/:id`, async ({ request, params }) => {
      const channel = String(params.id);
      const channelId = Number(params.id);
      const requestContentType = request.headers.get("content-type")?.toLowerCase() ?? "";
      let text = "";
      let uploadedPhotos: File[] = [];

      if (requestContentType.startsWith("multipart/form-data")) {
        const form = await request.formData();
        const rawText = form.get("text");
        text = typeof rawText === "string" ? rawText : "";
        uploadedPhotos = form
          .getAll("photos")
          .filter((value): value is File => value instanceof File);
        state.sentMessagePhotos.push({
          channel,
          text,
          photos: uploadedPhotos.map((file) => ({
            name: file.name,
            size: file.size,
            type: file.type,
          })),
        });
      } else {
        const body = (await request.json()) as { text: string };
        text = body.text;
      }

      state.sentMessages.push({ channel, text });
      const messageId = Math.floor(Math.random() * 1000) + 1000;
      const message: Message = {
        id: messageId,
        user_id: state.me?.id ?? 1,
        channel_id: channelId,
        parent_id: null,
        text,
        username: state.me?.username ?? "baipas",
        display_name: state.me?.display_name ?? null,
        avatar_url: state.me?.avatar_url ?? null,
        suppress_embeds: false,
        attachments: uploadedPhotos.map((photo, index) =>
          attachmentFromUploadedPhoto(photo, messageId, index),
        ),
        embeds: [],
        reactions: [],
      };
      state.messages[channel] = [...(state.messages[channel] ?? []), message];
      return HttpResponse.json(message);
    }),

    http.get(`${BASE}/threads/participated`, () => {
      const userId = state.me?.id;
      if (userId == null) return new HttpResponse(null, { status: 401 });

      const previews: ParticipatedThreadPreview[] = [];
      for (const [channelId, roots] of Object.entries(state.messages)) {
        const channel = state.channels.find((c) => String(c.id) === channelId);
        if (!channel) continue;
        for (const root of roots.filter((m) => m.parent_id == null)) {
          const replies = [...(state.threadReplies[String(root.id)] ?? [])].sort((a, b) => {
            const aCreatedAt = a.created_at ?? a.id;
            const bCreatedAt = b.created_at ?? b.id;
            if (aCreatedAt !== bCreatedAt) return aCreatedAt - bCreatedAt;
            return a.id - b.id;
          });
          if (replies.length === 0) continue;
          const participated =
            root.user_id === userId || replies.some((reply) => reply.user_id === userId);
          if (!participated) continue;
          const lastReply = replies[replies.length - 1];
          previews.push({
            channel,
            root,
            reply_count: replies.length,
            last_reply_created_at: lastReply.created_at ?? lastReply.id,
            recent_replies: replies.slice(Math.max(replies.length - 3, 0)),
          });
        }
      }

      previews.sort((a, b) => {
        if (a.last_reply_created_at !== b.last_reply_created_at) {
          return b.last_reply_created_at - a.last_reply_created_at;
        }
        return b.root.id - a.root.id;
      });
      return HttpResponse.json(previews);
    }),

    http.get(`${BASE}/thread/:id`, ({ params, request }) => {
      const rootId = Number(params.id);
      const url = new URL(request.url);
      const limitParam = Number(url.searchParams.get("limit") ?? "50");
      const limit = Math.max(1, Math.min(Number.isNaN(limitParam) ? 50 : limitParam, 100));
      const beforeCreatedAt = url.searchParams.get("before_created_at");
      const beforeId = url.searchParams.get("before_id");
      state.threadFetches.push(rootId);
      state.threadRequests.push({
        rootId,
        limit,
        beforeCreatedAt: beforeCreatedAt === null ? null : Number(beforeCreatedAt),
        beforeId: beforeId === null ? null : Number(beforeId),
      });
      for (const list of Object.values(state.messages)) {
        const root = list.find((m) => m.id === rootId);
        if (root) {
          const allReplies = [...(state.threadReplies[String(rootId)] ?? [])].sort((a, b) => {
            const aCreatedAt = a.created_at ?? a.id;
            const bCreatedAt = b.created_at ?? b.id;
            if (aCreatedAt !== bCreatedAt) return aCreatedAt - bCreatedAt;
            return a.id - b.id;
          });
          const eligibleReplies =
            beforeCreatedAt === null || beforeId === null
              ? allReplies
              : allReplies.filter((reply) => {
                  const createdAt = reply.created_at ?? reply.id;
                  const cursorCreatedAt = Number(beforeCreatedAt);
                  const cursorId = Number(beforeId);
                  return (
                    createdAt < cursorCreatedAt ||
                    (createdAt === cursorCreatedAt && reply.id < cursorId)
                  );
                });
          const replies = eligibleReplies.slice(Math.max(eligibleReplies.length - limit, 0));
          const thread: Thread = {
            root,
            replies,
            has_more_replies: eligibleReplies.length > replies.length,
          };
          return HttpResponse.json(thread);
        }
      }
      return new HttpResponse(null, { status: 404 });
    }),

    http.post(`${BASE}/thread/:id/reply`, async ({ request, params }) => {
      const rootId = Number(params.id);
      const requestContentType = request.headers.get("content-type")?.toLowerCase() ?? "";
      let text = "";
      let uploadedPhotos: File[] = [];

      if (requestContentType.startsWith("multipart/form-data")) {
        const form = await request.formData();
        const rawText = form.get("text");
        text = typeof rawText === "string" ? rawText : "";
        uploadedPhotos = form
          .getAll("photos")
          .filter((value): value is File => value instanceof File);
        state.sentThreadReplyPhotos.push({
          rootId,
          text,
          photos: uploadedPhotos.map((file) => ({
            name: file.name,
            size: file.size,
            type: file.type,
          })),
        });
      } else {
        const body = (await request.json()) as { text: string };
        text = body.text;
      }

      let root: Message | undefined;
      for (const list of Object.values(state.messages)) {
        root = list.find((m) => m.id === rootId);
        if (root) break;
      }
      if (!root) return new HttpResponse(null, { status: 404 });
      const replies = state.threadReplies[String(rootId)] ?? [];
      const replyId = Math.floor(Math.random() * 1000) + 1000;
      const reply: Message = {
        id: replyId,
        user_id: state.me?.id ?? 1,
        channel_id: root.channel_id,
        parent_id: rootId,
        text,
        username: state.me?.username ?? "baipas",
        display_name: state.me?.display_name ?? null,
        avatar_url: state.me?.avatar_url ?? null,
        suppress_embeds: false,
        attachments: uploadedPhotos.map((photo, index) =>
          attachmentFromUploadedPhoto(photo, replyId, index),
        ),
        embeds: [],
        reactions: [],
      };
      state.sentThreadReplies.push({ rootId, text });
      state.threadReplies[String(rootId)] = [...replies, reply];
      return HttpResponse.json(reply);
    }),

    http.put(`${BASE}/message/:id`, async ({ request, params }) => {
      const body = (await request.json()) as { text: string };
      const id = Number(params.id);
      state.editedMessages.push({ id, text: body.text });
      let updated: Message | null = null;
      for (const channel of Object.keys(state.messages)) {
        const list = state.messages[channel];
        const idx = list.findIndex((m) => m.id === id);
        if (idx >= 0) {
          updated = { ...list[idx], text: body.text };
          list[idx] = updated;
          break;
        }
      }
      if (!updated) {
        for (const rootId of Object.keys(state.threadReplies)) {
          const list = state.threadReplies[rootId];
          const idx = list.findIndex((m) => m.id === id);
          if (idx >= 0) {
            updated = { ...list[idx], text: body.text };
            list[idx] = updated;
            break;
          }
        }
      }
      if (!updated) return new HttpResponse(null, { status: 404 });
      return HttpResponse.json(updated);
    }),

    http.post(`${BASE}/message/:id/reactions`, async ({ request, params }) => {
      const id = Number(params.id);
      const body = (await request.json()) as import("../../api").ReactionRequest;
      const message = findMessageById(state, id);
      if (!message || message.deleted_at != null) return new HttpResponse(null, { status: 404 });
      const existing = (message.reactions ?? []).find((reaction) =>
        reaction.kind === "native" && body.kind === "native"
          ? reaction.emoji === body.emoji
          : reaction.kind === "custom" && body.kind === "custom"
            ? reaction.emoji_id === body.emoji_id
            : false,
      );
      if (existing) {
        if (existing.kind === "custom" && existing.deleted_at != null && !existing.me_reacted) {
          return new HttpResponse(null, { status: 400 });
        }
        if (!existing.me_reacted) {
          existing.count += 1;
          existing.me_reacted = true;
        }
      } else if (body.kind === "native") {
        message.reactions = [
          ...(message.reactions ?? []),
          { kind: "native", emoji: body.emoji, count: 1, me_reacted: true },
        ];
      } else {
        const customEmoji = state.customEmojis.find((emoji) => emoji.id === body.emoji_id);
        if (!customEmoji || customEmoji.deleted_at !== null) {
          return new HttpResponse(null, { status: 400 });
        }
        message.reactions = [
          ...(message.reactions ?? []),
          {
            kind: "custom",
            emoji_id: body.emoji_id,
            name: customEmoji?.name ?? body.name ?? "custom emoji",
            image_url: customEmoji.image_url,
            animated: customEmoji.animated,
            deleted_at: customEmoji.deleted_at,
            count: 1,
            me_reacted: true,
          },
        ];
      }
      return HttpResponse.json(message.reactions);
    }),

    http.delete(`${BASE}/message/:id/reactions`, async ({ request, params }) => {
      const id = Number(params.id);
      const body = (await request.json()) as import("../../api").ReactionRequest;
      const message = findMessageById(state, id);
      if (!message || message.deleted_at != null) return new HttpResponse(null, { status: 404 });
      message.reactions = (message.reactions ?? [])
        .map((reaction) => {
          const matches =
            reaction.kind === "native" && body.kind === "native"
              ? reaction.emoji === body.emoji
              : reaction.kind === "custom" && body.kind === "custom"
                ? reaction.emoji_id === body.emoji_id
                : false;
          return matches && reaction.me_reacted
            ? { ...reaction, count: Math.max(0, reaction.count - 1), me_reacted: false }
            : reaction;
        })
        .filter((reaction) => reaction.count > 0);
      return HttpResponse.json(message.reactions);
    }),

    http.delete(`${BASE}/message/:id`, ({ params }) => {
      const id = Number(params.id);
      state.deletedMessageIds.push(id);
      for (const channel of Object.keys(state.messages)) {
        const list = state.messages[channel];
        const idx = list.findIndex((m) => m.id === id);
        if (idx >= 0) {
          list.splice(idx, 1);
          return new HttpResponse(null, { status: 204 });
        }
      }
      for (const rootId of Object.keys(state.threadReplies)) {
        const list = state.threadReplies[rootId];
        const idx = list.findIndex((m) => m.id === id);
        if (idx >= 0) {
          list.splice(idx, 1);
          return new HttpResponse(null, { status: 204 });
        }
      }
      return new HttpResponse(null, { status: 404 });
    }),

    http.post(`${BASE}/message/:id/suppress_embeds`, async ({ request, params }) => {
      const id = Number(params.id);
      const body = (await request.json()) as { suppress: boolean };
      state.suppressedEmbeds.push({ id, suppress: body.suppress });
      for (const channel of Object.keys(state.messages)) {
        const list = state.messages[channel];
        const idx = list.findIndex((m) => m.id === id);
        if (idx >= 0) {
          list[idx] = { ...list[idx], suppress_embeds: body.suppress };
          return HttpResponse.json({
            id,
            channel_id: list[idx].channel_id,
            suppress_embeds: body.suppress,
            embeds: list[idx].embeds,
          });
        }
      }
      for (const rootId of Object.keys(state.threadReplies)) {
        const list = state.threadReplies[rootId];
        const idx = list.findIndex((m) => m.id === id);
        if (idx >= 0) {
          list[idx] = { ...list[idx], suppress_embeds: body.suppress };
          return HttpResponse.json({
            id,
            channel_id: list[idx].channel_id,
            suppress_embeds: body.suppress,
            embeds: list[idx].embeds,
          });
        }
      }
      return new HttpResponse(null, { status: 404 });
    }),

    http.post(`${BASE}/channel`, async ({ request }) => {
      const body = (await request.json()) as { name: string; type?: string };
      state.createdChannels.push(body);
      const nextPosition = state.channels.reduce((m, c) => Math.max(m, c.position), -1) + 1;
      const channelType = body.type === "voice" ? "voice" : "text";
      const newChannel: Channel = {
        id: Math.floor(Math.random() * 1000) + 200,
        name: body.name,
        position: nextPosition,
        type: channelType,
      };
      state.channels.push(newChannel);
      return HttpResponse.json(newChannel);
    }),

    http.put(`${BASE}/channels/order`, async ({ request }) => {
      const body = (await request.json()) as { ids: number[] };
      const byId = new Map(state.channels.map((c) => [c.id, c]));
      if (body.ids.length !== state.channels.length || body.ids.some((id) => !byId.has(id))) {
        return new HttpResponse(null, { status: 400 });
      }
      const reordered: Channel[] = body.ids.map((id, i) => {
        const existing = byId.get(id) as Channel;
        return { ...existing, position: i };
      });
      state.channels = reordered;
      state.reorderedIds = body.ids;
      return HttpResponse.json(reordered);
    }),

    http.post(`${BASE}/me/avatar`, async ({ request }) => {
      if (!state.me) return new HttpResponse(null, { status: 401 });
      const form = await request.formData();
      const file = form.get("file");
      if (!(file instanceof Blob)) return new HttpResponse(null, { status: 400 });
      state.uploadedAvatars.push({ size: file.size, type: file.type });
      const ts = Math.floor(Date.now() / 1000);
      state.me = {
        ...state.me,
        avatar_url: `/uploads/avatars/${state.me.id}.webp?v=${ts}`,
      };
      return HttpResponse.json(state.me);
    }),

    http.delete(`${BASE}/me/avatar`, () => {
      if (!state.me) return new HttpResponse(null, { status: 401 });
      state.deletedAvatar = true;
      state.me = { ...state.me, avatar_url: null };
      return HttpResponse.json(state.me);
    }),

    http.get(`${BASE}/voice/participants/:id`, ({ params }) => {
      const id = String(params.id);
      return HttpResponse.json(state.voiceParticipants[id] ?? []);
    }),

    http.get(`${BASE}/voice/screen-shares`, ({ request }) => {
      const url = new URL(request.url);
      const channelId = url.searchParams.get("channel_id");
      const streams =
        channelId == null
          ? state.screenShareStreams
          : state.screenShareStreams.filter((stream) => String(stream.channel_id) === channelId);
      return HttpResponse.json(streams);
    }),

    http.get(`${BASE}/voice/cameras`, ({ request }) => {
      const url = new URL(request.url);
      const channelId = url.searchParams.get("channel_id");
      const streams =
        channelId == null
          ? state.cameraStreams
          : state.cameraStreams.filter((stream) => String(stream.channel_id) === channelId);
      return HttpResponse.json(streams);
    }),

    http.post(`${BASE}/typing/:id`, ({ params }) => {
      state.typingPings.push(String(params.id));
      return new HttpResponse(null, { status: 204 });
    }),

    http.post(`${BASE}/voice/token/:id`, ({ params }) => {
      const id = Number(params.id);
      state.voiceTokensMinted.push(id);
      return HttpResponse.json({
        url: "ws://localhost:7880",
        token: "fake-jwt",
        room: `channel-${id}`,
      });
    }),

    http.post(`${BASE}/voice/status`, async ({ request }) => {
      const body = (await request.json()) as { muted: boolean; deafened: boolean };
      state.voiceStatusUpdates.push(body);
      return new HttpResponse(null, { status: 204 });
    }),
  ];
}
