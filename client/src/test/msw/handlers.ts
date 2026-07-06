import { http, HttpResponse } from "msw";
import type {
  CameraStream,
  Channel,
  CustomEmoji,
  Message,
  MessageAttachment,
  ParticipatedThreadPreview,
  PublicUser,
  ReadStateSummary,
  ScreenShareStream,
  Thread,
  User,
  VoiceParticipant,
} from "../../api";

const BASE = import.meta.env.VITE_HAMLET_DEFAULT_SERVER_URL ?? "http://127.0.0.1:3030";
const MAX_SAFE_MESSAGE_ID = Number.MAX_SAFE_INTEGER;
const CSRF_COOKIE = "hamlet_csrf";
const CSRF_TEST_TOKEN = "msw-csrf-token";

export const DEV_USER: User = {
  id: 1,
  username: "baipas",
  display_name: null,
  email: null,
  email_verified: false,
  avatar_url: null,
};

const USER_SEARCH_DEFAULT_LIMIT = 20;
const USER_SEARCH_MAX_LIMIT = 50;
const USER_SEARCH_MIN_LIMIT = 1;

type DirectoryUser = PublicUser & {
  email?: string | null;
  email_verified?: boolean;
  avatar_path?: string | null;
  credentials?: unknown;
  sessions?: unknown;
};

function publicUserFrom(
  user: Pick<PublicUser, "id" | "username" | "display_name" | "avatar_url">,
): PublicUser {
  return {
    id: user.id,
    username: user.username,
    display_name: user.display_name,
    avatar_url: user.avatar_url,
  };
}

export const DEV_PUBLIC_USER: PublicUser = publicUserFrom(DEV_USER);

export const DEFAULT_CHANNELS: Channel[] = [
  { id: 100, name: "general", position: 0, type: "text" },
];

export interface HandlerState {
  me: User | null;
  users: DirectoryUser[];
  userSearchRequests: { query: string; limit: number }[];
  channels: Channel[];
  messages: Record<string, Message[]>;
  validCredentials: { username: string; password: string };
  accountRegistrationEnabled: boolean;
  sentMessages: { channel: string; text: string }[];
  sentInlineReplies: { channel: string; text: string; replyToMessageId: number }[];
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
  passwordChanges: { currentPassword: string; newPassword: string }[];
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
  readStates: ReadStateSummary[];
  markReadRequests: { channelId: number; lastVisibleMessageId: number }[];
}

