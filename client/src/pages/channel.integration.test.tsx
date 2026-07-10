import { describe, expect, test, vi } from "vitest";
import { act, render, screen, fireEvent, waitFor, within } from "../test/testing-library";
import userEvent from "@testing-library/user-event";
import * as ReactRouter from "react-router-dom";

const makeRouter = (ReactRouter as any)["create" + "MemoryRouter"];
import { http, HttpResponse } from "msw";
import { AuthProvider } from "../contexts/auth";
import { ChannelsProvider } from "../contexts/channels";
import { CustomEmojisProvider } from "../contexts/custom-emojis";
import { EventsProvider } from "../contexts/events";
import { ReadStatesProvider } from "../contexts/read-states";
import { FakeEventSource, latestFakeEventSource } from "../test/msw/sse";
import { mswState, resetMswState, server } from "../test/msw/server";
import { DEV_USER } from "../test/msw/handlers";
import { expectNoA11yViolations } from "../test/a11y";
import { makeAttachment } from "../test/fixtures";
import { tinyJpegFile, tinyPngFile, tinyWebpFile } from "../test/image-fixtures";
import { assertExists } from "../test/render";
import ChannelView from "./channel";
import type { Message, PublicUser } from "../api";

const TEST_SERVER = import.meta.env.VITE_HAMLET_DEFAULT_SERVER_URL ?? "http://127.0.0.1:3030";

vi.mock("../api", async () => {
  const actual = await vi.importActual<typeof import("../api")>("../api");
  return {
    ...actual,
    messagesEventSource: () => new FakeEventSource("/mock/messages") as unknown as EventSource,
  };
});

function mountAt(path: string) {
  const router = makeRouter([{ path: "/channel/:id", element: <ChannelView /> }], {
    initialEntries: [path],
  });
  const history = {
    get: () => `${router.state.location.pathname}${router.state.location.search}`,
    set: ({ value }: { value: string }) => void router.navigate(value),
    back: () => void router.navigate(-1),
    forward: () => void router.navigate(1),
  };

  const result = render(() => (
    <AuthProvider>
      <EventsProvider>
        <CustomEmojisProvider>
          <ChannelsProvider>
            <ReadStatesProvider>
              <ReactRouter.RouterProvider router={router} />
            </ReadStatesProvider>
          </ChannelsProvider>
        </CustomEmojisProvider>
      </EventsProvider>
    </AuthProvider>
  ));

  return { ...result, history };
}

function seedAuthed() {
  const state = resetMswState();
  state.me = { ...DEV_USER, username: "alice", display_name: null, avatar_url: null };
  state.messages["100"] = [
    {
      id: 1,
      user_id: 1,
      channel_id: 100,
      text: "hello",
      username: "alice",
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [],
      attachments: [],
      embeds: [],
    },
    {
      id: 2,
      user_id: 2,
      channel_id: 100,
      text: "world",
      username: "bob",
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [],
      attachments: [],
      embeds: [],
    },
  ];
  return state;
}

function setInputSelection(input: HTMLInputElement, start: number, end = start) {
  act(() => {
    input.focus();
    input.setSelectionRange(start, end);
    fireEvent.select(input);
  });
}

function inputFromUser(input: HTMLInputElement, value: string, caretIndex = value.length) {
  act(() => {
    input.value = value;
    input.setSelectionRange(caretIndex, caretIndex);
    fireEvent.input(input);
  });
}

function deferred<T = void>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

function mockObjectUrls() {
  const originalCreateObjectURL = Object.getOwnPropertyDescriptor(URL, "createObjectURL");
  const originalRevokeObjectURL = Object.getOwnPropertyDescriptor(URL, "revokeObjectURL");
  let nextUrl = 0;
  const createObjectURL = vi.fn(
    (file: Blob | MediaSource) => `blob:${(file as File).name ?? "photo"}-${nextUrl++}`,
  );
  const revokeObjectURL = vi.fn();

  Object.defineProperty(URL, "createObjectURL", { configurable: true, value: createObjectURL });
  Object.defineProperty(URL, "revokeObjectURL", { configurable: true, value: revokeObjectURL });

  return {
    createObjectURL,
    revokeObjectURL,
    restore() {
      if (originalCreateObjectURL) {
        Object.defineProperty(URL, "createObjectURL", originalCreateObjectURL);
      } else {
        delete (URL as unknown as { createObjectURL?: unknown }).createObjectURL;
      }
      if (originalRevokeObjectURL) {
        Object.defineProperty(URL, "revokeObjectURL", originalRevokeObjectURL);
      } else {
        delete (URL as unknown as { revokeObjectURL?: unknown }).revokeObjectURL;
      }
    },
  };
}

function fileInput(): HTMLInputElement {
  return assertExists(
    document.querySelector('input[type="file"][aria-label="Photo files"]'),
    "photo file input",
  ) as HTMLInputElement;
}

function fileInputWithin(container: HTMLElement): HTMLInputElement {
  return within(container).getByLabelText(/photo files/i) as HTMLInputElement;
}

function photoFile(name: string, type = "image/png") {
  if (type === "image/webp") return tinyWebpFile(name);
  if (type === "image/jpeg" || type === "image/jpg") return tinyJpegFile(name);
  return tinyPngFile(name);
}

function messagesRegion(): HTMLDivElement {
  return screen.getByRole("region", { name: /messages/i }) as HTMLDivElement;
}

function setScrollMetrics(
  element: HTMLDivElement,
  metrics: { scrollHeight: number; clientHeight: number; scrollTop: number },
) {
  let currentScrollTop = metrics.scrollTop;
  Object.defineProperties(element, {
    scrollHeight: { configurable: true, get: () => metrics.scrollHeight },
    clientHeight: { configurable: true, get: () => metrics.clientHeight },
    scrollTop: {
      configurable: true,
      get: () => currentScrollTop,
      set: (value: number) => {
        currentScrollTop = value;
      },
    },
  });
  return {
    get scrollTop() {
      return currentScrollTop;
    },
  };
}

function findRenderedMessageText(text: string) {
  return screen.findByText(
    (_, element) =>
      element?.textContent === text && element.className.includes("whitespace-pre-wrap"),
  );
}

function findRenderedMessageTextWithin(container: HTMLElement, text: string) {
  return within(container).findByText(
    (_, element) =>
      element?.textContent === text && element.className.includes("whitespace-pre-wrap"),
  );
}

function queryRenderedMessageTextWithin(container: HTMLElement, text: string) {
  return within(container).queryByText(
    (_, element) =>
      element?.textContent === text && element.className.includes("whitespace-pre-wrap"),
  );
}

function seedOwnMessage(overrides: Partial<Message> = {}) {
  const state = resetMswState();
  state.me = { ...DEV_USER, username: "alice", display_name: null, avatar_url: null };
  state.messages["100"] = [
    {
      id: 7,
      user_id: DEV_USER.id,
      channel_id: 100,
      text: "original",
      username: DEV_USER.username,
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [],
      attachments: [],
      embeds: [],
      ...overrides,
    },
  ];
  return state;
}

function seedThreadWithOwnReply(overrides: Partial<Message> = {}) {
  const state = seedAuthed();
  state.threadReplies["1"] = [
    {
      id: 70,
      user_id: DEV_USER.id,
      channel_id: 100,
      parent_id: 1,
      created_at: 1_700_000_010_000_000,
      text: "own reply",
      username: DEV_USER.username,
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [],
      attachments: [],
      embeds: [],
      ...overrides,
    },
  ];
  return state;
}

async function openThreadReplyEdit(panel: HTMLElement) {
  fireEvent.click(within(panel).getByRole("button", { name: /edit reply/i }));
  return (await within(panel).findByRole("textbox", { name: /edit reply/i })) as HTMLInputElement;
}

async function openMessageEdit(messageText: string) {
  const original = await screen.findByText(messageText);
  fireEvent.contextMenu(original);

  const editItem = await screen.findByRole("menuitem", { name: /edit message/i });
  fireEvent.click(editItem);

  return (await screen.findByLabelText(/edit message/i)) as HTMLInputElement;
}

