import { http, HttpResponse } from "msw";
import type { Channel, CustomEmoji, Message, User, VoiceParticipant } from "../../api";

const BASE = "http://127.0.0.1:3030";

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
  editedMessages: { id: number; text: string }[];
  deletedMessageIds: number[];
  createdChannels: { name: string; type?: string }[];
  reorderedIds: number[] | null;
  uploadedAvatars: { size: number; type: string }[];
  deletedAvatar: boolean;
  displayNameUpdates: (string | null)[];
  voiceParticipants: Record<string, VoiceParticipant[]>;
  voiceTokensMinted: number[];
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
    editedMessages: [],
    deletedMessageIds: [],
    createdChannels: [],
    reorderedIds: null,
    uploadedAvatars: [],
    deletedAvatar: false,
    displayNameUpdates: [],
    voiceParticipants: {},
    voiceTokensMinted: [],
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
      const body = (await request.json()) as { text: string };
      state.sentMessages.push({ channel: String(params.id), text: body.text });
      return new HttpResponse(null, { status: 200 });
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
      if (!updated) return new HttpResponse(null, { status: 404 });
      return HttpResponse.json(updated);
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
  ];
}
