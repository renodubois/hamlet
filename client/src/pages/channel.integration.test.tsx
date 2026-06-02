import { describe, expect, test, vi } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@solidjs/testing-library";
import { MemoryRouter, Route, createMemoryHistory } from "@solidjs/router";
import { http, HttpResponse } from "msw";
import { AuthProvider } from "../contexts/auth";
import { ChannelsProvider } from "../contexts/channels";
import { CustomEmojisProvider } from "../contexts/custom-emojis";
import { EventsProvider } from "../contexts/events";
import { FakeEventSource, latestFakeEventSource } from "../test/msw/sse";
import { mswState, resetMswState, server } from "../test/msw/server";
import { DEV_USER } from "../test/msw/handlers";
import { expectNoA11yViolations } from "../test/a11y";
import { assertExists } from "../test/render";
import ChannelView from "./channel";
import type { Message } from "../api";

vi.mock("../api", async () => {
  const actual = await vi.importActual<typeof import("../api")>("../api");
  return {
    ...actual,
    messagesEventSource: () => new FakeEventSource("/mock/messages") as unknown as EventSource,
  };
});

function mountAt(path: string) {
  const history = createMemoryHistory();
  history.set({ value: path });

  const result = render(() => (
    <AuthProvider>
      <EventsProvider>
        <CustomEmojisProvider>
          <ChannelsProvider>
            <MemoryRouter history={history}>
              <Route path="/channel/:id" component={ChannelView} />
            </MemoryRouter>
          </ChannelsProvider>
        </CustomEmojisProvider>
      </EventsProvider>
    </AuthProvider>
  ));

  return { ...result, history };
}

function seedAuthed() {
  const state = resetMswState();
  state.me = DEV_USER;
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
      embeds: [],
    },
  ];
  return state;
}

function nextMessageId(prev: Message[]): number {
  return prev.reduce((max, m) => Math.max(max, m.id), 0) + 1;
}

function setInputSelection(input: HTMLInputElement, start: number, end = start) {
  input.focus();
  input.setSelectionRange(start, end);
  fireEvent.select(input);
}

function findRenderedMessageText(text: string) {
  return screen.findByText(
    (_, element) =>
      element?.textContent === text && element.classList.contains("whitespace-pre-wrap"),
  );
}

function findRenderedMessageTextWithin(container: HTMLElement, text: string) {
  return within(container).findByText(
    (_, element) =>
      element?.textContent === text && element.classList.contains("whitespace-pre-wrap"),
  );
}

function queryRenderedMessageTextWithin(container: HTMLElement, text: string) {
  return within(container).queryByText(
    (_, element) =>
      element?.textContent === text && element.classList.contains("whitespace-pre-wrap"),
  );
}

function seedOwnMessage(overrides: Partial<Message> = {}) {
  const state = resetMswState();
  state.me = DEV_USER;
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

  test("failed thread replies restore the exact multiline draft after clearing", async () => {
    const state = seedAuthed();
    let releaseReply: () => void = () => undefined;
    const replyPaused = new Promise<void>((resolve) => {
      releaseReply = resolve;
    });
    server.use(
      http.post("http://127.0.0.1:3030/thread/1/reply", async ({ request }) => {
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

  test("appends a message delivered over SSE", async () => {
    seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessage({
      id: nextMessageId([]),
      user_id: 3,
      channel_id: 100,
      text: "hot off the wire",
      username: "carol",
      display_name: null,
      avatar_url: null,
      suppress_embeds: false,
      embeds: [],
    });

    await waitFor(() => {
      expect(screen.getByText("hot off the wire")).toBeInTheDocument();
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
      thread_summary: { reply_count: 1, last_reply_created_at: 1_700_000_060_000_000 },
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
        embeds: [],
      },
    ];

    mountAt("/channel/100");
    await waitFor(() =>
      expect(screen.getByLabelText(/original message deleted/i)).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /open thread with 1 reply/i }));

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => {
      expect(within(panel).getByLabelText(/original message deleted/i)).toBeInTheDocument();
      expect(within(panel).getByText("reply under tombstone")).toBeInTheDocument();
    });
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
        embeds: [],
      },
    ];

    mountAt("/channel/100?thread=1");
    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("newer live reply")).toBeInTheDocument());
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

  test("empty channel drafts still submit to the existing API validation path", async () => {
    seedAuthed();
    mountAt("/channel/100");

    const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
    fireEvent.submit(assertExists(input.closest("form"), "form"));

    await waitFor(() => {
      expect(mswState().sentMessages).toContainEqual({ channel: "100", text: "" });
    });
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
    state.me = DEV_USER;
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
    state.me = DEV_USER;
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
    state.me = DEV_USER;
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
    state.me = DEV_USER;
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

  test("removes message from UI when a message_deleted SSE event arrives", async () => {
    seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessageDeleted({ id: 1, channel_id: 100 });

    await waitFor(() => {
      expect(screen.queryByText("hello")).toBeNull();
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
      embeds: [],
    });

    await waitFor(() => {
      expect(screen.getByText("hello (edited)")).toBeInTheDocument();
      expect(screen.queryByText("hello")).toBeNull();
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
    state.me = DEV_USER;
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
    state.me = DEV_USER;
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
    state.me = DEV_USER;
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
    state.me = DEV_USER;
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
    state.me = DEV_USER;
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