describe("Channel view integration", () => {
  test("loads initial messages from the server", async () => {
    seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => {
      expect(screen.getByText("hello")).toBeInTheDocument();
      expect(screen.getByText("world")).toBeInTheDocument();
    });
  });

  test("bottom-anchors short message lists and scrolls initial load to latest", async () => {
    seedAuthed();
    mountAt("/channel/100");
    const scroll = setScrollMetrics(messagesRegion(), {
      scrollHeight: 1000,
      clientHeight: 100,
      scrollTop: 0,
    });

    await waitFor(() => expect(screen.getByText("world")).toBeInTheDocument());

    expect(messagesRegion()).toHaveClass("flex", "flex-col", "overscroll-y-none");
    const messagesSection = assertExists(
      messagesRegion().querySelector("section"),
      "messages section",
    );
    expect(messagesSection).toHaveClass("min-h-full", "flex-1", "flex-col");
    expect(messagesSection).not.toHaveClass("justify-end");
    expect(assertExists(messagesSection.firstElementChild, "bottom anchor spacer")).toHaveClass(
      "mt-auto",
    );
    await waitFor(() => expect(scroll.scrollTop).toBe(1000));
  });

  test("marks the focused near-bottom active channel read with the last visible message", async () => {
    const focusSpy = vi.spyOn(document, "hasFocus").mockReturnValue(true);
    const state = seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("world")).toBeInTheDocument());
    setScrollMetrics(messagesRegion(), { scrollHeight: 1000, clientHeight: 100, scrollTop: 920 });
    fireEvent.scroll(messagesRegion());

    await waitFor(() => {
      expect(state.markReadRequests).toContainEqual({
        channelId: 100,
        lastVisibleMessageId: 2,
      });
    });
    focusSpy.mockRestore();
  });

  test("incoming messages while scrolled up show a jump affordance without forcing scroll", async () => {
    const focusSpy = vi.spyOn(document, "hasFocus").mockReturnValue(true);
    const state = seedAuthed();
    const { container } = mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("world")).toBeInTheDocument());
    state.markReadRequests = [];
    const scroll = setScrollMetrics(messagesRegion(), {
      scrollHeight: 1000,
      clientHeight: 100,
      scrollTop: 100,
    });
    fireEvent.scroll(messagesRegion());
    assertExists(latestFakeEventSource(), "latestFakeEventSource").pushMessage({
      id: 3,
      user_id: 2,
      channel_id: 100,
      parent_id: null,
      text: "below viewport",
      username: "bob",
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [],
      attachments: [],
      embeds: [],
    });

    await waitFor(() => expect(screen.getByText("below viewport")).toBeInTheDocument());
    expect(scroll.scrollTop).toBe(100);
    expect(
      screen.getByRole("button", { name: /new messages below\. jump to latest messages/i }),
    ).toBeInTheDocument();
    expect(state.markReadRequests).not.toContainEqual({
      channelId: 100,
      lastVisibleMessageId: 3,
    });
    await expectNoA11yViolations(container, "new-message jump affordance");
    focusSpy.mockRestore();
  });

  test("incoming messages near the bottom keep following the latest message", async () => {
    const focusSpy = vi.spyOn(document, "hasFocus").mockReturnValue(true);
    seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("world")).toBeInTheDocument());
    const scroll = setScrollMetrics(messagesRegion(), {
      scrollHeight: 1000,
      clientHeight: 100,
      scrollTop: 920,
    });
    assertExists(latestFakeEventSource(), "latestFakeEventSource").pushMessage({
      id: 3,
      user_id: 2,
      channel_id: 100,
      parent_id: null,
      text: "auto-followed",
      username: "bob",
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [],
      attachments: [],
      embeds: [],
    });

    await waitFor(() => expect(screen.getByText("auto-followed")).toBeInTheDocument());
    await waitFor(() => expect(scroll.scrollTop).toBe(1000));
    expect(screen.queryByRole("button", { name: /new messages below/i })).toBeNull();
    focusSpy.mockRestore();
  });

  test("jumping to newest messages scrolls to bottom and hands off to mark-read", async () => {
    const focusSpy = vi.spyOn(document, "hasFocus").mockReturnValue(true);
    const state = seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("world")).toBeInTheDocument());
    state.markReadRequests = [];
    const scroll = setScrollMetrics(messagesRegion(), {
      scrollHeight: 1000,
      clientHeight: 100,
      scrollTop: 100,
    });
    fireEvent.scroll(messagesRegion());
    assertExists(latestFakeEventSource(), "latestFakeEventSource").pushMessage({
      id: 4,
      user_id: 2,
      channel_id: 100,
      parent_id: null,
      text: "jump target",
      username: "bob",
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [],
      attachments: [],
      embeds: [],
    });

    const jump = await screen.findByRole("button", {
      name: /new messages below\. jump to latest messages/i,
    });
    fireEvent.click(jump);

    await waitFor(() => expect(scroll.scrollTop).toBe(1000));
    await waitFor(() => {
      expect(state.markReadRequests).toContainEqual({
        channelId: 100,
        lastVisibleMessageId: 4,
      });
    });
    expect(screen.queryByRole("button", { name: /new messages below/i })).toBeNull();
    focusSpy.mockRestore();
  });

  test("blurred or hidden renderer state suppresses near-bottom mark-read", async () => {
    const focusSpy = vi.spyOn(document, "hasFocus").mockReturnValue(false);
    const state = seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("world")).toBeInTheDocument());
    setScrollMetrics(messagesRegion(), { scrollHeight: 1000, clientHeight: 100, scrollTop: 920 });
    fireEvent.scroll(messagesRegion());

    await new Promise((resolve) => setTimeout(resolve, 180));
    expect(state.markReadRequests).toEqual([]);
    focusSpy.mockRestore();
  });

  test("opens a thread side panel from the reply action, focuses the composer, and sends a reply", async () => {
    const state = seedAuthed();
    const { history } = mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: /reply in thread to message by alice/i }));

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("hello")).toBeInTheDocument());
    expect(history.get()).toBe("/channel/100?thread=1");

    const input = (await within(panel).findByLabelText(/thread reply/i)) as HTMLInputElement;
    await waitFor(() => expect(document.activeElement).toBe(input));
    fireEvent.input(input, { target: { value: "reply from panel" } });
    fireEvent.click(within(panel).getByRole("button", { name: /^send$/i }));

    await waitFor(() => {
      expect(within(panel).getByText("reply from panel")).toBeInTheDocument();
    });
    expect(state.sentThreadReplies).toEqual([{ rootId: 1, text: "reply from panel" }]);
    expect(state.sentMessages).toEqual([]);
  });

  test("thread panel renders hydrated mention labels from roots, replies, sends, and live updates", async () => {
    const state = seedThreadWithOwnReply({
      text: "existing reply <@2>",
      mentions: [
        {
          id: 2,
          username: "bob",
          display_name: "Bobby <Tables>",
          avatar_url: null,
        },
      ],
    });
    const bob: PublicUser = {
      id: 2,
      username: "bob",
      display_name: "Bobby <Tables>",
      avatar_url: null,
    };
    state.users = [...state.users, bob];
    state.messages["100"][0] = {
      ...state.messages["100"][0],
      text: "hello <@2>",
      mentions: [bob],
    };
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    const rootText = await findRenderedMessageTextWithin(panel, "hello @Bobby <Tables>");
    expect(within(rootText).getByText("@Bobby <Tables>")).toHaveAttribute("title", "@bob");
    await findRenderedMessageTextWithin(panel, "existing reply @Bobby <Tables>");

    const input = (await within(panel).findByLabelText(/thread reply/i)) as HTMLInputElement;
    fireEvent.input(input, { target: { value: "sent reply <@2>" } });
    fireEvent.click(within(panel).getByRole("button", { name: /^send$/i }));

    await findRenderedMessageTextWithin(panel, "sent reply @Bobby <Tables>");
    expect(state.sentThreadReplies).toContainEqual({ rootId: 1, text: "sent reply <@2>" });

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushThreadReplyCreated({
      channel_id: 100,
      root_message_id: 1,
      reply: {
        id: 52,
        user_id: 2,
        channel_id: 100,
        parent_id: 1,
        created_at: 1_700_000_030_000_000,
        text: "live reply <@2>",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [bob],
        attachments: [],
        embeds: [],
      },
      thread_summary: { reply_count: 3, last_reply_created_at: 1_700_000_030_000_000 },
    });
    await findRenderedMessageTextWithin(panel, "live reply @Bobby <Tables>");

    es.pushMessageUpdated({
      id: 52,
      user_id: 2,
      channel_id: 100,
      parent_id: 1,
      created_at: 1_700_000_030_000_000,
      text: "live edited <@2>",
      username: "bob",
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [bob],
      attachments: [],
      embeds: [],
    });
    await findRenderedMessageTextWithin(panel, "live edited @Bobby <Tables>");
    expect(queryRenderedMessageTextWithin(panel, "live reply @Bobby <Tables>")).toBeNull();
  });

  test("thread panel emphasizes mentioned replies independently from authored replies and tombstones", async () => {
    const state = seedAuthed();
    const selfUser: PublicUser = {
      id: DEV_USER.id,
      username: DEV_USER.username,
      display_name: DEV_USER.display_name,
      avatar_url: DEV_USER.avatar_url,
    };
    state.threadReplies["1"] = [
      {
        id: 71,
        user_id: 2,
        channel_id: 100,
        parent_id: 1,
        created_at: 1_700_000_010_000_000,
        text: "thread ping <@1>",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [selfUser],
        attachments: [],
        embeds: [],
      },
      {
        id: 72,
        user_id: DEV_USER.id,
        channel_id: 100,
        parent_id: 1,
        created_at: 1_700_000_011_000_000,
        text: "own reply no mention",
        username: DEV_USER.username,
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
      {
        id: 73,
        user_id: DEV_USER.id,
        channel_id: 100,
        parent_id: 1,
        created_at: 1_700_000_012_000_000,
        text: "own reply ping <@1>",
        username: DEV_USER.username,
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [selfUser],
        attachments: [],
        embeds: [],
      },
      {
        id: 74,
        user_id: 2,
        channel_id: 100,
        parent_id: 1,
        created_at: 1_700_000_013_000_000,
        deleted_at: 1_700_000_014_000_000,
        text: "deleted thread ping <@1>",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: true,
        mentions: [selfUser],
        attachments: [],
        embeds: [],
      },
    ];
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    const mentionedText = await findRenderedMessageTextWithin(panel, "thread ping @baipas");
    const mentionedRow = assertExists(
      mentionedText.closest("article") as HTMLElement | null,
      "mentioned thread row",
    );
    expect(mentionedRow).toHaveAttribute("data-mentioned-current-user", "true");
    expect(mentionedRow).not.toHaveAttribute("data-authored-by-current-user");
    expect(mentionedRow).toHaveClass("bg-yellow-50", "ring-yellow-300", "border-yellow-300");

    const ownText = await findRenderedMessageTextWithin(panel, "own reply no mention");
    const ownRow = assertExists(ownText.closest("article") as HTMLElement | null, "own thread row");
    expect(ownRow).toHaveAttribute("data-authored-by-current-user", "true");
    expect(ownRow).not.toHaveAttribute("data-mentioned-current-user");
    expect(ownRow).toHaveClass("border-blue-400", "bg-blue-50/50");

    const bothText = await findRenderedMessageTextWithin(panel, "own reply ping @baipas");
    const bothRow = assertExists(
      bothText.closest("article") as HTMLElement | null,
      "own mentioned thread row",
    );
    expect(bothRow).toHaveAttribute("data-authored-by-current-user", "true");
    expect(bothRow).toHaveAttribute("data-mentioned-current-user", "true");
    expect(bothRow).toHaveClass("border-blue-400", "bg-yellow-50", "ring-yellow-300");

    const deletedRow = assertExists(
      within(panel)
        .getByLabelText(/original message deleted/i)
        .closest("article") as HTMLElement | null,
      "deleted thread row",
    );
    expect(deletedRow).not.toHaveAttribute("data-mentioned-current-user");
    expect(within(deletedRow).queryByRole("button", { name: /mention baipas/i })).toBeNull();
  });

  test("thread reply edits render hydrated mention labels from the HTTP response", async () => {
    const state = seedThreadWithOwnReply({ text: "before mention edit" });
    const bob: PublicUser = {
      id: 2,
      username: "bob",
      display_name: "Bobby <Tables>",
      avatar_url: null,
    };
    state.users = [...state.users, bob];
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("before mention edit")).toBeInTheDocument());
    const input = await openThreadReplyEdit(panel);
    fireEvent.input(input, { target: { value: "after mention <@2>" } });
    fireEvent.click(within(panel).getByRole("button", { name: /^save$/i }));

    await waitFor(() => {
      expect(state.editedMessages).toContainEqual({ id: 70, text: "after mention <@2>" });
    });
    const rendered = await findRenderedMessageTextWithin(panel, "after mention @Bobby <Tables>");
    expect(within(rendered).getByText("@Bobby <Tables>")).toHaveAttribute("title", "@bob");
  });

  test("thread panel exposes reaction controls for the root and replies", async () => {
    const state = seedThreadWithOwnReply({
      reactions: [{ kind: "native", emoji: "❤️", count: 1, me_reacted: false }],
    });
    state.messages["100"][0] = {
      ...state.messages["100"][0],
      reactions: [{ kind: "native", emoji: "👍", count: 2, me_reacted: false }],
    };
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("own reply")).toBeInTheDocument());

    fireEvent.click(
      within(panel).getByRole("button", { name: /👍 2 reactions\. add your reaction/i }),
    );
    await waitFor(() => {
      expect(
        within(panel).getByRole("button", { name: /👍 3 reactions\. remove your reaction/i }),
      ).toHaveAttribute("aria-pressed", "true");
    });
    await waitFor(() => {
      expect(state.messages["100"][0].reactions).toEqual([
        { kind: "native", emoji: "👍", count: 3, me_reacted: true },
      ]);
    });

    fireEvent.click(
      within(panel).getByRole("button", { name: /add reaction to message by alice/i }),
    );
    const dialog = await screen.findByRole("dialog", { name: /emoji picker/i });
    fireEvent.input(within(dialog).getByRole("combobox", { name: /search and select emoji/i }), {
      target: { value: "thumb" },
    });
    const thumbsCell = await within(dialog).findByRole("gridcell", { name: /emoji :thumbsup:/i });
    fireEvent.click(within(thumbsCell).getByRole("button", { name: /emoji :thumbsup:/i }));

    await waitFor(() => {
      expect(
        within(panel).getByRole("button", { name: /👍 1 reaction\. remove your reaction/i }),
      ).toHaveAttribute("aria-pressed", "true");
    });
    await waitFor(() => {
      expect(state.threadReplies["1"][0].reactions).toContainEqual({
        kind: "native",
        emoji: "👍",
        count: 1,
        me_reacted: true,
      });
    });
  });

  test("thread reply reactions stay visible while editing and the Add Reaction trigger hides", async () => {
    seedThreadWithOwnReply({
      reactions: [{ kind: "native", emoji: "👍", count: 1, me_reacted: false }],
    });
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("own reply")).toBeInTheDocument());

    const input = await openThreadReplyEdit(panel);
    const replyArticle = assertExists(input.closest("article"), "thread reply article");

    expect(
      within(replyArticle).getByRole("button", { name: /👍 1 reaction\. add your reaction/i }),
    ).toBeInTheDocument();
    expect(within(replyArticle).queryByRole("button", { name: /add reaction/i })).toBeNull();
  });

  test("adds and removes custom reactions in channel and thread picker flows", async () => {
    const state = seedThreadWithOwnReply();
    state.customEmojis = [
      {
        id: 9001,
        name: "party",
        image_url: "/uploads/emojis/party.webp?v=1",
        animated: true,
        created_by_user_id: DEV_USER.id,
        created_at: 1,
        updated_at: 1,
        deleted_at: null,
      },
    ];
    mountAt("/channel/100?thread=1");

    await waitFor(() => expect(screen.getByText("world")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: /add reaction to message by bob/i }));
    let dialog = await screen.findByRole("dialog", { name: /emoji picker/i });
    fireEvent.input(within(dialog).getByRole("combobox", { name: /search and select emoji/i }), {
      target: { value: "party" },
    });
    fireEvent.click(
      within(await within(dialog).findByRole("gridcell", { name: /emoji :party:/i })).getByRole(
        "button",
      ),
    );

    const channelParty = await screen.findByRole("button", {
      name: /animated :party: 1 reaction\. remove your reaction/i,
    });
    expect(channelParty).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(channelParty);
    await waitFor(() => {
      expect(
        screen.queryByRole("button", {
          name: /animated :party: 1 reaction\. remove your reaction/i,
        }),
      ).toBeNull();
    });

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("own reply")).toBeInTheDocument());
    fireEvent.click(
      within(panel).getByRole("button", { name: /add reaction to message by alice/i }),
    );
    dialog = await screen.findByRole("dialog", { name: /emoji picker/i });
    fireEvent.input(within(dialog).getByRole("combobox", { name: /search and select emoji/i }), {
      target: { value: "party" },
    });
    fireEvent.click(
      within(await within(dialog).findByRole("gridcell", { name: /emoji :party:/i })).getByRole(
        "button",
      ),
    );

    let threadParty: HTMLElement | null = null;
    await waitFor(() => {
      threadParty = within(panel).getByRole("button", {
        name: /animated :party: 1 reaction\. remove your reaction/i,
      });
      expect(threadParty).toHaveAttribute("aria-pressed", "true");
    });
    const finalThreadParty = within(panel).getByRole("button", {
      name: /animated :party: 1 reaction\. remove your reaction/i,
    });
    fireEvent.click(finalThreadParty);
    await waitFor(() => {
      expect(
        within(panel).queryByRole("button", {
          name: /animated :party: 1 reaction\. remove your reaction/i,
        }),
      ).toBeNull();
    });
  });

  test("rolls back failed reaction mutations in channel and thread surfaces", async () => {
    seedThreadWithOwnReply({
      reactions: [{ kind: "native", emoji: "👍", count: 1, me_reacted: true }],
    });
    const channelFailure = deferred();
    const threadFailure = deferred();
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    server.use(
      http.post(`${TEST_SERVER}/message/2/reactions`, async () => {
        await channelFailure.promise;
        return new HttpResponse(null, { status: 500 });
      }),
      http.delete(`${TEST_SERVER}/message/70/reactions`, async () => {
        await threadFailure.promise;
        return new HttpResponse(null, { status: 500 });
      }),
    );
    mountAt("/channel/100?thread=1");

    await waitFor(() => expect(screen.getByText("world")).toBeInTheDocument());
    const worldRow = assertExists(
      screen.getByText("world").closest(".group"),
      "world row",
    ) as HTMLElement;
    fireEvent.click(screen.getByRole("button", { name: /add reaction to message by bob/i }));
    const dialog = await screen.findByRole("dialog", { name: /emoji picker/i });
    fireEvent.input(within(dialog).getByRole("combobox", { name: /search and select emoji/i }), {
      target: { value: "thumb" },
    });
    fireEvent.click(
      within(await within(dialog).findByRole("gridcell", { name: /emoji :thumbsup:/i })).getByRole(
        "button",
      ),
    );

    await waitFor(() => {
      expect(
        within(worldRow).getByRole("button", { name: /👍 1 reaction\. remove your reaction/i }),
      ).toHaveAttribute("aria-pressed", "true");
    });
    channelFailure.resolve();
    await waitFor(() => {
      expect(
        within(worldRow).queryByRole("button", { name: /👍 1 reaction\. remove your reaction/i }),
      ).toBeNull();
    });

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    const threadPill = within(panel).getByRole("button", {
      name: /👍 1 reaction\. remove your reaction/i,
    });
    fireEvent.click(threadPill);
    await waitFor(() => {
      expect(
        within(panel).queryByRole("button", { name: /👍 1 reaction\. remove your reaction/i }),
      ).toBeNull();
    });
    threadFailure.resolve();
    await waitFor(() => {
      expect(
        within(panel).getByRole("button", { name: /👍 1 reaction\. remove your reaction/i }),
      ).toHaveAttribute("aria-pressed", "true");
    });
    expect(errorSpy).toHaveBeenCalledWith("failed to update reaction", expect.any(Error));
    errorSpy.mockRestore();
  });

  test("reaction picker and focused reactor preview expose accessible labels", async () => {
    const state = seedAuthed();
    state.messages["100"][1] = {
      ...state.messages["100"][1],
      reactions: [
        {
          kind: "native",
          emoji: "👍",
          count: 3,
          me_reacted: false,
          reactors: ["Alice", "Bob", "Carol"],
        },
      ],
    };
    const { container } = mountAt("/channel/100");

    const pill = await screen.findByRole("button", {
      name: /👍 3 reactions\. add your reaction/i,
    });
    pill.focus();
    await waitFor(() => expect(screen.getByRole("tooltip")).toHaveTextContent("Alice"));
    const describedBy = pill.getAttribute("aria-describedby");
    expect(describedBy).toBeTruthy();
    expect(
      assertExists(document.getElementById(describedBy ?? ""), "reaction preview"),
    ).toHaveTextContent("3 reactions: Alice, Bob, Carol");

    const addReaction = screen.getByRole("button", { name: /add reaction to message by bob/i });
    addReaction.focus();
    expect(document.activeElement).toBe(addReaction);
    fireEvent.click(addReaction);
    await screen.findByRole("dialog", { name: /emoji picker/i });
    expect(screen.getByRole("combobox", { name: /search and select emoji/i })).toBeInTheDocument();
    await expectNoA11yViolations(container, "reaction picker and reactor preview");
  });

  test("thread panel patches root and reply reactions from live SSE updates", async () => {
    seedThreadWithOwnReply();
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("own reply")).toBeInTheDocument());
    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");

    es.pushMessageReactionsUpdated({
      id: 1,
      channel_id: 100,
      parent_id: null,
      root_message_id: 1,
      user_id: 2,
      reactions: [{ kind: "native", emoji: "👍", count: 1, me_reacted: true }],
    });
    es.pushMessageReactionsUpdated({
      id: 70,
      channel_id: 100,
      parent_id: 1,
      root_message_id: 1,
      user_id: 2,
      reactions: [{ kind: "native", emoji: "❤️", count: 2, me_reacted: true }],
    });

    await waitFor(() => {
      expect(
        within(panel).getByRole("button", { name: /👍 1 reaction\. add your reaction/i }),
      ).toHaveAttribute("aria-pressed", "false");
      expect(
        within(panel).getByRole("button", { name: /❤️ 2 reactions\. add your reaction/i }),
      ).toHaveAttribute("aria-pressed", "false");
    });
  });

  test("thread reply Shift+Enter inserts a newline and Enter submits exact multiline text", async () => {
    const state = seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: /reply in thread to message by alice/i }));

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    const input = (await within(panel).findByLabelText(/thread reply/i)) as HTMLInputElement;
    fireEvent.input(input, { target: { value: "thread first line" } });
    setInputSelection(input, "thread first line".length);

    fireEvent.keyDown(input, { key: "Enter", shiftKey: true });

    await waitFor(() => {
      expect(input.value).toBe("thread first line\n");
      expect(input.selectionStart).toBe("thread first line\n".length);
    });

    const text = "thread first line\nthread second line";
    fireEvent.input(input, { target: { value: text } });
    setInputSelection(input, text.length);
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => {
      expect(state.sentThreadReplies).toContainEqual({ rootId: 1, text });
      expect(input.value).toBe("");
      expect(document.activeElement).toBe(input);
    });
    const replyText = await findRenderedMessageTextWithin(panel, text);
    expect(replyText.textContent).toBe(text);
    expect(replyText).toHaveClass("whitespace-pre-wrap", "break-words", "[overflow-wrap:anywhere]");
  });

  test("thread reply composer commits emoji autocomplete before submitting", async () => {
    const state = seedAuthed();
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("hello")).toBeInTheDocument());
    const input = (await within(panel).findByLabelText(/thread reply/i)) as HTMLInputElement;
    const textWithToken = "thread :sm";
    fireEvent.input(input, { target: { value: textWithToken } });
    setInputSelection(input, textWithToken.length);

    const listbox = await screen.findByRole("listbox", { name: /emoji suggestions/i });
    expect(within(listbox).getByRole("option", { name: /:smiley:/i })).toBeInTheDocument();

    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => expect(input.value).toBe("thread 😃"));
    expect(state.sentThreadReplies).toEqual([]);

    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => {
      expect(state.sentThreadReplies).toContainEqual({ rootId: 1, text: "thread 😃" });
    });
  });

  test("thread reply composer searches, chips, serializes, and renders user mentions", async () => {
    const state = seedAuthed();
    const bob: PublicUser = {
      id: 2,
      username: "bobthreadcompose",
      display_name: "Bobby Thread Compose",
      avatar_url: null,
    };
    state.users.push(bob);
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("hello")).toBeInTheDocument());
    const input = (await within(panel).findByLabelText(/thread reply/i)) as HTMLInputElement;

    inputFromUser(input, "thread @bobthread");
    const listbox = await screen.findByRole("listbox", { name: /mention suggestions/i });
    const bobOption = within(listbox).getByRole("option", {
      name: /mention bobby thread compose @bobthreadcompose/i,
    });
    expect(bobOption).toHaveAttribute("aria-selected", "true");
    expect(state.userSearchRequests).toContainEqual({ query: "bobthread", limit: 8 });

    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => {
      expect(input.value).toBe("thread <@2> ");
      expect(within(input).getByText("@Bobby Thread Compose")).toBeInTheDocument();
    });
    expect(state.sentThreadReplies).toEqual([]);

    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => {
      expect(state.sentThreadReplies).toContainEqual({ rootId: 1, text: "thread <@2> " });
      expect(input.value).toBe("");
    });
    expect(within(panel).getByText("@Bobby Thread Compose")).toHaveAttribute(
      "title",
      "@bobthreadcompose",
    );
  });

  test("failed thread replies restore the exact multiline draft after clearing", async () => {
    const state = seedAuthed();
    let releaseReply: () => void = () => undefined;
    const replyPaused = new Promise<void>((resolve) => {
      releaseReply = resolve;
    });
    server.use(
      http.post(`${TEST_SERVER}/thread/1/reply`, async ({ request }) => {
        const body = (await request.json()) as { text: string };
        state.sentThreadReplies.push({ rootId: 1, text: body.text });
        await replyPaused;
        return new HttpResponse(null, { status: 500 });
      }),
    );
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: /reply in thread to message by alice/i }));

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    const input = (await within(panel).findByLabelText(/thread reply/i)) as HTMLInputElement;
    const text = "restore first line\nrestore second line";
    fireEvent.input(input, { target: { value: text } });
    fireEvent.click(within(panel).getByRole("button", { name: /^send$/i }));

    await waitFor(() => {
      expect(state.sentThreadReplies).toContainEqual({ rootId: 1, text });
      expect(input.value).toBe("");
    });
    releaseReply();

    await waitFor(() => {
      expect(input.value).toBe(text);
      expect(within(panel).getByRole("alert")).toHaveTextContent("Thread reply failed (500)");
    });
  });

  test("SSE-delivered multiline thread replies render with preserved line breaks", async () => {
    seedAuthed();
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("hello")).toBeInTheDocument());
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const text = "SSE first line\nSSE second line\nSSE third line";
    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushThreadReplyCreated({
      channel_id: 100,
      root_message_id: 1,
      reply: {
        id: 52,
        user_id: 2,
        channel_id: 100,
        parent_id: 1,
        created_at: 1_700_000_030_000_000,
        text,
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
      thread_summary: { reply_count: 1, last_reply_created_at: 1_700_000_030_000_000 },
    });

    const replyText = await findRenderedMessageTextWithin(panel, text);
    expect(replyText.textContent).toBe(text);
    expect(replyText).toHaveClass("whitespace-pre-wrap", "break-words", "[overflow-wrap:anywhere]");
  });

  test("thread root and reply URLs around newlines stay clickable without losing line breaks", async () => {
    const state = seedAuthed();
    const rootText = "root before https://root-before.test\nhttps://root-solo.test root after";
    const replyText = "reply before\nhttps://reply-solo.test\nreply after https://reply-after.test";
    state.messages["100"] = [
      {
        ...state.messages["100"][0],
        text: rootText,
      },
    ];
    state.threadReplies["1"] = [
      {
        id: 53,
        user_id: 2,
        channel_id: 100,
        parent_id: 1,
        created_at: 1_700_000_031_000_000,
        text: replyText,
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
    ];

    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    const rootMessageText = await findRenderedMessageTextWithin(panel, rootText);
    expect(rootMessageText.textContent).toBe(rootText);
    expect(
      within(rootMessageText).getByRole("link", { name: "https://root-before.test" }),
    ).toHaveAttribute("href", "https://root-before.test");
    expect(
      within(rootMessageText).getByRole("link", { name: "https://root-solo.test" }),
    ).toHaveAttribute("href", "https://root-solo.test");

    const replyMessageText = await findRenderedMessageTextWithin(panel, replyText);
    expect(replyMessageText.textContent).toBe(replyText);
    expect(
      within(replyMessageText).getByRole("link", { name: "https://reply-solo.test" }),
    ).toHaveAttribute("href", "https://reply-solo.test");
    expect(
      within(replyMessageText).getByRole("link", { name: "https://reply-after.test" }),
    ).toHaveAttribute("href", "https://reply-after.test");
  });

  test("thread reply edit Shift+Enter inserts a newline and Enter PUTs exact multiline text", async () => {
    const state = seedThreadWithOwnReply({ text: "reply first line" });
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("reply first line")).toBeInTheDocument());
    const input = await openThreadReplyEdit(panel);
    setInputSelection(input, "reply first line".length);

    fireEvent.keyDown(input, { key: "Enter", shiftKey: true });

    await waitFor(() => {
      expect(input.value).toBe("reply first line\n");
      expect(input.selectionStart).toBe("reply first line\n".length);
    });

    const text = "reply first line\nreply second line";
    fireEvent.input(input, { target: { value: text } });
    setInputSelection(input, text.length);
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => {
      expect(state.editedMessages).toContainEqual({ id: 70, text });
    });
    const replyText = await findRenderedMessageTextWithin(panel, text);
    expect(replyText.textContent).toBe(text);
    expect(replyText).toHaveClass("whitespace-pre-wrap", "break-words", "[overflow-wrap:anywhere]");
  });

  test("thread reply edit commits emoji autocomplete before saving", async () => {
    const state = seedThreadWithOwnReply({ text: "reply body" });
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("reply body")).toBeInTheDocument());
    const input = await openThreadReplyEdit(panel);
    const textWithToken = "reply :sm";
    fireEvent.input(input, { target: { value: textWithToken } });
    setInputSelection(input, textWithToken.length);

    const listbox = await screen.findByRole("listbox", { name: /emoji suggestions/i });
    expect(within(listbox).getByRole("option", { name: /:smiley:/i })).toBeInTheDocument();

    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => expect(input.value).toBe("reply 😃"));
    expect(state.editedMessages).toEqual([]);

    fireEvent.click(
      within(assertExists(input.closest("form"), "thread reply edit form")).getByRole("button", {
        name: /^save$/i,
      }),
    );

    await waitFor(() => {
      expect(state.editedMessages).toContainEqual({ id: 70, text: "reply 😃" });
    });
  });

  test("thread reply edits search, chip, save markers, and render hydrated mentions", async () => {
    const state = seedThreadWithOwnReply({ text: "reply body" });
    const bob: PublicUser = {
      id: 2,
      username: "bobthreadedit",
      display_name: "Bobby Thread Edit",
      avatar_url: null,
    };
    state.users.push(bob);
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("reply body")).toBeInTheDocument());
    const input = await openThreadReplyEdit(panel);

    inputFromUser(input, "reply @bobthreadedit");
    const listbox = await screen.findByRole("listbox", { name: /mention suggestions/i });
    expect(
      within(listbox).getByRole("option", {
        name: /mention bobby thread edit @bobthreadedit/i,
      }),
    ).toHaveAttribute("aria-selected", "true");
    expect(state.userSearchRequests).toContainEqual({ query: "bobthreadedit", limit: 8 });

    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => {
      expect(input.value).toBe("reply <@2> ");
      expect(within(input).getByText("@Bobby Thread Edit")).toBeInTheDocument();
    });
    expect(state.editedMessages).toEqual([]);

    fireEvent.click(
      within(assertExists(input.closest("form"), "thread reply edit form")).getByRole("button", {
        name: /^save$/i,
      }),
    );
    await waitFor(() => {
      expect(state.editedMessages).toContainEqual({ id: 70, text: "reply <@2> " });
    });
    expect(within(panel).getByText("@Bobby Thread Edit")).toHaveAttribute(
      "title",
      "@bobthreadedit",
    );
  });

  test("thread reply edit Save button PUTs exact multiline text", async () => {
    const state = seedThreadWithOwnReply({ text: "button edit" });
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("button edit")).toBeInTheDocument());
    const input = await openThreadReplyEdit(panel);
    const text = "button first line\nbutton second line\nbutton third line";
    fireEvent.input(input, { target: { value: text } });
    fireEvent.click(within(panel).getByRole("button", { name: /^save$/i }));

    await waitFor(() => {
      expect(state.editedMessages).toContainEqual({ id: 70, text });
    });
    expect((await findRenderedMessageTextWithin(panel, text)).textContent).toBe(text);
  });

  test("Escape cancels thread reply multiline edits without PUTing", async () => {
    const state = seedThreadWithOwnReply({ text: "cancel first line\ncancel second line" });
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    const original = await findRenderedMessageTextWithin(
      panel,
      "cancel first line\ncancel second line",
    );
    expect(original).toBeInTheDocument();
    const input = await openThreadReplyEdit(panel);
    fireEvent.input(input, { target: { value: "cancel first line\ncancel second line\nunsaved" } });
    fireEvent.keyDown(input, { key: "Escape" });

    await waitFor(() =>
      expect(within(panel).queryByRole("textbox", { name: /edit reply/i })).toBeNull(),
    );
    expect(state.editedMessages).toEqual([]);
    expect(
      await findRenderedMessageTextWithin(panel, "cancel first line\ncancel second line"),
    ).toBeInTheDocument();
  });

  test("Escape dismisses thread reply edit mention autocomplete before canceling the edit", async () => {
    const state = seedThreadWithOwnReply({ text: "thread escape edit" });
    state.users.push({
      id: 2,
      username: "bobthreadescape",
      display_name: "Bobby Thread Escape",
      avatar_url: null,
    });
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("thread escape edit")).toBeInTheDocument());
    const input = await openThreadReplyEdit(panel);
    inputFromUser(input, "thread @bobthreadescape");
    await screen.findByRole("listbox", { name: /mention suggestions/i });

    fireEvent.keyDown(input, { key: "Escape" });

    await waitFor(() =>
      expect(screen.queryByRole("listbox", { name: /mention suggestions/i })).toBeNull(),
    );
    expect(within(panel).getByRole("textbox", { name: /edit reply/i })).toBeInTheDocument();
    expect(state.editedMessages).toEqual([]);

    fireEvent.keyDown(input, { key: "Escape" });

    await waitFor(() =>
      expect(within(panel).queryByRole("textbox", { name: /edit reply/i })).toBeNull(),
    );
    expect(state.editedMessages).toEqual([]);
    expect(within(panel).getByText("thread escape edit")).toBeInTheDocument();
  });

  test("unchanged thread reply multiline edits exit edit mode without PUTing", async () => {
    const text = "same reply first line\nsame reply second line";
    const state = seedThreadWithOwnReply({ text });
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await findRenderedMessageTextWithin(panel, text);
    const input = await openThreadReplyEdit(panel);
    fireEvent.submit(assertExists(input.closest("form"), "thread reply edit form"));

    await waitFor(() =>
      expect(within(panel).queryByRole("textbox", { name: /edit reply/i })).toBeNull(),
    );
    expect(state.editedMessages).toEqual([]);
  });

  test("blank thread reply multiline edits prompt for delete instead of PUTing blank text", async () => {
    const state = seedThreadWithOwnReply({ text: "blank first line\nblank second line" });
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await findRenderedMessageTextWithin(panel, "blank first line\nblank second line");
    const input = await openThreadReplyEdit(panel);
    fireEvent.input(input, { target: { value: "" } });
    fireEvent.submit(assertExists(input.closest("form"), "thread reply edit form"));

    const dialog = await screen.findByRole("dialog", { name: /delete reply/i });
    expect(state.editedMessages).not.toContainEqual({ id: 70, text: "" });

    fireEvent.click(within(dialog).getByRole("button", { name: /^delete$/i }));

    await waitFor(() => {
      expect(state.deletedMessageIds).toContain(70);
      expect(
        queryRenderedMessageTextWithin(panel, "blank first line\nblank second line"),
      ).toBeNull();
    });
  });

  test("blank thread reply edits with photos save an empty caption instead of prompting delete", async () => {
    const state = seedThreadWithOwnReply({
      text: "photo reply caption",
      attachments: [makeAttachment({ id: 9401, message_id: 70 })],
    });
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("photo reply caption")).toBeInTheDocument());
    expect(
      within(panel).getByRole("img", { name: /photo attachment from baipas/i }),
    ).toBeInTheDocument();
    const input = await openThreadReplyEdit(panel);
    fireEvent.input(input, { target: { value: "" } });
    fireEvent.submit(assertExists(input.closest("form"), "thread reply edit form"));

    await waitFor(() => expect(state.editedMessages).toContainEqual({ id: 70, text: "" }));
    expect(screen.queryByRole("dialog", { name: /delete reply/i })).toBeNull();
    expect(state.deletedMessageIds).not.toContain(70);
    expect(
      within(panel).getByRole("img", { name: /photo attachment from baipas/i }),
    ).toBeInTheDocument();
  });

  test("message_updated SSE events update thread replies with preserved line breaks", async () => {
    seedThreadWithOwnReply({ text: "before update" });
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("before update")).toBeInTheDocument());
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const text = "live edit first line\nlive edit second line\nlive edit third line";
    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessageUpdated({
      id: 70,
      user_id: DEV_USER.id,
      channel_id: 100,
      parent_id: 1,
      created_at: 1_700_000_010_000_000,
      text,
      username: DEV_USER.username,
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [],
      attachments: [],
      embeds: [],
    });

    const replyText = await findRenderedMessageTextWithin(panel, text);
    expect(replyText.textContent).toBe(text);
    expect(replyText).toHaveClass("whitespace-pre-wrap", "break-words", "[overflow-wrap:anywhere]");
    expect(within(panel).queryByText("before update")).toBeNull();
  });

  test("thread panel keeps replies scrollable while the reply composer grows", async () => {
    seedAuthed();
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    const input = (await within(panel).findByLabelText(/thread reply/i)) as HTMLInputElement;
    fireEvent.input(input, {
      target: { value: "one\ntwo\nthree\nfour\nfive\nsix\nseven" },
    });

    expect(panel).toHaveClass("min-h-0", "flex", "flex-col");
    const scrollArea = panel.querySelector(".min-h-0.flex-1.overflow-y-auto");
    expect(scrollArea).not.toBeNull();
    expect(input).toHaveClass("max-h-40", "overflow-y-auto", "whitespace-pre-wrap");
    expect(assertExists(input.closest("form"), "thread reply form")).toHaveClass("flex-shrink-0");
  });

  test("opens a deep-linked thread without stealing focus", async () => {
    const state = seedAuthed();
    state.threadReplies["1"] = [
      {
        id: 10,
        user_id: 2,
        channel_id: 100,
        parent_id: 1,
        text: "linked reply",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
    ];
    const { container, history } = mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => {
      expect(within(panel).getByText("hello")).toBeInTheDocument();
      expect(within(panel).getByText("linked reply")).toBeInTheDocument();
    });

    const input = await within(panel).findByLabelText(/thread reply/i);
    expect(document.activeElement).not.toBe(input);
    expect(history.get()).toBe("/channel/100?thread=1");
    await expectNoA11yViolations(container, "deep-linked thread panel");
  });

  test("channel and thread composers expose accessible multiline textbox semantics", async () => {
    seedAuthed();
    const { container } = mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("hello")).toBeInTheDocument());

    const channelComposer = await screen.findByRole("textbox", { name: /new message/i });
    expect(channelComposer).toHaveAttribute("aria-multiline", "true");
    expect(channelComposer).toHaveAttribute("aria-placeholder", "Send a new message...");
    expect(channelComposer).toHaveClass(
      "max-h-40",
      "overflow-y-auto",
      "whitespace-pre-wrap",
      "break-words",
    );

    const threadComposer = await within(panel).findByRole("textbox", { name: /thread reply/i });
    expect(threadComposer).toHaveAttribute("aria-multiline", "true");
    expect(threadComposer).toHaveAttribute("aria-placeholder", "Reply in thread...");
    expect(threadComposer).toHaveClass(
      "max-h-40",
      "overflow-y-auto",
      "whitespace-pre-wrap",
      "break-words",
    );

    await expectNoA11yViolations(container, "channel and thread multiline composers");
  });

  test("loads a long thread newest-first page and prepends older replies on demand", async () => {
    const state = seedAuthed();
    state.threadReplies["1"] = Array.from({ length: 55 }, (_, index) => {
      const replyNumber = index + 1;
      return {
        id: 1_000 + replyNumber,
        user_id: 2,
        channel_id: 100,
        parent_id: 1,
        created_at: 1_700_000_000_000_000 + replyNumber,
        text: `reply ${replyNumber}`,
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      } satisfies Message;
    });
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => {
      expect(within(panel).getByText("reply 6")).toBeInTheDocument();
      expect(within(panel).getByText("reply 55")).toBeInTheDocument();
    });
    expect(within(panel).queryByText("reply 5")).toBeNull();
    expect(state.threadRequests[0]).toEqual({
      rootId: 1,
      limit: 50,
      beforeCreatedAt: null,
      beforeId: null,
    });

    fireEvent.click(within(panel).getByRole("button", { name: /load older replies/i }));

    await waitFor(() => {
      expect(within(panel).getByText("reply 1")).toBeInTheDocument();
      expect(within(panel).getByText("reply 5")).toBeInTheDocument();
      expect(within(panel).queryByRole("button", { name: /load older replies/i })).toBeNull();
    });
    expect(state.threadRequests[1]).toEqual({
      rootId: 1,
      limit: 50,
      beforeCreatedAt: 1_700_000_000_000_006,
      beforeId: 1_006,
    });
    const text = panel.textContent ?? "";
    expect(text.indexOf("reply 5")).toBeLessThan(text.indexOf("reply 6"));
  });

  test("opens the correct thread from a channel summary without focusing the composer", async () => {
    const state = seedAuthed();
    state.messages["100"] = [
      {
        id: 1,
        user_id: 1,
        channel_id: 100,
        text: "quiet root",
        username: "alice",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
      {
        id: 2,
        user_id: 2,
        channel_id: 100,
        text: "busy root",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
        thread_summary: { reply_count: 2, last_reply_created_at: 1_700_000_000_000_000 },
      },
    ];
    state.threadReplies["2"] = [
      {
        id: 20,
        user_id: 1,
        channel_id: 100,
        parent_id: 2,
        text: "reply in busy thread",
        username: "alice",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
    ];
    const { history } = mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("busy root")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: /open thread with 2 replies/i }));

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => {
      expect(within(panel).getByText("busy root")).toBeInTheDocument();
      expect(within(panel).getByText("reply in busy thread")).toBeInTheDocument();
    });
    expect(history.get()).toBe("/channel/100?thread=2");
    expect(document.activeElement).not.toBe(within(panel).getByLabelText(/thread reply/i));
    expect(within(panel).queryByText("quiet root")).toBeNull();
  });

  test("closing, swapping, and back/forward navigation keep thread route state predictable", async () => {
    seedAuthed();
    const { history } = mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: /reply in thread to message by alice/i }));

    let panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("hello")).toBeInTheDocument());
    expect(history.get()).toBe("/channel/100?thread=1");

    fireEvent.click(screen.getByRole("button", { name: /reply in thread to message by bob/i }));

    await waitFor(() => {
      panel = screen.getByRole("complementary", { name: /thread panel/i });
      expect(within(panel).getByText("world")).toBeInTheDocument();
      expect(history.get()).toBe("/channel/100?thread=2");
    });

    fireEvent.click(within(panel).getByRole("button", { name: /close thread/i }));
    await waitFor(() => {
      expect(screen.queryByRole("complementary", { name: /thread panel/i })).toBeNull();
      expect(history.get()).toBe("/channel/100");
    });

    history.back();
    panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => {
      expect(within(panel).getByText("world")).toBeInTheDocument();
      expect(history.get()).toBe("/channel/100?thread=2");
    });

    history.back();
    panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => {
      expect(within(panel).getByText("hello")).toBeInTheDocument();
      expect(history.get()).toBe("/channel/100?thread=1");
    });

    history.forward();
    panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => {
      expect(within(panel).getByText("world")).toBeInTheDocument();
      expect(history.get()).toBe("/channel/100?thread=2");
    });
  });

  test("changing channels closes the thread panel", async () => {
    const state = seedAuthed();
    state.channels.push({ id: 200, name: "random", position: 1, type: "text" });
    state.messages["200"] = [
      {
        id: 2001,
        user_id: 2,
        channel_id: 200,
        text: "different channel",
        username: "carol",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
    ];
    const { history } = mountAt("/channel/100?thread=1");

    await screen.findByRole("complementary", { name: /thread panel/i });

    history.set({ value: "/channel/200" });

    await waitFor(() => {
      expect(screen.getByText("different channel")).toBeInTheDocument();
      expect(screen.queryByRole("complementary", { name: /thread panel/i })).toBeNull();
    });
  });

  test("invalid thread route state is removed for the current channel", async () => {
    seedAuthed();
    const { history } = mountAt("/channel/100?thread=999");

    await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
    await waitFor(() => {
      expect(screen.queryByRole("complementary", { name: /thread panel/i })).toBeNull();
      expect(history.get()).toBe("/channel/100");
    });
  });

  test("renders received literal emoji shortcode text without render-time conversion", async () => {
    const state = seedAuthed();
    state.messages["100"] = [
      {
        id: 10,
        user_id: 2,
        channel_id: 100,
        text: "stored :grinning: shortcode",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
    ];
    mountAt("/channel/100");

    await waitFor(() => {
      expect(screen.getByText("stored :grinning: shortcode")).toBeInTheDocument();
    });
    expect(screen.queryByText("stored 😀 shortcode")).toBeNull();
  });

  test("renders fetched multiline channel messages with preserved line breaks", async () => {
    const state = seedAuthed();
    const text = "fetched first line\nfetched second line\nfetched third line";
    state.messages["100"] = [
      {
        id: 12,
        user_id: 2,
        channel_id: 100,
        text,
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
    ];
    mountAt("/channel/100");

    const messageText = await findRenderedMessageText(text);
    expect(messageText.textContent).toBe(text);
    expect(messageText).toHaveClass(
      "whitespace-pre-wrap",
      "break-words",
      "[overflow-wrap:anywhere]",
    );
  });

  test("renders photo attachments from history and incoming SSE payloads", async () => {
    const state = seedAuthed();
    state.messages["100"] = [
      {
        id: 13,
        user_id: 2,
        channel_id: 100,
        text: "history photo",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [makeAttachment({ id: 9101, message_id: 13 })],
        embeds: [],
      },
    ];
    mountAt("/channel/100");

    const historyImage = await screen.findByRole("img", { name: /photo attachment from bob/i });
    expect(historyImage).toHaveAttribute("src", `${TEST_SERVER}/attachments/9101/thumbnail`);
    expect(screen.getByRole("button", { name: /open photo attachment from bob/i })).toBeEnabled();
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessage({
      id: 14,
      user_id: 3,
      channel_id: 100,
      text: "live photo",
      username: "carol",
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [],
      attachments: [makeAttachment({ id: 9102, message_id: 14 })],
      embeds: [],
    });

    const liveImage = await screen.findByRole("img", { name: /photo attachment from carol/i });
    expect(liveImage).toHaveAttribute("src", `${TEST_SERVER}/attachments/9102/thumbnail`);
    expect(screen.getByText("live photo")).toBeInTheDocument();
  });

  test("thread panel renders root, paginated reply, and live reply photo attachments", async () => {
    const state = seedAuthed();
    state.messages["100"][0] = {
      ...state.messages["100"][0],
      attachments: [makeAttachment({ id: 9201, message_id: 1 })],
    };
    state.threadReplies["1"] = Array.from({ length: 51 }, (_, index) => {
      const replyNumber = index + 1;
      return {
        id: 2_000 + replyNumber,
        user_id: 2,
        channel_id: 100,
        parent_id: 1,
        created_at: 1_700_000_000_000_000 + replyNumber,
        text: `photo reply ${replyNumber}`,
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments:
          replyNumber === 1 || replyNumber === 51
            ? [makeAttachment({ id: 9201 + replyNumber, message_id: 2_000 + replyNumber })]
            : [],
        embeds: [],
      } satisfies Message;
    });
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("photo reply 51")).toBeInTheDocument());
    expect(
      within(panel).getByRole("button", { name: /open photo attachment from alice/i }),
    ).toBeEnabled();
    expect(within(panel).getByRole("img", { name: /photo attachment from bob/i })).toHaveAttribute(
      "src",
      `${TEST_SERVER}/attachments/9252/thumbnail`,
    );
    expect(within(panel).queryByText("photo reply 1")).toBeNull();

    fireEvent.click(within(panel).getByRole("button", { name: /load older replies/i }));

    await waitFor(() => expect(within(panel).getByText("photo reply 1")).toBeInTheDocument());
    const bobPhotos = within(panel).getAllByRole("img", { name: /photo attachment from bob/i });
    expect(
      bobPhotos.some(
        (img) => img.getAttribute("src") === `${TEST_SERVER}/attachments/9202/thumbnail`,
      ),
    ).toBe(true);
    expect(
      bobPhotos.some(
        (img) => img.getAttribute("src") === `${TEST_SERVER}/attachments/9252/thumbnail`,
      ),
    ).toBe(true);

    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());
    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushThreadReplyCreated({
      channel_id: 100,
      root_message_id: 1,
      reply: {
        id: 2_100,
        user_id: 3,
        channel_id: 100,
        parent_id: 1,
        created_at: 1_700_000_060_000_000,
        text: "live thread photo",
        username: "carol",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [makeAttachment({ id: 9301, message_id: 2_100 })],
        embeds: [],
      },
      thread_summary: { reply_count: 52, last_reply_created_at: 1_700_000_060_000_000 },
    });

    const liveImage = await within(panel).findByRole("img", {
      name: /photo attachment from carol/i,
    });
    expect(liveImage).toHaveAttribute("src", `${TEST_SERVER}/attachments/9301/thumbnail`);
    expect(within(panel).getByText("live thread photo")).toBeInTheDocument();
  });

  test("appends a message delivered over SSE and auto-follows when already near bottom", async () => {
    seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const scrollArea = screen.getByRole("region", { name: /messages/i }) as HTMLDivElement;
    const scroll = setScrollMetrics(scrollArea, {
      scrollHeight: 1234,
      clientHeight: 100,
      scrollTop: 1140,
    });

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessage({
      id: 3,
      user_id: 3,
      channel_id: 100,
      text: "hot off the wire",
      username: "carol",
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [],
      attachments: [],
      embeds: [],
    });

    await waitFor(() => {
      expect(screen.getByText("hot off the wire")).toBeInTheDocument();
      expect(scroll.scrollTop).toBe(1234);
    });
  });

  test("renders multiline messages delivered over SSE with preserved line breaks", async () => {
    seedAuthed();
    const text = "live first line\nlive second line\nlive third line";
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessage({
      id: 102,
      user_id: 3,
      channel_id: 100,
      text,
      username: "carol",
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [],
      attachments: [],
      embeds: [],
    });

    const messageText = await findRenderedMessageText(text);
    expect(messageText.textContent).toBe(text);
    expect(messageText).toHaveClass(
      "whitespace-pre-wrap",
      "break-words",
      "[overflow-wrap:anywhere]",
    );
  });

  test("renders emoji glyph messages delivered over SSE with linkification", async () => {
    seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessage({
      id: 101,
      user_id: 3,
      channel_id: 100,
      text: "realtime 😀 link https://example.com",
      username: "carol",
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [],
      attachments: [],
      embeds: [],
    });

    await waitFor(() => {
      const link = screen.getByRole("link", { name: "https://example.com" });
      expect(assertExists(link.parentElement, "message text")).toHaveTextContent(
        "realtime 😀 link https://example.com",
      );
    });
  });

  test("ignores SSE messages destined for other channels", async () => {
    seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessage({
      id: 99,
      user_id: 3,
      channel_id: 999,
      text: "do not show",
      username: "carol",
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [],
      attachments: [],
      embeds: [],
    });

    // Give reactivity a tick to flush; then assert nothing appeared.
    await new Promise((r) => setTimeout(r, 10));
    expect(screen.queryByText("do not show")).toBeNull();
  });

  test("appends matching thread_reply_created events to the open panel without refetching", async () => {
    const state = seedAuthed();
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("hello")).toBeInTheDocument());
    await waitFor(() => expect(state.threadFetches).toEqual([1]));
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushThreadReplyCreated({
      channel_id: 999,
      root_message_id: 1,
      reply: {
        id: 49,
        user_id: 2,
        channel_id: 999,
        parent_id: 1,
        created_at: 1_700_000_020_000_000,
        text: "wrong channel reply",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
      thread_summary: { reply_count: 9, last_reply_created_at: 1_700_000_020_000_000 },
    });
    es.pushThreadReplyCreated({
      channel_id: 100,
      root_message_id: 2,
      reply: {
        id: 50,
        user_id: 2,
        channel_id: 100,
        parent_id: 2,
        created_at: 1_700_000_025_000_000,
        text: "other thread reply",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
      thread_summary: { reply_count: 4, last_reply_created_at: 1_700_000_025_000_000 },
    });

    await new Promise((r) => setTimeout(r, 10));
    expect(within(panel).queryByText("wrong channel reply")).toBeNull();
    expect(within(panel).queryByText("other thread reply")).toBeNull();

    es.pushThreadReplyCreated({
      channel_id: 100,
      root_message_id: 1,
      reply: {
        id: 51,
        user_id: 2,
        channel_id: 100,
        parent_id: 1,
        created_at: 1_700_000_030_000_000,
        text: "live thread reply",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
      thread_summary: { reply_count: 1, last_reply_created_at: 1_700_000_030_000_000 },
    });

    await waitFor(() => {
      expect(within(panel).getByText("live thread reply")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /open thread with 1 reply/i })).toBeInTheDocument();
      expect(state.threadFetches).toEqual([1]);
    });
  });

  test("updates channel thread summaries from thread_reply_created events", async () => {
    seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());
    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");

    es.pushThreadReplyCreated({
      channel_id: 999,
      root_message_id: 1,
      reply: {
        id: 60,
        user_id: 2,
        channel_id: 999,
        parent_id: 1,
        created_at: 1_700_000_030_000_000,
        text: "wrong channel summary reply",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
      thread_summary: { reply_count: 3, last_reply_created_at: 1_700_000_030_000_000 },
    });

    await new Promise((r) => setTimeout(r, 10));
    expect(screen.queryByRole("button", { name: /open thread with 3 replies/i })).toBeNull();

    es.pushThreadReplyCreated({
      channel_id: 100,
      root_message_id: 1,
      reply: {
        id: 61,
        user_id: 2,
        channel_id: 100,
        parent_id: 1,
        created_at: 1_700_000_040_000_000,
        text: "summary reply",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
      thread_summary: { reply_count: 2, last_reply_created_at: 1_700_000_040_000_000 },
    });

    await waitFor(() => {
      expect(
        screen.getByRole("button", {
          name: /open thread with 2 replies, last reply 2023-11-14 22:14 UTC/i,
        }),
      ).toBeInTheDocument();
    });
    expect(screen.queryByText("summary reply")).toBeNull();
  });

  test("edits, deletes, and suppresses embeds on own thread replies", async () => {
    const state = seedAuthed();
    state.messages["100"][0] = {
      ...state.messages["100"][0],
      thread_summary: { reply_count: 1, last_reply_created_at: 1_700_000_010_000_000 },
    };
    state.threadReplies["1"] = [
      {
        id: 70,
        user_id: DEV_USER.id,
        channel_id: 100,
        parent_id: 1,
        created_at: 1_700_000_010_000_000,
        text: "own reply https://example.com",
        username: DEV_USER.username,
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [
          {
            id: 700,
            message_id: 70,
            url: "https://example.com",
            title: "Example",
            description: null,
            image_url: null,
            site_name: "Example Site",
            embed_type: "link",
            iframe_url: null,
            iframe_width: null,
            iframe_height: null,
          },
        ],
      },
      {
        id: 71,
        user_id: 2,
        channel_id: 100,
        parent_id: 1,
        created_at: 1_700_000_011_000_000,
        text: "other user's reply",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
    ];

    mountAt("/channel/100?thread=1");
    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("own reply")).toBeInTheDocument());
    expect(within(panel).getByRole("link", { name: "https://example.com" })).toBeInTheDocument();
    expect(within(panel).getByText("Example")).toBeInTheDocument();
    expect(within(panel).queryAllByRole("button", { name: /edit reply/i })).toHaveLength(1);

    fireEvent.click(within(panel).getByRole("button", { name: /edit reply/i }));
    const editInput = (await within(panel).findByLabelText(/edit reply/i)) as HTMLInputElement;
    fireEvent.input(editInput, { target: { value: "edited thread reply https://example.com" } });
    fireEvent.click(within(panel).getByRole("button", { name: /^save$/i }));

    await waitFor(() => {
      expect(mswState().editedMessages).toContainEqual({
        id: 70,
        text: "edited thread reply https://example.com",
      });
      expect(within(panel).getByText("edited thread reply")).toBeInTheDocument();
      expect(within(panel).getByRole("link", { name: "https://example.com" })).toBeInTheDocument();
    });

    fireEvent.click(within(panel).getByRole("button", { name: /remove embed/i }));
    await waitFor(() => {
      expect(mswState().suppressedEmbeds).toContainEqual({ id: 70, suppress: true });
      expect(within(panel).queryByText("Example")).toBeNull();
    });

    fireEvent.click(within(panel).getByRole("button", { name: /delete reply/i }));
    const dialog = await screen.findByRole("dialog", { name: /delete reply/i });
    fireEvent.click(within(dialog).getByRole("button", { name: /^delete$/i }));

    await waitFor(() => {
      expect(mswState().deletedMessageIds).toContain(70);
      expect(within(panel).queryByText("edited thread reply")).toBeNull();
      expect(within(panel).getByText("other user's reply")).toBeInTheDocument();
    });
  });

  test("renders tombstoned roots in the channel and thread panel", async () => {
    const state = seedAuthed();
    state.messages["100"][0] = {
      ...state.messages["100"][0],
      text: "",
      deleted_at: 1_700_000_050_000_000,
      suppress_embeds: true,
      reactions: [{ kind: "native", emoji: "👍", count: 2, me_reacted: true }],
      thread_summary: { reply_count: 2, last_reply_created_at: 1_700_000_061_000_000 },
    };
    state.threadReplies["1"] = [
      {
        id: 80,
        user_id: 2,
        channel_id: 100,
        parent_id: 1,
        created_at: 1_700_000_060_000_000,
        text: "reply under tombstone",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
      {
        id: 81,
        user_id: 2,
        channel_id: 100,
        parent_id: 1,
        created_at: 1_700_000_061_000_000,
        deleted_at: 1_700_000_062_000_000,
        text: "",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: true,
        mentions: [],
        attachments: [],
        embeds: [],
        reactions: [{ kind: "native", emoji: "❤️", count: 1, me_reacted: false }],
      },
    ];

    mountAt("/channel/100");
    await waitFor(() =>
      expect(screen.getByLabelText(/original message deleted/i)).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /open thread with 2 replies/i }));

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => {
      expect(within(panel).getAllByLabelText(/original message deleted/i)).toHaveLength(2);
      expect(within(panel).getByText("reply under tombstone")).toBeInTheDocument();
    });
    expect(screen.queryByRole("button", { name: /👍 2 reactions/i })).toBeNull();
    expect(within(panel).queryByRole("button", { name: /❤️ 1 reaction/i })).toBeNull();
    const tombstones = within(panel).getAllByLabelText(/original message deleted/i);
    const deletedReplyRow = assertExists(tombstones[1].closest("article"), "deleted reply row");
    expect(within(deletedReplyRow as HTMLElement).queryByRole("button")).toBeNull();
  });

  test("thread reply delete events remove replies and recalculate channel summaries", async () => {
    const state = seedAuthed();
    state.messages["100"][0] = {
      ...state.messages["100"][0],
      thread_summary: { reply_count: 2, last_reply_created_at: 1_700_000_020_000_000 },
    };
    state.threadReplies["1"] = [
      {
        id: 90,
        user_id: 2,
        channel_id: 100,
        parent_id: 1,
        created_at: 1_700_000_010_000_000,
        text: "older live reply",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [makeAttachment({ id: 9801, message_id: 90 })],
        embeds: [],
      },
      {
        id: 91,
        user_id: 2,
        channel_id: 100,
        parent_id: 1,
        created_at: 1_700_000_020_000_000,
        text: "newer live reply",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [makeAttachment({ id: 9802, message_id: 91 })],
        embeds: [],
      },
    ];

    mountAt("/channel/100?thread=1");
    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("newer live reply")).toBeInTheDocument());
    expect(within(panel).getAllByRole("img", { name: /photo attachment from bob/i })).toHaveLength(
      2,
    );
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());
    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");

    es.pushThreadReplyDeleted({
      channel_id: 100,
      root_message_id: 1,
      reply_id: 91,
      thread_summary: { reply_count: 1, last_reply_created_at: 1_700_000_010_000_000 },
    });

    await waitFor(() => {
      expect(within(panel).queryByText("newer live reply")).toBeNull();
      expect(
        within(panel).getAllByRole("img", { name: /photo attachment from bob/i }),
      ).toHaveLength(1);
      expect(screen.getByRole("button", { name: /open thread with 1 reply/i })).toBeInTheDocument();
    });

    es.pushThreadReplyDeleted({
      channel_id: 100,
      root_message_id: 1,
      reply_id: 90,
      thread_summary: null,
    });

    await waitFor(() => {
      expect(within(panel).queryByText("older live reply")).toBeNull();
      expect(within(panel).queryByRole("img", { name: /photo attachment from bob/i })).toBeNull();
      expect(screen.queryByRole("button", { name: /open thread with/i })).toBeNull();
    });
  });

  test("Shift+Enter inserts a newline and Enter submits exact multiline text", async () => {
    seedAuthed();
    mountAt("/channel/100");

    const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
    fireEvent.input(input, { target: { value: "first line" } });
    setInputSelection(input, "first line".length);

    fireEvent.keyDown(input, { key: "Enter", shiftKey: true });

    await waitFor(() => {
      expect(input.value).toBe("first line\n");
      expect(input.selectionStart).toBe("first line\n".length);
    });

    fireEvent.input(input, { target: { value: "first line\nsecond line" } });
    setInputSelection(input, "first line\nsecond line".length);
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => {
      expect(mswState().sentMessages).toContainEqual({
        channel: "100",
        text: "first line\nsecond line",
      });
    });
    await waitFor(() => {
      expect(input.value).toBe("");
      expect(document.activeElement).toBe(input);
    });
  });

  test("channel composer searches, chips, serializes, and renders user mentions", async () => {
    const bob: PublicUser = {
      id: 2,
      username: "bob",
      display_name: "Bobby",
      avatar_url: null,
    };
    const state = seedAuthed();
    state.users = [...state.users, bob];
    state.messages["100"] = [
      ...state.messages["100"],
      {
        id: 9,
        user_id: 1,
        channel_id: 100,
        text: "primed <@2>",
        username: "alice",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [bob],
        attachments: [],
        embeds: [],
      },
    ];
    mountAt("/channel/100");

    const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
    await screen.findByText("@Bobby");

    inputFromUser(input, "<@2>");
    await waitFor(() => {
      expect(within(input).getByText("@Bobby")).toHaveClass("bg-blue-100", "text-blue-800");
    });

    inputFromUser(input, "@bo");
    const listbox = await screen.findByRole("listbox", { name: /mention suggestions/i });
    const bobOption = within(listbox).getByRole("option", { name: /mention bobby @bob/i });
    expect(bobOption).toHaveAttribute("aria-selected", "true");
    expect(state.userSearchRequests).toContainEqual({ query: "bo", limit: 8 });

    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => {
      expect(input.value).toBe("<@2> ");
      expect(within(input).getByText("@Bobby")).toBeInTheDocument();
    });
    expect(state.sentMessages).toEqual([]);

    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => {
      expect(state.sentMessages).toContainEqual({ channel: "100", text: "<@2> " });
      expect(input.value).toBe("");
    });

    const created = state.messages["100"].at(-1);
    expect(created?.mentions).toEqual([bob]);
    if (created) latestFakeEventSource()?.pushMessage(created);

    await waitFor(() => {
      expect(screen.getAllByText("@Bobby").length).toBeGreaterThanOrEqual(2);
    });
  });

  test("Send button submits the same exact multiline draft and closes the emoji picker", async () => {
    seedAuthed();
    mountAt("/channel/100");

    const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
    const text = "button first line\nbutton second line";
    fireEvent.input(input, { target: { value: text } });
    fireEvent.click(screen.getByRole("button", { name: /open emoji picker/i }));
    await screen.findByRole("dialog", { name: /emoji picker/i });

    fireEvent.click(screen.getByRole("button", { name: /^send$/i }));

    await waitFor(() => {
      expect(mswState().sentMessages).toContainEqual({ channel: "100", text });
      expect(input.value).toBe("");
      expect(screen.queryByRole("dialog", { name: /emoji picker/i })).toBeNull();
      expect(document.activeElement).toBe(input);
    });
  });

  test("empty channel drafts keep Send disabled and do not submit", async () => {
    seedAuthed();
    mountAt("/channel/100");

    const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
    const sendButton = screen.getByRole("button", { name: /^send$/i });
    expect(sendButton).toBeDisabled();

    fireEvent.submit(assertExists(input.closest("form"), "form"));
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(mswState().sentMessages).toEqual([]);

    fireEvent.input(input, { target: { value: "text draft" } });
    await waitFor(() => expect(sendButton).not.toBeDisabled());

    fireEvent.input(input, { target: { value: "" } });
    await waitFor(() => expect(sendButton).toBeDisabled());
  });

  test("selected channel photos preview, persist while typing, and can be removed", async () => {
    const urls = mockObjectUrls();
    const state = seedAuthed();
    const { unmount } = mountAt("/channel/100");

    try {
      const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
      const sendButton = screen.getByRole("button", { name: /^send$/i });
      expect(sendButton).toBeDisabled();

      fireEvent.change(fileInput(), { target: { files: [photoFile("cat.png")] } });

      expect(urls.createObjectURL).toHaveBeenCalledTimes(1);
      expect(screen.getByRole("img", { name: /selected photo 1: cat\.png/i })).toHaveAttribute(
        "src",
        "blob:cat.png-0",
      );
      expect(sendButton).not.toBeDisabled();
      expect(state.typingPings).toEqual([]);

      fireEvent.input(input, { target: { value: "caption while preview remains" } });
      await waitFor(() => expect(state.typingPings).toEqual(["100"]));
      expect(screen.getByRole("img", { name: /selected photo 1: cat\.png/i })).toBeInTheDocument();

      fireEvent.click(screen.getByRole("button", { name: /remove selected photo 1: cat\.png/i }));

      expect(urls.revokeObjectURL).toHaveBeenCalledWith("blob:cat.png-0");
      expect(screen.queryByRole("img", { name: /cat\.png/i })).toBeNull();
      expect(sendButton).not.toBeDisabled();
    } finally {
      unmount();
      urls.restore();
    }
  });

  test("photo-only channel drafts submit as multipart, clear previews, revoke URLs, and refocus", async () => {
    const urls = mockObjectUrls();
    const state = seedAuthed();
    const { unmount } = mountAt("/channel/100");

    try {
      const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
      const sendButton = screen.getByRole("button", { name: /^send$/i });

      const catPhoto = photoFile("cat.png");
      fireEvent.change(fileInput(), { target: { files: [catPhoto] } });
      await waitFor(() => expect(sendButton).not.toBeDisabled());
      fireEvent.click(sendButton);

      await waitFor(() => {
        expect(state.sentMessagePhotos).toContainEqual({
          channel: "100",
          text: "",
          photos: [{ name: "cat.png", size: catPhoto.size, type: "image/png" }],
        });
        expect(screen.queryByRole("img", { name: /cat\.png/i })).toBeNull();
        expect(urls.revokeObjectURL).toHaveBeenCalledWith("blob:cat.png-0");
        expect(input.value).toBe("");
        expect(document.activeElement).toBe(input);
      });
    } finally {
      unmount();
      urls.restore();
    }
  });

  test("photo captions submit mention markers and render hydrated SSE mentions with attachments", async () => {
    const urls = mockObjectUrls();
    const state = seedAuthed();
    state.users.push({ id: 2, username: "bob", display_name: "Bobby Tables", avatar_url: null });
    const { unmount } = mountAt("/channel/100");

    try {
      const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
      const photo = photoFile("mention-caption.png");
      const text = "caption <@2>";
      fireEvent.input(input, { target: { value: text } });
      fireEvent.change(fileInput(), { target: { files: [photo] } });
      fireEvent.click(screen.getByRole("button", { name: /^send$/i }));

      await waitFor(() => {
        expect(state.sentMessagePhotos).toContainEqual({
          channel: "100",
          text,
          photos: [{ name: "mention-caption.png", size: photo.size, type: "image/png" }],
        });
        expect(screen.queryByRole("img", { name: /mention-caption\.png/i })).toBeNull();
        expect(urls.revokeObjectURL).toHaveBeenCalledWith("blob:mention-caption.png-0");
        expect(input.value).toBe("");
        expect(document.activeElement).toBe(input);
      });

      const created = assertExists(
        state.messages["100"].find((message) => message.text === text),
        "created photo caption mention",
      );
      expect(created.mentions).toEqual([
        { id: 2, username: "bob", display_name: "Bobby Tables", avatar_url: null },
      ]);
      await waitFor(() => expect(latestFakeEventSource()).toBeDefined());
      assertExists(latestFakeEventSource(), "latestFakeEventSource").pushMessage(created);

      const mention = await screen.findByText("@Bobby Tables");
      expect(mention).toHaveAttribute("title", "@bob");
      const messageText = assertExists(
        mention.closest(".whitespace-pre-wrap"),
        "photo caption mention text",
      );
      expect(messageText).toHaveTextContent("caption @Bobby Tables");
      expect(messageText).not.toHaveTextContent("<@2>");
      expect(screen.getByRole("img", { name: /photo attachment from baipas/i })).toHaveAttribute(
        "src",
        expect.stringContaining("/attachments/"),
      );
      expect(
        screen.getByRole("button", { name: /open photo attachment from baipas/i }),
      ).toBeEnabled();
    } finally {
      unmount();
      urls.restore();
    }
  });

  test("failed photo sends keep selected photos and object URLs for retry", async () => {
    const urls = mockObjectUrls();
    seedAuthed();
    server.use(
      http.post(`${TEST_SERVER}/message/:id`, () => new HttpResponse(null, { status: 500 })),
    );
    const { unmount } = mountAt("/channel/100");

    try {
      const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
      fireEvent.input(input, { target: { value: "retry caption" } });
      fireEvent.change(fileInput(), { target: { files: [photoFile("retry.webp", "image/webp")] } });

      fireEvent.click(screen.getByRole("button", { name: /^send$/i }));

      await waitFor(() => {
        expect(screen.getByRole("img", { name: /selected photo 1: retry\.webp/i })).toHaveAttribute(
          "src",
          "blob:retry.webp-0",
        );
        expect(input.value).toBe("retry caption");
        expect(screen.getByRole("button", { name: /^send$/i })).not.toBeDisabled();
      });
      expect(urls.revokeObjectURL).not.toHaveBeenCalled();
    } finally {
      unmount();
      urls.restore();
    }
  });

  test("inline reply target selection preserves selected photos and shows attachment-only previews", async () => {
    const urls = mockObjectUrls();
    const user = userEvent.setup();
    const state = seedAuthed();
    state.messages["100"][1] = {
      ...state.messages["100"][1],
      text: "",
      attachments: [makeAttachment({ id: 8101, message_id: 2 })],
    };
    const { container, unmount } = mountAt("/channel/100");

    try {
      const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
      fireEvent.input(input, { target: { value: "draft with photo" } });
      fireEvent.change(fileInput(), { target: { files: [photoFile("keep.png")] } });
      await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());

      const aliceReplyButton = screen.getByRole("button", {
        name: /reply inline to message by alice: hello/i,
      });
      aliceReplyButton.focus();
      await user.keyboard("{Enter}");
      let banner = await screen.findByLabelText(/inline reply target/i);
      expect(banner).toHaveAccessibleName(/inline reply target: replying to alice: hello/i);
      expect(input).toHaveAttribute("aria-describedby", banner.id);
      expect(within(banner).getByText("hello")).toBeInTheDocument();
      expect(screen.getByRole("img", { name: /selected photo 1: keep\.png/i })).toHaveAttribute(
        "src",
        "blob:keep.png-0",
      );
      expect(input.value).toBe("draft with photo");

      fireEvent.click(screen.getByRole("button", { name: /reply inline to message by bob/i }));
      banner = screen.getByLabelText(/inline reply target/i);
      expect(banner).toHaveAccessibleName(/inline reply target: replying to bob: attachment/i);
      expect(within(banner).getByText(/replying to bob/i)).toBeInTheDocument();
      expect(within(banner).getByText("Attachment")).toBeInTheDocument();
      expect(within(banner).queryByText("hello")).toBeNull();
      expect(screen.getByRole("img", { name: /selected photo 1: keep\.png/i })).toBeInTheDocument();
      expect(input.value).toBe("draft with photo");

      await expectNoA11yViolations(container, "inline reply composer banner");

      const dismissButton = within(banner).getByRole("button", {
        name: /dismiss inline reply to message by bob: attachment/i,
      });
      dismissButton.focus();
      await user.keyboard("{Enter}");
      await waitFor(() => expect(screen.queryByLabelText(/inline reply target/i)).toBeNull());
      expect(screen.getByRole("img", { name: /selected photo 1: keep\.png/i })).toBeInTheDocument();
      expect(input.value).toBe("draft with photo");
      await waitFor(() => expect(document.activeElement).toBe(input));
      expect(urls.revokeObjectURL).not.toHaveBeenCalled();
    } finally {
      unmount();
      urls.restore();
    }
  });

  test("successful photo inline replies clear composer state, refocus, and render SSE previews", async () => {
    const urls = mockObjectUrls();
    const state = seedAuthed();
    const { unmount } = mountAt("/channel/100");

    try {
      const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
      const photo = photoFile("inline-success.png");
      fireEvent.change(fileInput(), { target: { files: [photo] } });
      await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
      fireEvent.click(screen.getByRole("button", { name: /reply inline to message by alice/i }));
      await screen.findByLabelText(/inline reply target/i);

      fireEvent.click(screen.getByRole("button", { name: /^send$/i }));

      await waitFor(() => {
        expect(state.sentInlineReplies).toContainEqual({
          channel: "100",
          text: "",
          replyToMessageId: 1,
        });
        expect(state.sentMessagePhotos).toContainEqual({
          channel: "100",
          text: "",
          photos: [{ name: "inline-success.png", size: photo.size, type: "image/png" }],
        });
        expect(screen.queryByLabelText(/inline reply target/i)).toBeNull();
        expect(screen.queryByRole("img", { name: /inline-success\.png/i })).toBeNull();
        expect(urls.revokeObjectURL).toHaveBeenCalledWith("blob:inline-success.png-0");
        expect(input.value).toBe("");
        expect(document.activeElement).toBe(input);
      });

      await waitFor(() => expect(latestFakeEventSource()).toBeDefined());
      const created = assertExists(
        state.messages["100"].find((message) => message.reply_to_message_id === 1),
        "created photo inline reply",
      );
      assertExists(latestFakeEventSource(), "latestFakeEventSource").pushMessage(created);

      const preview = await screen.findByLabelText(/replying to alice/i);
      expect(preview).toHaveTextContent("hello");
      expect(screen.getByRole("img", { name: /photo attachment from baipas/i })).toHaveAttribute(
        "src",
        expect.stringContaining("/attachments/"),
      );
    } finally {
      unmount();
      urls.restore();
    }
  });

  test("failed photo inline replies preserve target, draft, and photos", async () => {
    const urls = mockObjectUrls();
    const state = seedAuthed();
    server.use(
      http.post(`${TEST_SERVER}/message/:id`, async ({ request }) => {
        const form = await request.formData();
        const rawText = form.get("text");
        const rawReplyTarget = form.get("reply_to_message_id");
        const file = form.get("photos");
        state.sentInlineReplies.push({
          channel: "100",
          text: typeof rawText === "string" ? rawText : "",
          replyToMessageId: typeof rawReplyTarget === "string" ? Number(rawReplyTarget) : 0,
        });
        state.sentMessagePhotos.push({
          channel: "100",
          text: typeof rawText === "string" ? rawText : "",
          photos:
            file instanceof File ? [{ name: file.name, size: file.size, type: file.type }] : [],
        });
        return new HttpResponse(null, { status: 500 });
      }),
    );
    const { unmount } = mountAt("/channel/100");

    try {
      const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
      fireEvent.input(input, { target: { value: "retry inline caption" } });
      fireEvent.change(fileInput(), {
        target: { files: [photoFile("retry-inline.webp", "image/webp")] },
      });
      await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
      fireEvent.click(screen.getByRole("button", { name: /reply inline to message by alice/i }));
      const banner = await screen.findByLabelText(/inline reply target/i);

      fireEvent.click(screen.getByRole("button", { name: /^send$/i }));

      await waitFor(() => {
        expect(state.sentInlineReplies).toContainEqual({
          channel: "100",
          text: "retry inline caption",
          replyToMessageId: 1,
        });
        expect(state.sentMessagePhotos).toContainEqual({
          channel: "100",
          text: "retry inline caption",
          photos: [
            {
              name: "retry-inline.webp",
              size: photoFile("retry-inline.webp", "image/webp").size,
              type: "image/webp",
            },
          ],
        });
        expect(screen.getByLabelText(/inline reply target/i)).toBe(banner);
        expect(
          screen.getByRole("img", { name: /selected photo 1: retry-inline\.webp/i }),
        ).toHaveAttribute("src", "blob:retry-inline.webp-0");
        expect(input.value).toBe("retry inline caption");
        expect(screen.getByRole("button", { name: /^send$/i })).not.toBeDisabled();
      });
      expect(urls.revokeObjectURL).not.toHaveBeenCalled();
    } finally {
      unmount();
      urls.restore();
    }
  });

  test("channel switching clears inline reply target without clearing draft or selected photos", async () => {
    const urls = mockObjectUrls();
    const state = seedAuthed();
    state.channels.push({ id: 200, name: "random", position: 1, type: "text" });
    state.messages["200"] = [
      {
        id: 2001,
        user_id: 2,
        channel_id: 200,
        text: "different channel",
        username: "carol",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
    ];
    const { history, unmount } = mountAt("/channel/100");

    try {
      const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
      fireEvent.input(input, { target: { value: "carry this draft" } });
      fireEvent.change(fileInput(), { target: { files: [photoFile("carry.png")] } });
      await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
      fireEvent.click(screen.getByRole("button", { name: /reply inline to message by alice/i }));
      await screen.findByLabelText(/inline reply target/i);

      history.set({ value: "/channel/200" });

      await waitFor(() => {
        expect(screen.getByText("different channel")).toBeInTheDocument();
        expect(screen.queryByLabelText(/inline reply target/i)).toBeNull();
        expect(input.value).toBe("carry this draft");
        expect(screen.getByRole("img", { name: /selected photo 1: carry\.png/i })).toHaveAttribute(
          "src",
          "blob:carry.png-0",
        );
      });
      expect(urls.revokeObjectURL).not.toHaveBeenCalled();
    } finally {
      unmount();
      urls.restore();
    }
  });

  test("live target deletion and tombstone updates clear inline reply target without clearing draft or photos", async () => {
    const urls = mockObjectUrls();
    const state = seedAuthed();
    const { unmount } = mountAt("/channel/100");

    try {
      const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
      fireEvent.input(input, { target: { value: "keep after deletion" } });
      fireEvent.change(fileInput(), { target: { files: [photoFile("delete-keep.png")] } });
      await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
      await waitFor(() => expect(latestFakeEventSource()).toBeDefined());
      const eventSource = assertExists(latestFakeEventSource(), "latestFakeEventSource");

      fireEvent.click(screen.getByRole("button", { name: /reply inline to message by alice/i }));
      await screen.findByLabelText(/inline reply target/i);

      eventSource.pushMessageDeleted({ id: 1, channel_id: 100 });

      await waitFor(() => {
        expect(screen.queryByLabelText(/inline reply target/i)).toBeNull();
        expect(input.value).toBe("keep after deletion");
        expect(
          screen.getByRole("img", { name: /selected photo 1: delete-keep\.png/i }),
        ).toHaveAttribute("src", "blob:delete-keep.png-0");
      });

      fireEvent.click(screen.getByRole("button", { name: /reply inline to message by bob/i }));
      await screen.findByLabelText(/inline reply target/i);

      eventSource.pushMessageUpdated({
        ...state.messages["100"][1],
        deleted_at: 1_700_000_100_000_000,
      });

      await waitFor(() => {
        expect(screen.queryByLabelText(/inline reply target/i)).toBeNull();
        expect(screen.getByLabelText(/original message deleted/i)).toBeInTheDocument();
        expect(input.value).toBe("keep after deletion");
        expect(
          screen.getByRole("img", { name: /selected photo 1: delete-keep\.png/i }),
        ).toHaveAttribute("src", "blob:delete-keep.png-0");
      });
      expect(urls.revokeObjectURL).not.toHaveBeenCalled();
    } finally {
      unmount();
      urls.restore();
    }
  });

  test("thread reply photos preview, remove, and submit photo-only multipart", async () => {
    const urls = mockObjectUrls();
    const state = seedAuthed();
    const { container, unmount } = mountAt("/channel/100?thread=1");

    try {
      const panel = await screen.findByRole("complementary", { name: /thread panel/i });
      await waitFor(() => expect(within(panel).getByText("hello")).toBeInTheDocument());
      const input = (await within(panel).findByLabelText(/thread reply/i)) as HTMLInputElement;
      const sendButton = within(panel).getByRole("button", { name: /^send$/i });
      expect(sendButton).toBeDisabled();

      fireEvent.change(fileInputWithin(panel), { target: { files: [photoFile("remove-me.png")] } });
      expect(urls.createObjectURL).toHaveBeenCalledTimes(1);
      expect(
        within(panel).getByRole("img", { name: /selected photo 1: remove-me\.png/i }),
      ).toHaveAttribute("src", "blob:remove-me.png-0");
      expect(sendButton).not.toBeDisabled();

      fireEvent.click(
        within(panel).getByRole("button", { name: /remove selected photo 1: remove-me\.png/i }),
      );
      expect(urls.revokeObjectURL).toHaveBeenCalledWith("blob:remove-me.png-0");
      expect(within(panel).queryByRole("img", { name: /remove-me\.png/i })).toBeNull();
      expect(sendButton).toBeDisabled();

      const catPhoto = photoFile("thread-cat.png");
      fireEvent.change(fileInputWithin(panel), { target: { files: [catPhoto] } });
      await waitFor(() => expect(sendButton).not.toBeDisabled());
      fireEvent.click(sendButton);

      await waitFor(() => {
        expect(state.sentThreadReplyPhotos).toContainEqual({
          rootId: 1,
          text: "",
          photos: [{ name: "thread-cat.png", size: catPhoto.size, type: "image/png" }],
        });
        expect(within(panel).queryByRole("img", { name: /thread-cat\.png/i })).toBeNull();
        expect(urls.revokeObjectURL).toHaveBeenCalledWith("blob:thread-cat.png-1");
        expect(input.value).toBe("");
        expect(document.activeElement).toBe(input);
      });
      expect(
        within(panel)
          .getByRole("img", { name: /photo attachment from baipas/i })
          .getAttribute("src"),
      ).toContain("/attachments/");

      await expectNoA11yViolations(container, "thread photo reply composer");
    } finally {
      unmount();
      urls.restore();
    }
  });

  test("failed thread photo sends keep selected photos and text for retry", async () => {
    const urls = mockObjectUrls();
    const state = seedAuthed();
    server.use(
      http.post(`${TEST_SERVER}/thread/1/reply`, async ({ request }) => {
        const form = await request.formData();
        const rawText = form.get("text");
        const file = form.get("photos");
        state.sentThreadReplyPhotos.push({
          rootId: 1,
          text: typeof rawText === "string" ? rawText : "",
          photos:
            file instanceof File ? [{ name: file.name, size: file.size, type: file.type }] : [],
        });
        return new HttpResponse(null, { status: 500 });
      }),
    );
    const { unmount } = mountAt("/channel/100?thread=1");

    try {
      const panel = await screen.findByRole("complementary", { name: /thread panel/i });
      await waitFor(() => expect(within(panel).getByText("hello")).toBeInTheDocument());
      const input = (await within(panel).findByLabelText(/thread reply/i)) as HTMLInputElement;
      fireEvent.input(input, { target: { value: "retry thread caption" } });
      fireEvent.change(fileInputWithin(panel), {
        target: { files: [photoFile("retry-thread.webp", "image/webp")] },
      });

      fireEvent.click(within(panel).getByRole("button", { name: /^send$/i }));

      await waitFor(() => {
        expect(state.sentThreadReplyPhotos).toContainEqual({
          rootId: 1,
          text: "retry thread caption",
          photos: [
            {
              name: "retry-thread.webp",
              size: photoFile("retry-thread.webp", "image/webp").size,
              type: "image/webp",
            },
          ],
        });
        expect(
          within(panel).getByRole("img", { name: /selected photo 1: retry-thread\.webp/i }),
        ).toHaveAttribute("src", "blob:retry-thread.webp-0");
        expect(input.value).toBe("retry thread caption");
        expect(within(panel).getByRole("alert")).toHaveTextContent("Thread reply failed (500)");
        expect(within(panel).getByRole("button", { name: /^send$/i })).not.toBeDisabled();
      });
      expect(urls.revokeObjectURL).not.toHaveBeenCalled();
    } finally {
      unmount();
      urls.restore();
    }
  });

  test("typing pings remain throttled while composing multiline drafts", async () => {
    seedAuthed();
    mountAt("/channel/100");

    const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
    fireEvent.input(input, { target: { value: "first line" } });

    await waitFor(() => {
      expect(mswState().typingPings).toEqual(["100"]);
    });

    setInputSelection(input, "first line".length);
    fireEvent.keyDown(input, { key: "Enter", shiftKey: true });
    await waitFor(() => expect(input.value).toBe("first line\n"));

    fireEvent.input(input, { target: { value: "first line\nsecond line" } });
    await new Promise((resolve) => setTimeout(resolve, 10));

    expect(mswState().typingPings).toEqual(["100"]);
  });

  test("POSTs to /message/:id when the form is submitted", async () => {
    seedAuthed();
    mountAt("/channel/100");

    const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
    expect(screen.getByLabelText(/new message/i)).toBe(input);
    fireEvent.input(input, { target: { value: "typed message" } });
    const form = assertExists(input.closest("form"), "form");
    fireEvent.submit(form);

    await waitFor(() => {
      expect(mswState().sentMessages).toContainEqual({
        channel: "100",
        text: "typed message",
      });
    });
    await waitFor(() => expect(input.value).toBe(""));
  });

  test("selects an inline reply target, preserves the draft, and renders the SSE reply preview", async () => {
    const state = seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
    const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
    fireEvent.input(input, { target: { value: "draft reply body" } });

    fireEvent.click(screen.getByRole("button", { name: /reply inline to message by alice/i }));
    let banner = await screen.findByLabelText(/inline reply target/i);
    expect(within(banner).getByText(/replying to alice/i)).toBeInTheDocument();
    expect(within(banner).getByText("hello")).toBeInTheDocument();
    expect(input.value).toBe("draft reply body");

    fireEvent.click(screen.getByRole("button", { name: /reply inline to message by bob/i }));
    banner = screen.getByLabelText(/inline reply target/i);
    expect(within(banner).getByText(/replying to bob/i)).toBeInTheDocument();
    expect(within(banner).getByText("world")).toBeInTheDocument();
    expect(within(banner).queryByText("hello")).toBeNull();
    expect(input.value).toBe("draft reply body");

    fireEvent.click(within(banner).getByRole("button", { name: /dismiss inline reply/i }));
    expect(screen.queryByLabelText(/inline reply target/i)).toBeNull();
    expect(input.value).toBe("draft reply body");

    fireEvent.click(screen.getByRole("button", { name: /reply inline to message by alice/i }));
    fireEvent.submit(assertExists(input.closest("form"), "form"));

    await waitFor(() => {
      expect(state.sentInlineReplies).toContainEqual({
        channel: "100",
        text: "draft reply body",
        replyToMessageId: 1,
      });
      expect(input.value).toBe("");
      expect(screen.queryByLabelText(/inline reply target/i)).toBeNull();
    });

    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());
    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessage({
      id: 777,
      user_id: DEV_USER.id,
      channel_id: 100,
      parent_id: null,
      created_at: 1_700_000_050_000_000,
      reply_to_message_id: 1,
      reply_to: {
        id: 1,
        user_id: 1,
        channel_id: 100,
        created_at: 1_700_000_000_000_000,
        text: "hello",
        username: "alice",
        display_name: null,
        avatar_url: null,
      },
      text: "draft reply body",
      username: DEV_USER.username,
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [],
      attachments: [],
      embeds: [],
    });

    const preview = await screen.findByLabelText(/replying to alice/i);
    const body = await findRenderedMessageText("draft reply body");
    expect(preview).toHaveTextContent("hello");
    expect(preview.compareDocumentPosition(body) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.queryByRole("complementary", { name: /thread panel/i })).toBeNull();
  });

  test("message_updated SSE preserves an edited inline reply preview while replacing the message", async () => {
    const state = seedAuthed();
    state.messages["100"] = [
      {
        id: 21,
        user_id: 2,
        channel_id: 100,
        parent_id: null,
        reply_to_message_id: null,
        reply_to: null,
        created_at: 1_700_000_000_000_000,
        text: "original target for edit",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
      {
        id: 22,
        user_id: DEV_USER.id,
        channel_id: 100,
        parent_id: null,
        reply_to_message_id: 21,
        created_at: 1_700_000_001_000_000,
        text: "inline before live edit",
        username: DEV_USER.username,
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
        reply_to: {
          id: 21,
          user_id: 2,
          channel_id: 100,
          created_at: 1_700_000_000_000_000,
          deleted_at: null,
          text: "original target for edit",
          attachment_count: 0,
          username: "bob",
          display_name: null,
          avatar_url: null,
        },
      },
    ];
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("inline before live edit")).toBeInTheDocument());
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessageUpdated({
      id: 22,
      user_id: DEV_USER.id,
      channel_id: 100,
      parent_id: null,
      reply_to_message_id: 21,
      created_at: 1_700_000_001_000_000,
      text: "inline after live edit",
      username: DEV_USER.username,
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [],
      attachments: [],
      embeds: [],
      reply_to: {
        id: 21,
        user_id: 2,
        channel_id: 100,
        created_at: 1_700_000_000_000_000,
        deleted_at: null,
        text: "original target for edit",
        attachment_count: 0,
        username: "bob",
        display_name: null,
        avatar_url: null,
      },
    });

    await waitFor(() => {
      expect(screen.getByText("inline after live edit")).toBeInTheDocument();
      expect(screen.queryByText("inline before live edit")).toBeNull();
      expect(screen.getByLabelText(/replying to bob/i)).toHaveTextContent(
        "original target for edit",
      );
    });
  });

  test("message_updated and message_deleted SSE patch visible inline reply references", async () => {
    const state = seedAuthed();
    state.messages["100"] = [
      {
        id: 31,
        user_id: 2,
        channel_id: 100,
        parent_id: null,
        reply_to_message_id: null,
        reply_to: null,
        created_at: 1_700_000_000_000_000,
        text: "target before live patch",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
      {
        id: 32,
        user_id: DEV_USER.id,
        channel_id: 100,
        parent_id: null,
        reply_to_message_id: 31,
        created_at: 1_700_000_001_000_000,
        text: "reply watching target",
        username: DEV_USER.username,
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
        reply_to: {
          id: 31,
          user_id: 2,
          channel_id: 100,
          created_at: 1_700_000_000_000_000,
          deleted_at: null,
          text: "target before live patch",
          attachment_count: 0,
          username: "bob",
          display_name: null,
          avatar_url: null,
        },
      },
    ];
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("reply watching target")).toBeInTheDocument());
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());
    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");

    es.pushMessageUpdated({
      ...state.messages["100"][0],
      text: "target after live patch",
    });

    await waitFor(() => {
      expect(screen.getByLabelText(/replying to bob/i)).toHaveTextContent(
        "target after live patch",
      );
    });

    es.pushMessageUpdated({
      ...state.messages["100"][0],
      text: "",
      deleted_at: 1_700_000_002_000_000,
      suppress_embeds: true,
      mentions: [],
      attachments: [],
      embeds: [],
      reactions: [],
    });

    await waitFor(() => {
      expect(screen.getByLabelText(/replying to deleted message by bob/i)).toHaveTextContent(
        "Original message deleted",
      );
    });

    es.pushMessageDeleted({ id: 31, channel_id: 100 });

    await waitFor(() => {
      expect(screen.getByLabelText(/replying to unavailable message 31/i)).toHaveTextContent(
        "Original message unavailable",
      );
      expect(screen.getByText("reply watching target")).toBeInTheDocument();
    });
  });

  test("thread panel patches root reply previews from referenced target updates", async () => {
    const state = seedAuthed();
    state.messages["100"] = [
      {
        id: 41,
        user_id: 2,
        channel_id: 100,
        parent_id: null,
        reply_to_message_id: null,
        reply_to: null,
        created_at: 1_700_000_000_000_000,
        text: "thread target before edit",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
      {
        id: 42,
        user_id: DEV_USER.id,
        channel_id: 100,
        parent_id: null,
        reply_to_message_id: 41,
        created_at: 1_700_000_001_000_000,
        text: "inline root with open thread",
        username: DEV_USER.username,
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
        thread_summary: { reply_count: 1, last_reply_created_at: 1_700_000_002_000_000 },
        reply_to: {
          id: 41,
          user_id: 2,
          channel_id: 100,
          created_at: 1_700_000_000_000_000,
          deleted_at: null,
          text: "thread target before edit",
          attachment_count: 0,
          username: "bob",
          display_name: null,
          avatar_url: null,
        },
      },
    ];
    state.threadReplies["42"] = [
      {
        id: 43,
        user_id: 2,
        channel_id: 100,
        parent_id: 42,
        reply_to_message_id: null,
        reply_to: null,
        created_at: 1_700_000_002_000_000,
        text: "thread reply under inline root",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
    ];
    mountAt("/channel/100?thread=42");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() =>
      expect(within(panel).getByText("thread reply under inline root")).toBeInTheDocument(),
    );
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());
    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");

    es.pushMessageUpdated({
      ...state.messages["100"][0],
      text: "thread target after edit",
    });

    await waitFor(() => {
      expect(within(panel).getByLabelText(/replying to bob/i)).toHaveTextContent(
        "thread target after edit",
      );
    });

    es.pushMessageUpdated({
      ...state.messages["100"][0],
      text: "",
      deleted_at: 1_700_000_003_000_000,
      suppress_embeds: true,
      mentions: [],
      attachments: [],
      embeds: [],
      reactions: [],
    });

    await waitFor(() => {
      expect(within(panel).getByLabelText(/replying to deleted message by bob/i)).toHaveTextContent(
        "Original message deleted",
      );
      expect(within(panel).getByText("thread reply under inline root")).toBeInTheDocument();
    });
  });

  test("channel history reload renders inline replies as chronological top-level messages", async () => {
    const state = seedAuthed();
    state.messages["100"] = [
      {
        id: 21,
        user_id: 2,
        channel_id: 100,
        parent_id: null,
        created_at: 1_700_000_000_000_000,
        text: "original history message",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
      {
        id: 22,
        user_id: DEV_USER.id,
        channel_id: 100,
        parent_id: null,
        reply_to_message_id: 21,
        created_at: 1_700_000_001_000_000,
        text: "history inline reply",
        username: DEV_USER.username,
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
        reply_to: {
          id: 21,
          user_id: 2,
          channel_id: 100,
          created_at: 1_700_000_000_000_000,
          text: "original history message",
          username: "bob",
          display_name: null,
          avatar_url: null,
        },
      },
    ];
    mountAt("/channel/100");

    const original = await findRenderedMessageText("original history message");
    const reply = await findRenderedMessageText("history inline reply");
    const preview = screen.getByLabelText(/replying to bob/i);

    expect(original.compareDocumentPosition(reply) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(preview).toHaveTextContent("original history message");
    expect(preview.compareDocumentPosition(reply) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  test("channel history reload renders tombstoned and hard-deleted reply fallbacks", async () => {
    const state = seedAuthed();
    state.messages["100"] = [
      {
        id: 51,
        user_id: 2,
        channel_id: 100,
        parent_id: null,
        created_at: 1_700_000_000_000_000,
        deleted_at: 1_700_000_002_000_000,
        text: "",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: true,
        mentions: [],
        attachments: [],
        embeds: [],
        reactions: [],
      },
      {
        id: 52,
        user_id: DEV_USER.id,
        channel_id: 100,
        parent_id: null,
        reply_to_message_id: 51,
        created_at: 1_700_000_003_000_000,
        text: "reply after tombstone reload",
        username: DEV_USER.username,
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
        reply_to: {
          id: 51,
          user_id: 2,
          channel_id: 100,
          created_at: 1_700_000_000_000_000,
          deleted_at: 1_700_000_002_000_000,
          text: "",
          attachment_count: 0,
          username: "bob",
          display_name: null,
          avatar_url: null,
        },
      },
      {
        id: 54,
        user_id: DEV_USER.id,
        channel_id: 100,
        parent_id: null,
        reply_to_message_id: 53,
        created_at: 1_700_000_004_000_000,
        text: "reply after hard delete reload",
        username: DEV_USER.username,
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
        reply_to: null,
      },
    ];
    mountAt("/channel/100");

    await waitFor(() => {
      expect(screen.getByText("reply after tombstone reload")).toBeInTheDocument();
      expect(screen.getByText("reply after hard delete reload")).toBeInTheDocument();
    });
    expect(screen.getByLabelText(/replying to deleted message by bob/i)).toHaveTextContent(
      "Original message deleted",
    );
    expect(screen.getByLabelText(/replying to unavailable message 53/i)).toHaveTextContent(
      "Original message unavailable",
    );
    expect(screen.queryByText("secret before delete")).toBeNull();
    expect(screen.queryByRole("img", { name: /photo attachment from bob/i })).toBeNull();
  });

  test("channel composer commits native emoji autocomplete before sending", async () => {
    seedAuthed();
    mountAt("/channel/100");

    const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
    const draftWithToken = "native autocomplete :sm";
    fireEvent.input(input, { target: { value: draftWithToken } });
    setInputSelection(input, draftWithToken.length);

    const listbox = await screen.findByRole("listbox", { name: /emoji suggestions/i });
    expect(within(listbox).getByRole("option", { name: /:smiley:/i })).toBeInTheDocument();

    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => expect(input.value).toBe("native autocomplete 😃"));
    expect(mswState().sentMessages).toEqual([]);

    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => {
      expect(mswState().sentMessages).toContainEqual({
        channel: "100",
        text: "native autocomplete 😃",
      });
    });
  });

  test("sends selected custom emoji markers unchanged through the message API", async () => {
    const state = seedAuthed();
    state.customEmojis = [
      {
        id: 123,
        name: "party",
        image_url: "/uploads/emojis/123.webp?v=1",
        animated: false,
        created_by_user_id: DEV_USER.id,
        created_at: 1,
        updated_at: 1,
        deleted_at: null,
      },
    ];
    mountAt("/channel/100");

    const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
    fireEvent.input(input, { target: { value: "hello " } });
    fireEvent.click(screen.getByRole("button", { name: /open emoji picker/i }));
    const dialog = await screen.findByRole("dialog", { name: /emoji picker/i });
    fireEvent.input(within(dialog).getByRole("combobox", { name: /search and select emoji/i }), {
      target: { value: "party" },
    });
    const partyCell = await within(dialog).findByRole("gridcell", { name: /emoji :party:/i });
    fireEvent.click(within(partyCell).getByRole("button", { name: /emoji :party:/i }));
    await waitFor(() => expect(input.value).toBe("hello <:party:123>"));
    fireEvent.submit(assertExists(input.closest("form"), "form"));

    await waitFor(() => {
      expect(mswState().sentMessages).toContainEqual({
        channel: "100",
        text: "hello <:party:123>",
      });
    });
  });

  test("sends custom emoji autocomplete markers and renders delivered markers as images", async () => {
    const state = seedAuthed();
    state.customEmojis = [
      {
        id: 123,
        name: "party",
        image_url: "/uploads/emojis/123.webp?v=1",
        animated: false,
        created_by_user_id: DEV_USER.id,
        created_at: 1,
        updated_at: 1,
        deleted_at: null,
      },
    ];
    mountAt("/channel/100");

    const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
    fireEvent.input(input, { target: { value: "hello :pa" } });
    input.setSelectionRange("hello :pa".length, "hello :pa".length);
    fireEvent.select(input);
    await screen.findByRole("option", { name: /emoji :party:/i });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => {
      expect(input.value).toBe("hello <:party:123>");
      expect(screen.getByRole("img", { name: /custom emoji :party:/i })).toBeInTheDocument();
    });
    expect(screen.queryByText("<:party:123>")).toBeNull();
    fireEvent.submit(assertExists(input.closest("form"), "form"));

    await waitFor(() => {
      expect(mswState().sentMessages).toContainEqual({
        channel: "100",
        text: "hello <:party:123>",
      });
    });
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());
    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessage({
      id: 99,
      user_id: DEV_USER.id,
      channel_id: 100,
      text: "hello <:party:123>",
      username: DEV_USER.username,
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [],
      attachments: [],
      embeds: [],
    });

    const image = await screen.findByRole("img", { name: /^:party:$/i });
    expect(image.getAttribute("src")).toContain("/uploads/emojis/123.webp?v=1");
  });

  test("submitting closes an open emoji picker", async () => {
    seedAuthed();
    mountAt("/channel/100");

    const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
    fireEvent.input(input, { target: { value: "typed message" } });
    fireEvent.click(screen.getByRole("button", { name: /open emoji picker/i }));
    await screen.findByRole("dialog", { name: /emoji picker/i });

    fireEvent.submit(assertExists(input.closest("form"), "form"));

    await waitFor(() => {
      expect(mswState().sentMessages).toContainEqual({ channel: "100", text: "typed message" });
      expect(input.value).toBe("");
      expect(screen.queryByRole("dialog", { name: /emoji picker/i })).toBeNull();
    });
  });

  test("converts a completed emoji shortcode in the composer and sends the glyph", async () => {
    seedAuthed();
    mountAt("/channel/100");

    const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
    fireEvent.input(input, { target: { value: ":grinning" } });
    expect(input.value).toBe(":grinning");

    fireEvent.input(input, { target: { value: ":grinning:" } });

    await waitFor(() => {
      expect(input.value).toBe("😀");
      expect(input.selectionStart).toBe("😀".length);
    });

    fireEvent.submit(assertExists(input.closest("form"), "form"));
    await waitFor(() => {
      expect(mswState().sentMessages).toContainEqual({ channel: "100", text: "😀" });
    });
  });

  test("right-clicking own message and submitting edit PUTs to /message/:id", async () => {
    seedOwnMessage({ username: "baipas" });
    mountAt("/channel/100");

    const input = await openMessageEdit("original");
    expect(input.value).toBe("original");
    fireEvent.input(input, { target: { value: "edited!" } });
    const form = assertExists(input.closest("form"), "form");
    fireEvent.submit(form);

    await waitFor(() => {
      expect(mswState().editedMessages).toContainEqual({ id: 7, text: "edited!" });
    });
  });

  test("applies mention metadata returned by channel message edit responses", async () => {
    const bob: PublicUser = {
      id: 2,
      username: "bobeditresponse",
      display_name: "Bobby Response",
      avatar_url: null,
    };
    const state = seedOwnMessage({ username: "baipas" });
    state.users.push(bob);
    mountAt("/channel/100");

    const input = await openMessageEdit("original");
    const text = `edited mention <@${bob.id}>`;
    fireEvent.input(input, { target: { value: text } });
    fireEvent.submit(assertExists(input.closest("form"), "form"));

    await waitFor(() => {
      expect(state.editedMessages).toContainEqual({ id: 7, text });
      expect(screen.getByText("@Bobby Response")).toHaveAttribute("title", "@bobeditresponse");
      expect(state.messages["100"][0]?.mentions).toEqual([bob]);
    });
  });

  test("channel message edits search, chip, serialize, and render user mentions", async () => {
    const bob: PublicUser = {
      id: 2,
      username: "bobchanneledit",
      display_name: "Bobby Channel Edit",
      avatar_url: null,
    };
    const state = seedOwnMessage({ id: 36, username: "baipas", text: "edit me" });
    state.users.push(bob);
    mountAt("/channel/100");

    const input = await openMessageEdit("edit me");
    inputFromUser(input, "edited @bobchannel");

    const listbox = await screen.findByRole("listbox", { name: /mention suggestions/i });
    const bobOption = within(listbox).getByRole("option", {
      name: /mention bobby channel edit @bobchanneledit/i,
    });
    expect(bobOption).toHaveAttribute("aria-selected", "true");
    expect(state.userSearchRequests).toContainEqual({ query: "bobchannel", limit: 8 });

    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => {
      expect(input.value).toBe("edited <@2> ");
      expect(within(input).getByText("@Bobby Channel Edit")).toBeInTheDocument();
    });
    expect(state.editedMessages).toEqual([]);

    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => {
      expect(state.editedMessages).toContainEqual({ id: 36, text: "edited <@2> " });
    });
    expect(screen.getByText("@Bobby Channel Edit")).toHaveAttribute("title", "@bobchanneledit");
  });

  test("Shift+Enter inserts an edit newline and Enter PUTs the exact multiline draft", async () => {
    seedOwnMessage({ username: "baipas", text: "first line" });
    mountAt("/channel/100");

    const input = await openMessageEdit("first line");
    setInputSelection(input, "first line".length);

    fireEvent.keyDown(input, { key: "Enter", shiftKey: true });

    await waitFor(() => {
      expect(input.value).toBe("first line\n");
      expect(input.selectionStart).toBe("first line\n".length);
    });

    const text = "first line\nsecond line";
    fireEvent.input(input, { target: { value: text } });
    setInputSelection(input, text.length);
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => {
      expect(mswState().editedMessages).toContainEqual({ id: 7, text });
    });
  });

  test("Save button PUTs exact multiline edit text", async () => {
    seedOwnMessage({ username: "baipas", text: "button edit" });
    mountAt("/channel/100");

    const input = await openMessageEdit("button edit");
    const text = "button first line\nbutton second line\nbutton third line";
    fireEvent.input(input, { target: { value: text } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() => {
      expect(mswState().editedMessages).toContainEqual({ id: 7, text });
    });
  });

  test("Escape cancels multiline edits without PUTing", async () => {
    seedOwnMessage({ username: "baipas", text: "cancel me" });
    mountAt("/channel/100");

    const input = await openMessageEdit("cancel me");
    fireEvent.input(input, { target: { value: "cancel me\nunsaved" } });
    fireEvent.keyDown(input, { key: "Escape" });

    await waitFor(() => expect(screen.queryByLabelText(/edit message/i)).toBeNull());
    expect(mswState().editedMessages).toEqual([]);
    expect(screen.getByText("cancel me")).toBeInTheDocument();
  });

  test("Escape dismisses message edit autocomplete before a second Escape cancels edit", async () => {
    seedOwnMessage({ id: 35, text: "escape edit" });
    mountAt("/channel/100");

    const input = await openMessageEdit("escape edit");
    const draftWithToken = "escape edit :sm";
    fireEvent.input(input, { target: { value: draftWithToken } });
    setInputSelection(input, draftWithToken.length);
    await screen.findByRole("listbox", { name: /emoji suggestions/i });

    fireEvent.keyDown(input, { key: "Escape" });

    await waitFor(() =>
      expect(screen.queryByRole("listbox", { name: /emoji suggestions/i })).toBeNull(),
    );
    expect(screen.getByLabelText(/edit message/i)).toBeInTheDocument();
    expect(mswState().editedMessages).toEqual([]);

    fireEvent.keyDown(input, { key: "Escape" });

    await waitFor(() => expect(screen.queryByLabelText(/edit message/i)).toBeNull());
    expect(mswState().editedMessages).toEqual([]);
    expect(screen.getByText("escape edit")).toBeInTheDocument();
  });

  test("Escape dismisses message edit mention autocomplete before canceling the edit", async () => {
    const state = seedOwnMessage({ id: 37, text: "escape mention edit" });
    state.users.push({
      id: 2,
      username: "bobescapeedit",
      display_name: "Bobby Escape Edit",
      avatar_url: null,
    });
    mountAt("/channel/100");

    const input = await openMessageEdit("escape mention edit");
    inputFromUser(input, "escape @bobescape");
    await screen.findByRole("listbox", { name: /mention suggestions/i });

    fireEvent.keyDown(input, { key: "Escape" });

    await waitFor(() =>
      expect(screen.queryByRole("listbox", { name: /mention suggestions/i })).toBeNull(),
    );
    expect(screen.getByLabelText(/edit message/i)).toBeInTheDocument();
    expect(state.editedMessages).toEqual([]);

    fireEvent.keyDown(input, { key: "Escape" });

    await waitFor(() => expect(screen.queryByLabelText(/edit message/i)).toBeNull());
    expect(state.editedMessages).toEqual([]);
    expect(screen.getByText("escape mention edit")).toBeInTheDocument();
  });

  test("unchanged multiline edits exit edit mode without PUTing", async () => {
    const text = "same first line\nsame second line";
    seedOwnMessage({ username: "baipas", text });
    mountAt("/channel/100");

    const original = await findRenderedMessageText(text);
    fireEvent.contextMenu(original);
    fireEvent.click(await screen.findByRole("menuitem", { name: /edit message/i }));
    const input = (await screen.findByLabelText(/edit message/i)) as HTMLInputElement;
    fireEvent.submit(assertExists(input.closest("form"), "form"));

    await waitFor(() => expect(screen.queryByLabelText(/edit message/i)).toBeNull());
    expect(mswState().editedMessages).toEqual([]);
  });

  test("converts a completed emoji shortcode while editing and PUTs the glyph", async () => {
    seedOwnMessage({ id: 31, text: "plain text" });
    mountAt("/channel/100");

    const input = await openMessageEdit("plain text");
    fireEvent.input(input, { target: { value: "looks :grinning:" } });

    await waitFor(() => {
      expect(input.value).toBe("looks 😀");
      expect(input.selectionStart).toBe("looks 😀".length);
    });

    fireEvent.submit(assertExists(input.closest("form"), "form"));
    await waitFor(() => {
      expect(mswState().editedMessages).toContainEqual({ id: 31, text: "looks 😀" });
    });
  });

  test("channel message edits commit emoji autocomplete before saving", async () => {
    seedOwnMessage({ id: 34, text: "plain text" });
    mountAt("/channel/100");

    const input = await openMessageEdit("plain text");
    const textWithToken = "edit :sm";
    fireEvent.input(input, { target: { value: textWithToken } });
    setInputSelection(input, textWithToken.length);

    const listbox = await screen.findByRole("listbox", { name: /emoji suggestions/i });
    expect(within(listbox).getByRole("option", { name: /:smiley:/i })).toBeInTheDocument();

    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => expect(input.value).toBe("edit 😃"));
    expect(mswState().editedMessages).toEqual([]);

    fireEvent.submit(assertExists(input.closest("form"), "form"));

    await waitFor(() => {
      expect(mswState().editedMessages).toContainEqual({ id: 34, text: "edit 😃" });
    });
  });

  test("selecting an emoji inserts it at the edit caret and PUTs the glyph", async () => {
    seedOwnMessage({ id: 32, text: "hello world" });
    mountAt("/channel/100");

    const input = await openMessageEdit("hello world");
    const form = assertExists(input.closest("form"), "form");
    setInputSelection(input, "hello ".length);
    fireEvent.click(within(form).getByRole("button", { name: /open emoji picker/i }));

    const dialog = await screen.findByRole("dialog", { name: /emoji picker/i });
    fireEvent.input(within(dialog).getByRole("combobox", { name: /search and select emoji/i }), {
      target: { value: ":smile:" },
    });
    const smileCell = within(dialog).getByRole("gridcell", { name: /emoji :smile:/i });
    fireEvent.click(within(smileCell).getByRole("button", { name: /emoji :smile:/i }));

    await waitFor(() => {
      expect(input.value).toBe("hello 😄world");
      expect(document.activeElement).toBe(input);
    });

    fireEvent.submit(form);
    await waitFor(() => {
      expect(mswState().editedMessages).toContainEqual({ id: 32, text: "hello 😄world" });
    });
  });

  test("selecting an emoji replaces selected edit text", async () => {
    seedOwnMessage({ id: 33, text: "hello world" });
    mountAt("/channel/100");

    const input = await openMessageEdit("hello world");
    const form = assertExists(input.closest("form"), "form");
    setInputSelection(input, "hello ".length, "hello world".length);
    fireEvent.click(within(form).getByRole("button", { name: /open emoji picker/i }));

    const dialog = await screen.findByRole("dialog", { name: /emoji picker/i });
    fireEvent.input(within(dialog).getByRole("combobox", { name: /search and select emoji/i }), {
      target: { value: "heart" },
    });
    const heartCell = within(dialog).getByRole("gridcell", { name: /emoji :heart:/i });
    fireEvent.click(within(heartCell).getByRole("button", { name: /emoji :heart:/i }));

    await waitFor(() => {
      expect(input.value).toBe("hello ❤️");
      expect(document.activeElement).toBe(input);
    });
  });

  test("does not open edit menu for other users' messages", async () => {
    const state = resetMswState();
    state.me = { ...DEV_USER, username: "alice", display_name: null, avatar_url: null };
    state.messages["100"] = [
      {
        id: 8,
        user_id: 999,
        channel_id: 100,
        text: "someone else",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
    ];
    mountAt("/channel/100");

    const other = await screen.findByText("someone else");
    fireEvent.contextMenu(other);

    expect(screen.queryByRole("menuitem", { name: /edit message/i })).toBeNull();
  });

  test("right-clicking own message and confirming delete sends DELETE /message/:id", async () => {
    const state = resetMswState();
    state.me = { ...DEV_USER, username: "alice", display_name: null, avatar_url: null };
    state.messages["100"] = [
      {
        id: 7,
        user_id: DEV_USER.id,
        channel_id: 100,
        text: "delete me",
        username: "baipas",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
    ];
    mountAt("/channel/100");

    const original = await screen.findByText("delete me");
    fireEvent.contextMenu(original);

    const deleteItem = await screen.findByRole("menuitem", { name: /delete message/i });
    fireEvent.click(deleteItem);

    const dialog = await screen.findByRole("dialog", { name: /delete message/i });
    fireEvent.click(within(dialog).getByRole("button", { name: /^delete$/i }));

    await waitFor(() => {
      expect(mswState().deletedMessageIds).toContain(7);
    });
  });

  test("canceling the delete confirmation does not call the server", async () => {
    const state = resetMswState();
    state.me = { ...DEV_USER, username: "alice", display_name: null, avatar_url: null };
    state.messages["100"] = [
      {
        id: 9,
        user_id: DEV_USER.id,
        channel_id: 100,
        text: "keep me",
        username: "baipas",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
    ];
    mountAt("/channel/100");

    const original = await screen.findByText("keep me");
    fireEvent.contextMenu(original);

    const deleteItem = await screen.findByRole("menuitem", { name: /delete message/i });
    fireEvent.click(deleteItem);

    // The confirmation modal should be open.
    await screen.findByRole("dialog", { name: /delete message/i });

    const cancelBtn = screen.getAllByRole("button", { name: /cancel/i })[0];
    fireEvent.click(cancelBtn);

    await new Promise((r) => setTimeout(r, 10));
    expect(mswState().deletedMessageIds).not.toContain(9);
  });

  test("submitting an empty edit prompts delete confirmation and deletes on confirm", async () => {
    const state = resetMswState();
    state.me = { ...DEV_USER, username: "alice", display_name: null, avatar_url: null };
    state.messages["100"] = [
      {
        id: 11,
        user_id: DEV_USER.id,
        channel_id: 100,
        text: "blank me",
        username: "baipas",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
    ];
    mountAt("/channel/100");

    const original = await screen.findByText("blank me");
    fireEvent.contextMenu(original);

    const editItem = await screen.findByRole("menuitem", { name: /edit message/i });
    fireEvent.click(editItem);

    const input = (await screen.findByLabelText(/edit message/i)) as HTMLInputElement;
    fireEvent.input(input, { target: { value: "" } });
    const form = assertExists(input.closest("form"), "form");
    fireEvent.submit(form);

    const dialog = await screen.findByRole("dialog", { name: /delete message/i });
    // No edit should have been sent for the blank text.
    expect(mswState().editedMessages).not.toContainEqual({ id: 11, text: "" });

    fireEvent.click(within(dialog).getByRole("button", { name: /^delete$/i }));

    await waitFor(() => {
      expect(mswState().deletedMessageIds).toContain(11);
    });
  });

  test("submitting an empty edit on a photo message saves an empty caption", async () => {
    const state = resetMswState();
    state.me = { ...DEV_USER, username: "alice", display_name: null, avatar_url: null };
    state.messages["100"] = [
      {
        id: 12,
        user_id: DEV_USER.id,
        channel_id: 100,
        text: "photo caption",
        username: "baipas",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [makeAttachment({ id: 9501, message_id: 12 })],
        embeds: [],
      },
    ];
    mountAt("/channel/100");

    const input = await openMessageEdit("photo caption");
    fireEvent.input(input, { target: { value: "" } });
    fireEvent.submit(assertExists(input.closest("form"), "form"));

    await waitFor(() => {
      expect(state.editedMessages).toContainEqual({ id: 12, text: "" });
      expect(
        screen.getByRole("img", { name: /photo attachment from baipas/i }),
      ).toBeInTheDocument();
    });
    expect(screen.queryByRole("dialog", { name: /delete message/i })).toBeNull();
    expect(state.deletedMessageIds).not.toContain(12);
  });

  test("removes message from UI when a message_deleted SSE event arrives", async () => {
    const state = seedAuthed();
    state.messages["100"][0] = {
      ...state.messages["100"][0],
      attachments: [makeAttachment({ id: 9701, message_id: 1 })],
    };
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
    expect(screen.getByRole("img", { name: /photo attachment from alice/i })).toBeInTheDocument();
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessageDeleted({ id: 1, channel_id: 100 });

    await waitFor(() => {
      expect(screen.queryByText("hello")).toBeNull();
      expect(screen.queryByRole("img", { name: /photo attachment from alice/i })).toBeNull();
      expect(screen.getByText("world")).toBeInTheDocument();
    });
  });

  test("updates displayed text when a message_updated SSE event arrives", async () => {
    seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessageUpdated({
      id: 1,
      user_id: 1,
      channel_id: 100,
      text: "hello (edited)",
      username: "alice",
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [],
      attachments: [],
      embeds: [],
    });

    await waitFor(() => {
      expect(screen.getByText("hello (edited)")).toBeInTheDocument();
      expect(screen.queryByText("hello")).toBeNull();
    });
  });

  test("applies mention metadata from message_updated SSE events", async () => {
    const bob: PublicUser = {
      id: 2,
      username: "bobliveedit",
      display_name: "Bobby Live",
      avatar_url: null,
    };
    const carol: PublicUser = {
      id: 3,
      username: "carolliveedit",
      display_name: "Carol Live",
      avatar_url: null,
    };
    const state = seedAuthed();
    state.messages["100"] = [
      {
        id: 15,
        user_id: DEV_USER.id,
        channel_id: 100,
        text: `before <@${bob.id}>`,
        username: DEV_USER.username,
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [bob],
        attachments: [],
        embeds: [],
      },
    ];
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("@Bobby Live")).toBeInTheDocument());
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessageUpdated({
      id: 15,
      user_id: DEV_USER.id,
      channel_id: 100,
      text: `after <@${carol.id}>`,
      username: DEV_USER.username,
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [carol],
      attachments: [],
      embeds: [],
    });

    await waitFor(() => {
      expect(screen.getByText("@Carol Live")).toHaveAttribute("title", "@carolliveedit");
      expect(screen.queryByText("@Bobby Live")).toBeNull();
    });
  });

  test("message_updated tombstones hide root photos in channel and open thread views", async () => {
    const state = seedAuthed();
    state.messages["100"][0] = {
      ...state.messages["100"][0],
      attachments: [makeAttachment({ id: 9601, message_id: 1 })],
      embeds: [
        {
          id: 9602,
          message_id: 1,
          url: "https://example.com",
          title: "Example",
          description: null,
          image_url: null,
          site_name: null,
          embed_type: "link",
          iframe_url: null,
          iframe_width: null,
          iframe_height: null,
        },
      ],
      reactions: [{ kind: "native", emoji: "👍", count: 1, me_reacted: true }],
      thread_summary: { reply_count: 1, last_reply_created_at: 1_700_000_010_000_000 },
    };
    state.threadReplies["1"] = [
      {
        id: 96,
        user_id: 2,
        channel_id: 100,
        parent_id: 1,
        created_at: 1_700_000_010_000_000,
        text: "reply survives tombstone",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
    ];
    mountAt("/channel/100?thread=1");

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() =>
      expect(within(panel).getByText("reply survives tombstone")).toBeInTheDocument(),
    );
    expect(screen.getAllByRole("img", { name: /photo attachment from alice/i })).toHaveLength(2);
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessageUpdated({
      id: 1,
      user_id: 1,
      channel_id: 100,
      parent_id: null,
      created_at: 1_700_000_000_000_000,
      deleted_at: 1_700_000_020_000_000,
      text: "",
      username: "alice",
      display_name: null,
      avatar_url: null,
      suppress_embeds: true,
      mentions: [],
      attachments: [],
      embeds: [],
      reactions: [],
      thread_summary: { reply_count: 1, last_reply_created_at: 1_700_000_010_000_000 },
    });

    await waitFor(() => {
      expect(screen.queryByRole("img", { name: /photo attachment from alice/i })).toBeNull();
      expect(screen.queryByRole("link", { name: /example/i })).toBeNull();
      expect(screen.queryByRole("button", { name: /👍 1 reaction/i })).toBeNull();
      expect(screen.getAllByLabelText(/original message deleted/i)).toHaveLength(2);
      expect(within(panel).getByText("reply survives tombstone")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /open thread with 1 reply/i })).toBeInTheDocument();
    });
  });

  test("renders multiline text from message_updated SSE events with visible line breaks", async () => {
    seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const text = "updated first line\nupdated second line\nupdated third line";
    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessageUpdated({
      id: 1,
      user_id: 1,
      channel_id: 100,
      text,
      username: "alice",
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [],
      attachments: [],
      embeds: [],
    });

    const messageText = await findRenderedMessageText(text);
    expect(messageText.textContent).toBe(text);
    expect(messageText).toHaveClass(
      "whitespace-pre-wrap",
      "break-words",
      "[overflow-wrap:anywhere]",
    );
    expect(screen.queryByText("hello")).toBeNull();
  });

  test("renders emoji glyphs and links from message_updated SSE events", async () => {
    seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessageUpdated({
      id: 1,
      user_id: 1,
      channel_id: 100,
      text: "edited 😀 link https://example.com",
      username: "alice",
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      mentions: [],
      attachments: [],
      embeds: [],
    });

    await waitFor(() => {
      const link = screen.getByRole("link", { name: "https://example.com" });
      expect(assertExists(link.parentElement, "message text")).toHaveTextContent(
        "edited 😀 link https://example.com",
      );
      expect(screen.queryByText("hello")).toBeNull();
    });
  });

  test("shows a typing indicator when another user's user_typing event arrives", async () => {
    seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushUserTyping({ channel_id: 100, user_id: 2, username: "carol" });

    await waitFor(() => {
      expect(screen.getByText(/carol is typing/i)).toBeInTheDocument();
    });
  });

  test("does not show a typing indicator for the current user's own pings", async () => {
    const state = resetMswState();
    state.me = { ...DEV_USER, username: "alice", display_name: null, avatar_url: null };
    // Own messages have a right-click "edit" menu — we use that as our signal
    // that the auth resource has resolved so currentUserId is known.
    state.messages["100"] = [
      {
        id: 42,
        user_id: DEV_USER.id,
        channel_id: 100,
        text: "mine",
        username: DEV_USER.username,
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
    ];
    mountAt("/channel/100");

    await screen.findByText("mine");
    await waitFor(() => {
      expect(screen.getByRole("toolbar", { name: /message actions/i })).toBeInTheDocument();
    });

    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());
    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushUserTyping({ channel_id: 100, user_id: DEV_USER.id, username: DEV_USER.username });

    await new Promise((r) => setTimeout(r, 20));
    expect(screen.queryByTestId("typing-indicator")).toBeNull();
  });

  test("POSTs a typing ping to the current channel when the input changes", async () => {
    seedAuthed();
    mountAt("/channel/100");

    const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
    fireEvent.input(input, { target: { value: "h" } });

    await waitFor(() => {
      expect(mswState().typingPings).toContain("100");
    });
  });

  test("selecting an emoji inserts it at the composer caret, closes the picker, and sends it", async () => {
    seedAuthed();
    mountAt("/channel/100");

    const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
    fireEvent.input(input, { target: { value: "hello world" } });
    setInputSelection(input, "hello ".length);
    fireEvent.click(screen.getByRole("button", { name: /open emoji picker/i }));

    const dialog = await screen.findByRole("dialog", { name: /emoji picker/i });
    fireEvent.input(within(dialog).getByRole("combobox", { name: /search and select emoji/i }), {
      target: { value: ":smile:" },
    });
    const smileCell = within(dialog).getByRole("gridcell", { name: /emoji :smile:/i });
    fireEvent.click(within(smileCell).getByRole("button", { name: /emoji :smile:/i }));

    await waitFor(() => expect(input.value).toBe("hello 😄world"));
    expect(screen.queryByRole("dialog", { name: /emoji picker/i })).toBeNull();
    await waitFor(() => expect(document.activeElement).toBe(input));

    fireEvent.submit(assertExists(input.closest("form"), "form"));
    await waitFor(() => {
      expect(mswState().sentMessages).toContainEqual({ channel: "100", text: "hello 😄world" });
    });
  });

  test("selecting an emoji replaces selected composer text", async () => {
    seedAuthed();
    mountAt("/channel/100");

    const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
    fireEvent.input(input, { target: { value: "hello world" } });
    setInputSelection(input, "hello ".length, "hello world".length);
    fireEvent.click(screen.getByRole("button", { name: /open emoji picker/i }));

    const dialog = await screen.findByRole("dialog", { name: /emoji picker/i });
    fireEvent.input(within(dialog).getByRole("combobox", { name: /search and select emoji/i }), {
      target: { value: "heart" },
    });
    const heartCell = within(dialog).getByRole("gridcell", { name: /emoji :heart:/i });
    fireEvent.click(within(heartCell).getByRole("button", { name: /emoji :heart:/i }));

    await waitFor(() => {
      expect(input.value).toBe("hello ❤️");
      expect(document.activeElement).toBe(input);
    });
  });

  test("selecting an emoji into an empty draft posts a typing ping", async () => {
    seedAuthed();
    mountAt("/channel/100");

    await screen.findByPlaceholderText(/send a new message/i);
    fireEvent.click(screen.getByRole("button", { name: /open emoji picker/i }));

    const dialog = await screen.findByRole("dialog", { name: /emoji picker/i });
    fireEvent.input(within(dialog).getByRole("combobox", { name: /search and select emoji/i }), {
      target: { value: "heart" },
    });
    const heartCell = within(dialog).getByRole("gridcell", { name: /emoji :heart:/i });
    fireEvent.click(within(heartCell).getByRole("button", { name: /emoji :heart:/i }));

    await waitFor(() => {
      expect(mswState().typingPings).toContain("100");
    });
  });

  test("clicking the hover toolbar Edit button opens the edit form and PUTs /message/:id", async () => {
    const state = resetMswState();
    state.me = { ...DEV_USER, username: "alice", display_name: null, avatar_url: null };
    state.messages["100"] = [
      {
        id: 21,
        user_id: DEV_USER.id,
        channel_id: 100,
        text: "before edit",
        username: "baipas",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
    ];
    mountAt("/channel/100");

    await screen.findByText("before edit");

    // Toolbar buttons are always in the DOM (hidden via CSS until hover);
    // the test can click them directly without simulating mouseover.
    fireEvent.click(await screen.findByRole("button", { name: /^edit$/i }));

    const input = (await screen.findByLabelText(/edit message/i)) as HTMLInputElement;
    fireEvent.input(input, { target: { value: "after edit" } });
    fireEvent.submit(assertExists(input.closest("form"), "form"));

    await waitFor(() => {
      expect(mswState().editedMessages).toContainEqual({ id: 21, text: "after edit" });
    });
  });

  test("clicking the hover toolbar Delete button confirms and DELETEs /message/:id", async () => {
    const state = resetMswState();
    state.me = { ...DEV_USER, username: "alice", display_name: null, avatar_url: null };
    state.messages["100"] = [
      {
        id: 22,
        user_id: DEV_USER.id,
        channel_id: 100,
        text: "zap me",
        username: "baipas",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
    ];
    mountAt("/channel/100");

    await screen.findByText("zap me");
    fireEvent.click(await screen.findByRole("button", { name: /^delete$/i }));

    const dialog = await screen.findByRole("dialog", { name: /delete message/i });
    fireEvent.click(within(dialog).getByRole("button", { name: /^delete$/i }));

    await waitFor(() => {
      expect(mswState().deletedMessageIds).toContain(22);
    });
  });

  test("renders only the thread action on another user's message", async () => {
    const state = resetMswState();
    state.me = { ...DEV_USER, username: "alice", display_name: null, avatar_url: null };
    state.messages["100"] = [
      {
        id: 23,
        user_id: 999,
        channel_id: 100,
        text: "not mine",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
    ];
    mountAt("/channel/100");

    await screen.findByText("not mine");
    // Wait a tick for the auth resource to resolve so currentUserId is known.
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /reply in thread/i })).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /^edit$/i })).toBeNull();
      expect(screen.queryByRole("button", { name: /^delete$/i })).toBeNull();
    });
  });

  test("renders avatars next to each message", async () => {
    const state = resetMswState();
    state.me = { ...DEV_USER, username: "alice", display_name: null, avatar_url: null };
    state.messages["100"] = [
      {
        id: 1,
        user_id: 1,
        channel_id: 100,
        text: "hello",
        username: "alice",
        display_name: null,
        avatar_url: "/uploads/avatars/1.webp?v=1",
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
      {
        id: 2,
        user_id: 2,
        channel_id: 100,
        text: "world",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
    ];
    mountAt("/channel/100");

    await waitFor(() => {
      const aliceAvatar = screen.getByRole("img", { name: /alice's avatar/i });
      expect(aliceAvatar.querySelector("img")).not.toBeNull();
      const bobAvatar = screen.getByRole("img", { name: /bob's avatar/i });
      expect(bobAvatar.querySelector("svg")).not.toBeNull();
    });
  });

  test("applies a message_embeds_updated SSE event to the existing message", async () => {
    const state = seedAuthed();
    state.messages["100"] = [
      {
        id: 77,
        user_id: 1,
        channel_id: 100,
        text: "see 😀 https://example.com",
        username: "alice",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
      },
    ];
    mountAt("/channel/100");

    await waitFor(() => {
      const link = screen.getByRole("link", { name: "https://example.com" });
      expect(assertExists(link.parentElement, "message text")).toHaveTextContent(
        "see 😀 https://example.com",
      );
    });
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessageEmbedsUpdated({
      id: 77,
      channel_id: 100,
      suppress_embeds: false,
      embeds: [
        {
          id: 5000,
          message_id: 77,
          url: "https://example.com",
          title: "Example domain",
          description: null,
          image_url: null,
          site_name: "Example",
          embed_type: "link",
          iframe_url: null,
          iframe_width: null,
          iframe_height: null,
        },
      ],
    });

    await waitFor(() => {
      expect(screen.getByRole("link", { name: /example domain/i })).toBeInTheDocument();
    });
  });

  test("applies a message_reactions_updated SSE event to the existing message", async () => {
    const state = seedAuthed();
    state.messages["100"] = [
      {
        id: 78,
        user_id: 2,
        channel_id: 100,
        text: "react live",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
        reactions: [],
      },
    ];
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("react live")).toBeInTheDocument());
    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessageReactionsUpdated({
      id: 78,
      channel_id: 100,
      user_id: DEV_USER.id,
      reactions: [{ kind: "native", emoji: "👍", count: 3, me_reacted: true }],
    });

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /👍 3 reactions\. remove your reaction/i }),
      ).toHaveAttribute("aria-pressed", "true");
    });
  });

  test("reaction SSE patches only the matching visible message without refetching or reordering", async () => {
    const state = seedAuthed();
    let messageFetches = 0;
    server.use(
      http.get(`${TEST_SERVER}/messages/:id`, ({ params }) => {
        messageFetches += 1;
        return HttpResponse.json(state.messages[String(params.id)] ?? []);
      }),
    );

    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
    await waitFor(() => expect(messageFetches).toBe(1));
    const hello = screen.getByText("hello");
    const world = screen.getByText("world");
    expect(hello.compareDocumentPosition(world) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessageReactionsUpdated({
      id: 2,
      channel_id: 100,
      user_id: 2,
      reactions: [{ kind: "native", emoji: "👍", count: 1, me_reacted: true }],
    });

    const helloRow = assertExists(hello.closest(".group"), "hello row") as HTMLElement;
    const worldRow = assertExists(world.closest(".group"), "world row") as HTMLElement;
    await waitFor(() => {
      expect(
        within(worldRow).getByRole("button", { name: /👍 1 reaction\. add your reaction/i }),
      ).toHaveAttribute("aria-pressed", "false");
    });
    expect(within(helloRow).queryByRole("button", { name: /👍/i })).toBeNull();
    expect(hello.compareDocumentPosition(world) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(messageFetches).toBe(1);
  });

  test("reaction SSE from another user preserves the viewer's own pressed state", async () => {
    const state = seedAuthed();
    state.messages["100"] = [
      {
        id: 78,
        user_id: 2,
        channel_id: 100,
        text: "already reacted",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [],
        reactions: [{ kind: "native", emoji: "👍", count: 2, me_reacted: true }],
      },
    ];
    mountAt("/channel/100");

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /👍 2 reactions\. remove your reaction/i }),
      ).toHaveAttribute("aria-pressed", "true");
    });
    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessageReactionsUpdated({
      id: 78,
      channel_id: 100,
      user_id: 2,
      reactions: [{ kind: "native", emoji: "👍", count: 1, me_reacted: false }],
    });

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /👍 1 reaction\. remove your reaction/i }),
      ).toHaveAttribute("aria-pressed", "true");
    });
  });

  test("message_embeds_updated with suppress_embeds=true hides existing embeds", async () => {
    const state = seedAuthed();
    state.messages["100"] = [
      {
        id: 88,
        user_id: 1,
        channel_id: 100,
        text: "see https://example.com",
        username: "alice",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        mentions: [],
        attachments: [],
        embeds: [
          {
            id: 6000,
            message_id: 88,
            url: "https://example.com",
            title: "Will go away",
            description: null,
            image_url: null,
            site_name: null,
            embed_type: "link",
            iframe_url: null,
            iframe_width: null,
            iframe_height: null,
          },
        ],
      },
    ];
    mountAt("/channel/100");

    await waitFor(() => {
      expect(screen.getByRole("link", { name: /will go away/i })).toBeInTheDocument();
    });

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessageEmbedsUpdated({
      id: 88,
      channel_id: 100,
      suppress_embeds: true,
      embeds: [],
    });

    await waitFor(() => {
      expect(screen.queryByRole("link", { name: /will go away/i })).toBeNull();
    });
  });
});
