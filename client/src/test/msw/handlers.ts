import { http, HttpResponse } from "msw";
import type { Channel, Message, User } from "../../api";

const BASE = "http://localhost:3030";

export const DEV_USER: User = {
  id: 1,
  username: "baipas",
  email: null,
  email_verified: false,
  avatar_url: null,
};

export const DEFAULT_CHANNELS: Channel[] = [{ id: 100, name: "general", position: 0 }];

export interface HandlerState {
  me: User | null;
  channels: Channel[];
  messages: Record<string, Message[]>;
  validCredentials: { username: string; password: string };
  sentMessages: { channel: string; text: string }[];
  editedMessages: { id: number; text: string }[];
  deletedMessageIds: number[];
  createdChannels: { name: string }[];
  reorderedIds: number[] | null;
  uploadedAvatars: { size: number; type: string }[];
  deletedAvatar: boolean;
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
    ...overrides,
  };
}

export function createHandlers(state: HandlerState) {
  return [
    http.get(`${BASE}/me`, () => {
      if (!state.me) return new HttpResponse(null, { status: 401 });
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

    http.post(`${BASE}/channel`, async ({ request }) => {
      const body = (await request.json()) as { name: string };
      state.createdChannels.push(body);
      const nextPosition = state.channels.reduce((m, c) => Math.max(m, c.position), -1) + 1;
      const newChannel: Channel = {
        id: Math.floor(Math.random() * 1000) + 200,
        name: body.name,
        position: nextPosition,
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
  ];
}