export function createState(overrides: Partial<HandlerState> = {}): HandlerState {
  const state: HandlerState = {
    me: null,
    users: [DEV_PUBLIC_USER],
    userSearchRequests: [],
    channels: [...DEFAULT_CHANNELS],
    messages: { "100": [] },
    validCredentials: { username: "baipas", password: "password" },
    accountRegistrationEnabled: true,
    sentMessages: [],
    sentInlineReplies: [],
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
    passwordChanges: [],
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
    readStates: [],
    markReadRequests: [],
    ...overrides,
  };
  if (state.readStates.length === 0) {
    state.readStates = state.channels
      .filter((channel) => channel.type === "text")
      .map((channel) => ({
        channel_id: channel.id,
        has_unread: false,
        mention_count: 0,
        last_read_created_at: 0,
        last_read_message_id: 0,
        updated_at: 0,
      }));
  }
  if (state.me) upsertDirectoryUser(state, state.me);
  return state;
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

function withReplyMetadataDefaults(message: Message): Message {
  return {
    ...message,
    mentions: message.mentions ?? [],
    reply_to_message_id: message.reply_to_message_id ?? null,
    reply_to: message.reply_to
      ? {
          ...message.reply_to,
          deleted_at: message.reply_to.deleted_at ?? null,
          attachment_count: message.reply_to.attachment_count ?? 0,
        }
      : null,
  };
}

function withReplyMetadataDefaultsList(messages: Message[]): Message[] {
  return messages.map((message) => withReplyMetadataDefaults(message));
}

type UserSearchRank = "exact" | "prefix" | "substring" | "fuzzy" | "empty";
type UserSearchFieldRank = "username" | "display_name" | "empty";

const USER_SEARCH_RANK_ORDER: Record<UserSearchRank, number> = {
  exact: 0,
  prefix: 1,
  substring: 2,
  fuzzy: 3,
  empty: 4,
};

const USER_SEARCH_FIELD_ORDER: Record<UserSearchFieldRank, number> = {
  username: 0,
  display_name: 1,
  empty: 2,
};

function boundedUserSearchLimit(rawLimit: string | null): number {
  const parsed = rawLimit === null ? USER_SEARCH_DEFAULT_LIMIT : Number(rawLimit);
  const fallback = Number.isFinite(parsed) ? Math.trunc(parsed) : USER_SEARCH_DEFAULT_LIMIT;
  return Math.max(USER_SEARCH_MIN_LIMIT, Math.min(fallback, USER_SEARCH_MAX_LIMIT));
}

function fuzzyMatch(candidate: string, query: string): boolean {
  let queryIndex = 0;
  for (const char of candidate) {
    if (char === query[queryIndex]) queryIndex += 1;
    if (queryIndex >= query.length) return true;
  }
  return query.length === 0;
}

function scoreText(candidate: string, query: string): UserSearchRank | null {
  if (candidate === query) return "exact";
  if (candidate.startsWith(query)) return "prefix";
  if (candidate.includes(query)) return "substring";
  if (fuzzyMatch(candidate, query)) return "fuzzy";
  return null;
}

function scoreUser(
  user: DirectoryUser,
  query: string,
): { rank: UserSearchRank; field: UserSearchFieldRank } | null {
  if (query.length === 0) return { rank: "empty", field: "empty" };

  const candidates: { rank: UserSearchRank; field: UserSearchFieldRank }[] = [];
  const usernameRank = scoreText(user.username.toLowerCase(), query);
  if (usernameRank) candidates.push({ rank: usernameRank, field: "username" });
  if (user.display_name) {
    const displayNameRank = scoreText(user.display_name.toLowerCase(), query);
    if (displayNameRank) candidates.push({ rank: displayNameRank, field: "display_name" });
  }
  candidates.sort((a, b) => {
    const rankDelta = USER_SEARCH_RANK_ORDER[a.rank] - USER_SEARCH_RANK_ORDER[b.rank];
    if (rankDelta !== 0) return rankDelta;
    return USER_SEARCH_FIELD_ORDER[a.field] - USER_SEARCH_FIELD_ORDER[b.field];
  });
  return candidates[0] ?? null;
}

function searchPublicUsers(users: DirectoryUser[], rawQuery: string, limit: number): PublicUser[] {
  const query = rawQuery.trim().toLowerCase();
  return users
    .map((user) => ({ user, score: scoreUser(user, query) }))
    .filter(
      (
        entry,
      ): entry is {
        user: DirectoryUser;
        score: { rank: UserSearchRank; field: UserSearchFieldRank };
      } => entry.score !== null,
    )
    .sort((a, b) => {
      const rankDelta = USER_SEARCH_RANK_ORDER[a.score.rank] - USER_SEARCH_RANK_ORDER[b.score.rank];
      if (rankDelta !== 0) return rankDelta;
      const fieldDelta =
        USER_SEARCH_FIELD_ORDER[a.score.field] - USER_SEARCH_FIELD_ORDER[b.score.field];
      if (fieldDelta !== 0) return fieldDelta;
      const usernameDelta = a.user.username
        .toLowerCase()
        .localeCompare(b.user.username.toLowerCase());
      if (usernameDelta !== 0) return usernameDelta;
      return a.user.id - b.user.id;
    })
    .slice(0, limit)
    .map((entry) => publicUserFrom(entry.user));
}

function upsertDirectoryUser(currentState: HandlerState, user: PublicUser): void {
  const next = publicUserFrom(user);
  const index = currentState.users.findIndex((existing) => existing.id === next.id);
  if (index === -1) currentState.users = [...currentState.users, next];
  else
    currentState.users = currentState.users.map((existing, i) =>
      i === index ? { ...existing, ...next } : existing,
    );
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

function errorJson(kind: string, message: string, status: number): Response {
  return HttpResponse.json({ error: { kind, message } }, { status });
}

type ReplyTargetResult =
  | { ok: true; replyToMessageId: number | null }
  | { ok: false; response: Response };

function parseJsonReplyTarget(value: unknown): ReplyTargetResult {
  if (value == null) return { ok: true, replyToMessageId: null };
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value <= 0) {
    return {
      ok: false,
      response: errorJson(
        Number.isFinite(value) ? "reply_target_unsafe" : "invalid_request",
        "reply target id must be a safe positive integer",
        400,
      ),
    };
  }
  return { ok: true, replyToMessageId: value };
}

function parseFormReplyTarget(value: FormDataEntryValue | null): ReplyTargetResult {
  if (value === null) return { ok: true, replyToMessageId: null };
  if (typeof value !== "string") {
    return { ok: false, response: errorJson("invalid_request", "invalid request", 400) };
  }
  const trimmed = value.trim();
  if (trimmed.length === 0 || trimmed.toLowerCase() === "null") {
    return { ok: true, replyToMessageId: null };
  }
  if (/^-\d+$/.test(trimmed)) {
    return {
      ok: false,
      response: errorJson(
        "reply_target_unsafe",
        "reply target id must be a safe positive integer",
        400,
      ),
    };
  }
  if (!/^\d+$/.test(trimmed)) {
    return { ok: false, response: errorJson("invalid_request", "invalid request", 400) };
  }
  const parsed = Number(trimmed);
  if (!Number.isSafeInteger(parsed) || parsed <= 0 || parsed > MAX_SAFE_MESSAGE_ID) {
    return {
      ok: false,
      response: errorJson(
        "reply_target_unsafe",
        "reply target id must be a safe positive integer",
        400,
      ),
    };
  }
  return { ok: true, replyToMessageId: parsed };
}

function validateInlineReplyTarget(
  currentState: HandlerState,
  channelId: number,
  replyToMessageId: number | null,
): Response | null {
  if (replyToMessageId === null) return null;
  const target = findMessageById(currentState, replyToMessageId);
  if (!target) {
    return errorJson("reply_target_not_found", "reply target message was not found", 404);
  }
  if (target.channel_id !== channelId) {
    return errorJson("reply_target_cross_channel", "reply target must be in the same channel", 400);
  }
  if (target.parent_id != null) {
    return errorJson(
      "reply_target_not_top_level",
      "reply target must be a top-level channel message",
      400,
    );
  }
  if (target.deleted_at != null) {
    return errorJson("reply_target_deleted", "reply target message was deleted", 400);
  }
  return null;
}

function threadInlineReplyTargetError(): Response {
  return errorJson(
    "thread_inline_reply_not_allowed",
    "thread replies cannot include an inline reply target",
    400,
  );
}

const MAX_UNIQUE_MENTIONS_PER_MESSAGE = 50;

type MentionHydrationResult =
  | { ok: true; mentions: PublicUser[] }
  | { ok: false; response: Response };

function hydrateMentionsFromText(currentState: HandlerState, text: string): MentionHydrationResult {
  const mentionRe = /<@(\d+)>/g;
  const ids: number[] = [];
  const seen = new Set<number>();
  let match: RegExpExecArray | null = mentionRe.exec(text);
  while (match !== null) {
    const id = Number(match[1]);
    if (!Number.isSafeInteger(id) || id <= 0 || id > Number.MAX_SAFE_INTEGER) {
      return { ok: false, response: errorJson("invalid_request", "invalid request", 400) };
    }
    if (!seen.has(id)) {
      seen.add(id);
      ids.push(id);
    }
    match = mentionRe.exec(text);
  }

  if (ids.length > MAX_UNIQUE_MENTIONS_PER_MESSAGE) {
    return { ok: false, response: errorJson("invalid_request", "invalid request", 400) };
  }

  const usersById = new Map(currentState.users.map((user) => [user.id, publicUserFrom(user)]));
  const mentions: PublicUser[] = [];
  for (const id of ids) {
    const user = usersById.get(id);
    if (!user) return { ok: false, response: errorJson("invalid_request", "invalid request", 400) };
    mentions.push(user);
  }
  return { ok: true, mentions };
}

export function createHandlers(state: HandlerState) {
  return [
    http.get(`${BASE}/config`, () =>
      HttpResponse.json({
        account_registration_enabled: state.accountRegistrationEnabled,
      }),
    ),

    http.get(`${BASE}/csrf`, () => {
      if (!state.me) return new HttpResponse(null, { status: 401 });
      return HttpResponse.json(
        { token: CSRF_TEST_TOKEN },
        {
          headers: {
            "Set-Cookie": `${CSRF_COOKIE}=${CSRF_TEST_TOKEN}; Path=/; SameSite=Lax`,
          },
        },
      );
    }),

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
      upsertDirectoryUser(state, state.me);
      return HttpResponse.json(state.me);
    }),

    http.put(`${BASE}/me/password`, async ({ request }) => {
      if (!state.me) return new HttpResponse(null, { status: 401 });
      const body = (await request.json()) as {
        current_password?: string;
        new_password?: string;
      };
      if (!body.current_password || !body.new_password) {
        return errorJson("invalid_request", "invalid request", 400);
      }
      if (body.current_password !== state.validCredentials.password) {
        return errorJson("invalid_credentials", "invalid credentials", 401);
      }
      state.passwordChanges.push({
        currentPassword: body.current_password,
        newPassword: body.new_password,
      });
      state.validCredentials = { ...state.validCredentials, password: body.new_password };
      return new HttpResponse(null, { status: 204 });
    }),

    http.get(`${BASE}/users`, ({ request }) => {
      if (!state.me) return new HttpResponse(null, { status: 401 });
      const url = new URL(request.url);
      const query = url.searchParams.get("q") ?? url.searchParams.get("query") ?? "";
      const limit = boundedUserSearchLimit(url.searchParams.get("limit"));
      state.userSearchRequests.push({ query, limit });
      return HttpResponse.json(searchPublicUsers(state.users, query, limit));
    }),

    http.post(`${BASE}/login`, async ({ request }) => {
      const body = (await request.json()) as { username: string; password: string };
      if (
        body.username === state.validCredentials.username &&
        body.password === state.validCredentials.password
      ) {
        state.me = DEV_USER;
        upsertDirectoryUser(state, DEV_USER);
        return new HttpResponse(null, { status: 200 });
      }
      return new HttpResponse(null, { status: 401 });
    }),

    http.post(`${BASE}/logout`, () => {
      state.me = null;
      return new HttpResponse(null, { status: 200 });
    }),

    http.post(`${BASE}/register`, async ({ request }) => {
      if (!state.accountRegistrationEnabled) {
        return errorJson("registration_disabled", "account registration is disabled", 403);
      }
      const body = (await request.json()) as { username: string; password: string };
      state.me = { ...DEV_USER, username: body.username };
      state.validCredentials = { username: body.username, password: body.password };
      upsertDirectoryUser(state, state.me);
      return new HttpResponse(null, { status: 200 });
    }),

    http.get(`${BASE}/channels`, () => HttpResponse.json(state.channels)),

    http.get(`${BASE}/read-states`, () => {
      if (!state.me) return new HttpResponse(null, { status: 401 });
      return HttpResponse.json(state.readStates);
    }),

    http.put(`${BASE}/channels/:id/read-state`, async ({ request, params }) => {
      if (!state.me) return new HttpResponse(null, { status: 401 });
      const channelId = Number(params.id);
      const body = (await request.json()) as { last_visible_message_id: number };
      state.markReadRequests.push({
        channelId,
        lastVisibleMessageId: body.last_visible_message_id,
      });
      const message = findMessageById(state, body.last_visible_message_id);
      const summary: ReadStateSummary = {
        channel_id: channelId,
        has_unread: false,
        mention_count: 0,
        last_read_created_at: message?.created_at ?? body.last_visible_message_id,
        last_read_message_id: body.last_visible_message_id,
        updated_at: Date.now(),
      };
      const existingIndex = state.readStates.findIndex(
        (readState) => readState.channel_id === channelId,
      );
      if (existingIndex >= 0) state.readStates[existingIndex] = summary;
      else state.readStates.push(summary);
      return HttpResponse.json(summary);
    }),

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
      return HttpResponse.json(withReplyMetadataDefaultsList(state.messages[id] ?? []));
    }),

    http.post(`${BASE}/message/:id`, async ({ request, params }) => {
      if (!state.me) return new HttpResponse(null, { status: 401 });
      const channel = String(params.id);
      const channelId = Number(params.id);
      const requestContentType = request.headers.get("content-type")?.toLowerCase() ?? "";
      let text = "";
      let uploadedPhotos: File[] = [];
      let replyToMessageId: number | null = null;

      if (requestContentType.startsWith("multipart/form-data")) {
        const form = await request.formData();
        const rawText = form.get("text");
        const parsedTarget = parseFormReplyTarget(form.get("reply_to_message_id"));
        if (!parsedTarget.ok) return parsedTarget.response;
        text = typeof rawText === "string" ? rawText : "";
        replyToMessageId = parsedTarget.replyToMessageId;
        uploadedPhotos = form
          .getAll("photos")
          .filter((value): value is File => value instanceof File);
      } else {
        const body = (await request.json()) as { text: string; reply_to_message_id?: unknown };
        const parsedTarget = parseJsonReplyTarget(body.reply_to_message_id);
        if (!parsedTarget.ok) return parsedTarget.response;
        text = body.text;
        replyToMessageId = parsedTarget.replyToMessageId;
      }

      const validationError = validateInlineReplyTarget(state, channelId, replyToMessageId);
      if (validationError) return validationError;
      const mentionHydration = hydrateMentionsFromText(state, text);
      if (!mentionHydration.ok) return mentionHydration.response;

      state.sentMessages.push({ channel, text });
      if (replyToMessageId !== null) {
        state.sentInlineReplies.push({ channel, text, replyToMessageId });
      }
      if (uploadedPhotos.length > 0) {
        state.sentMessagePhotos.push({
          channel,
          text,
          photos: uploadedPhotos.map((file) => ({
            name: file.name,
            size: file.size,
            type: file.type,
          })),
        });
      }
      const replyTo =
        replyToMessageId === null ? undefined : findMessageById(state, replyToMessageId);
      const replyReference = replyTo
        ? {
            id: replyTo.id,
            user_id: replyTo.user_id,
            channel_id: replyTo.channel_id,
            created_at: replyTo.created_at ?? replyTo.id,
            ...(replyTo.deleted_at != null ? { deleted_at: replyTo.deleted_at } : {}),
            text: replyTo.text,
            ...(replyTo.attachments.length > 0
              ? { attachment_count: replyTo.attachments.length }
              : {}),
            username: replyTo.username,
            display_name: replyTo.display_name,
            avatar_url: replyTo.avatar_url,
          }
        : undefined;
      const messageId = Math.floor(Math.random() * 1000) + 1000;
      const message: Message = {
        id: messageId,
        user_id: state.me?.id ?? 1,
        channel_id: channelId,
        parent_id: null,
        reply_to_message_id: replyToMessageId,
        reply_to: replyReference ?? null,
        text,
        username: state.me?.username ?? "baipas",
        display_name: state.me?.display_name ?? null,
        avatar_url: state.me?.avatar_url ?? null,
        suppress_embeds: false,
        mentions: mentionHydration.mentions,
        attachments: uploadedPhotos.map((photo, index) =>
          attachmentFromUploadedPhoto(photo, messageId, index),
        ),
        embeds: [],
        reactions: [],
      };
      state.messages[channel] = [...(state.messages[channel] ?? []), message];
      return HttpResponse.json(withReplyMetadataDefaults(message));
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
            root: withReplyMetadataDefaults(root),
            reply_count: replies.length,
            last_reply_created_at: lastReply.created_at ?? lastReply.id,
            recent_replies: withReplyMetadataDefaultsList(
              replies.slice(Math.max(replies.length - 3, 0)),
            ),
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
            root: withReplyMetadataDefaults(root),
            replies: withReplyMetadataDefaultsList(replies),
            has_more_replies: eligibleReplies.length > replies.length,
          };
          return HttpResponse.json(thread);
        }
      }
      return new HttpResponse(null, { status: 404 });
    }),

    http.post(`${BASE}/thread/:id/reply`, async ({ request, params }) => {
      if (!state.me) return new HttpResponse(null, { status: 401 });
      const rootId = Number(params.id);
      const requestContentType = request.headers.get("content-type")?.toLowerCase() ?? "";
      let text = "";
      let uploadedPhotos: File[] = [];

      if (requestContentType.startsWith("multipart/form-data")) {
        const form = await request.formData();
        const parsedTarget = parseFormReplyTarget(form.get("reply_to_message_id"));
        if (!parsedTarget.ok) return parsedTarget.response;
        if (parsedTarget.replyToMessageId !== null) return threadInlineReplyTargetError();
        const rawText = form.get("text");
        text = typeof rawText === "string" ? rawText : "";
        uploadedPhotos = form
          .getAll("photos")
          .filter((value): value is File => value instanceof File);
      } else {
        const body = (await request.json()) as { text: string; reply_to_message_id?: unknown };
        const parsedTarget = parseJsonReplyTarget(body.reply_to_message_id);
        if (!parsedTarget.ok) return parsedTarget.response;
        if (parsedTarget.replyToMessageId !== null) return threadInlineReplyTargetError();
        text = body.text;
      }

      const mentionHydration = hydrateMentionsFromText(state, text);
      if (!mentionHydration.ok) return mentionHydration.response;

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
        reply_to_message_id: null,
        reply_to: null,
        text,
        username: state.me?.username ?? "baipas",
        display_name: state.me?.display_name ?? null,
        avatar_url: state.me?.avatar_url ?? null,
        suppress_embeds: false,
        mentions: mentionHydration.mentions,
        attachments: uploadedPhotos.map((photo, index) =>
          attachmentFromUploadedPhoto(photo, replyId, index),
        ),
        embeds: [],
        reactions: [],
      };
      state.sentThreadReplies.push({ rootId, text });
      if (uploadedPhotos.length > 0) {
        state.sentThreadReplyPhotos.push({
          rootId,
          text,
          photos: uploadedPhotos.map((file) => ({
            name: file.name,
            size: file.size,
            type: file.type,
          })),
        });
      }
      state.threadReplies[String(rootId)] = [...replies, reply];
      return HttpResponse.json(withReplyMetadataDefaults(reply));
    }),

    http.put(`${BASE}/message/:id`, async ({ request, params }) => {
      const body = (await request.json()) as { text: string };
      const id = Number(params.id);
      let existing: Message | null = null;
      let replaceMessage: ((message: Message) => void) | null = null;
      for (const channel of Object.keys(state.messages)) {
        const list = state.messages[channel];
        const idx = list.findIndex((m) => m.id === id);
        if (idx >= 0) {
          existing = list[idx];
          replaceMessage = (message) => {
            list[idx] = message;
          };
          break;
        }
      }
      if (!existing) {
        for (const rootId of Object.keys(state.threadReplies)) {
          const list = state.threadReplies[rootId];
          const idx = list.findIndex((m) => m.id === id);
          if (idx >= 0) {
            existing = list[idx];
            replaceMessage = (message) => {
              list[idx] = message;
            };
            break;
          }
        }
      }
      if (!existing || !replaceMessage) return new HttpResponse(null, { status: 404 });

      const mentionHydration = hydrateMentionsFromText(state, body.text);
      if (!mentionHydration.ok) return mentionHydration.response;

      state.editedMessages.push({ id, text: body.text });
      const updated = { ...existing, text: body.text, mentions: mentionHydration.mentions };
      replaceMessage(updated);
      return HttpResponse.json(withReplyMetadataDefaults(updated));
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
      upsertDirectoryUser(state, state.me);
      return HttpResponse.json(state.me);
    }),

    http.delete(`${BASE}/me/avatar`, () => {
      if (!state.me) return new HttpResponse(null, { status: 401 });
      state.deletedAvatar = true;
      state.me = { ...state.me, avatar_url: null };
      upsertDirectoryUser(state, state.me);
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
