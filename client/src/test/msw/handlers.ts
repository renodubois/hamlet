import { http, HttpResponse } from "msw";
import type { Channel, Message, User } from "../../api";

const BASE = "http://localhost:3030";

export const DEV_USER: User = {
  id: 1,
  username: "baipas",
  email: null,
  email_verified: false,
};

export const DEFAULT_CHANNELS: Channel[] = [{ id: 100, name: "general" }];

export interface HandlerState {
  me: User | null;
  channels: Channel[];
  messages: Record<string, Message[]>;
  validCredentials: { username: string; password: string };
  sentMessages: { channel: string; text: string }[];
  createdChannels: { name: string }[];
}

export function createState(overrides: Partial<HandlerState> = {}): HandlerState {
  return {
    me: null,
    channels: [...DEFAULT_CHANNELS],
    messages: { "100": [] },
    validCredentials: { username: "baipas", password: "password" },
    sentMessages: [],
    createdChannels: [],
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

    http.post(`${BASE}/channel`, async ({ request }) => {
      const body = (await request.json()) as { name: string };
      state.createdChannels.push(body);
      const newChannel: Channel = { id: Math.floor(Math.random() * 1000) + 200, name: body.name };
      state.channels.push(newChannel);
      return HttpResponse.json(newChannel);
    }),
  ];
}
