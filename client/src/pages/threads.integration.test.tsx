import { describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { MemoryRouter, Route, createMemoryHistory } from "@solidjs/router";
import { AuthProvider } from "../contexts/auth";
import { ChannelsProvider } from "../contexts/channels";
import { EventsProvider } from "../contexts/events";
import { FakeEventSource } from "../test/msw/sse";
import { resetMswState } from "../test/msw/server";
import { DEV_USER } from "../test/msw/handlers";
import ChannelView from "./channel";
import ThreadsView from "./threads";

vi.mock("../api", async () => {
  const actual = await vi.importActual<typeof import("../api")>("../api");
  return {
    ...actual,
    messagesEventSource: () => new FakeEventSource("/mock/messages") as unknown as EventSource,
  };
});

function mountAt(path = "/threads") {
  const history = createMemoryHistory();
  history.set({ value: path });

  const result = render(() => (
    <AuthProvider>
      <EventsProvider>
        <ChannelsProvider>
          <MemoryRouter history={history}>
            <Route path="/threads" component={ThreadsView} />
            <Route path="/channel/:id" component={ChannelView} />
          </MemoryRouter>
        </ChannelsProvider>
      </EventsProvider>
    </AuthProvider>
  ));

  return { ...result, history };
}

function seedParticipatedThreads() {
  const state = resetMswState();
  state.me = DEV_USER;
  state.channels = [
    { id: 100, name: "general", position: 0, type: "text" },
    { id: 200, name: "random", position: 1, type: "text" },
  ];
  state.messages = {
    "100": [
      {
        id: 10,
        user_id: DEV_USER.id,
        channel_id: 100,
        parent_id: null,
        created_at: 1_700_000_000_000_000,
        deleted_at: 1_700_000_005_000_000,
        text: "",
        username: DEV_USER.username,
        display_name: null,
        avatar_url: null,
        suppress_embeds: true,
        embeds: [],
      },
      {
        id: 30,
        user_id: 2,
        channel_id: 100,
        parent_id: null,
        created_at: 1_700_000_030_000_000,
        text: "bob-only root",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        embeds: [],
      },
    ],
    "200": [
      {
        id: 20,
        user_id: 2,
        channel_id: 200,
        parent_id: null,
        created_at: 1_700_000_020_000_000,
        text: "root alice joined",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        embeds: [],
        reactions: [{ kind: "native", emoji: "👍", count: 4, me_reacted: true }],
        thread_summary: {
          reply_count: 5,
          last_reply_created_at: 1_700_000_025_000_000,
        },
      },
    ],
  };
  state.threadReplies = {
    "10": [
      {
        id: 11,
        user_id: 2,
        channel_id: 100,
        parent_id: 10,
        created_at: 1_700_000_010_000_000,
        text: "reply to deleted root",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        embeds: [],
      },
    ],
    "20": [
      {
        id: 21,
        user_id: 2,
        channel_id: 200,
        parent_id: 20,
        created_at: 1_700_000_021_000_000,
        text: "old preview reply",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        embeds: [],
      },
      {
        id: 22,
        user_id: DEV_USER.id,
        channel_id: 200,
        parent_id: 20,
        created_at: 1_700_000_022_000_000,
        text: "alice participated",
        username: DEV_USER.username,
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        embeds: [],
      },
      {
        id: 23,
        user_id: 3,
        channel_id: 200,
        parent_id: 20,
        created_at: 1_700_000_023_000_000,
        text: "third newest",
        username: "charlie",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        embeds: [],
      },
      {
        id: 24,
        user_id: 2,
        channel_id: 200,
        parent_id: 20,
        created_at: 1_700_000_024_000_000,
        text: "second newest",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        embeds: [],
      },
      {
        id: 25,
        user_id: DEV_USER.id,
        channel_id: 200,
        parent_id: 20,
        created_at: 1_700_000_025_000_000,
        text: "newest preview reply",
        username: DEV_USER.username,
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        embeds: [],
        reactions: [{ kind: "native", emoji: "🔥", count: 2, me_reacted: false }],
      },
    ],
    "30": [
      {
        id: 31,
        user_id: 2,
        channel_id: 100,
        parent_id: 30,
        created_at: 1_700_000_040_000_000,
        text: "bob-only newest reply",
        username: "bob",
        display_name: null,
        avatar_url: null,
        suppress_embeds: false,
        embeds: [],
      },
    ],
  };
  return state;
}

describe("Threads view integration", () => {
  test("lists participated threads with previews and opens the selected full thread", async () => {
    const state = seedParticipatedThreads();
    const { history } = mountAt();

    await screen.findByRole("heading", { name: "Threads" });
    await waitFor(() => expect(screen.getByText("root alice joined")).toBeInTheDocument());

    const articles = screen.getAllByRole("article");
    expect(within(articles[0]).getByText("# random")).toBeInTheDocument();
    expect(within(articles[0]).getByText(/5 replies/i)).toBeInTheDocument();
    expect(within(articles[0]).getByText("third newest")).toBeInTheDocument();
    expect(within(articles[0]).getByText("second newest")).toBeInTheDocument();
    expect(within(articles[0]).getByText("newest preview reply")).toBeInTheDocument();
    expect(within(articles[0]).queryByText("old preview reply")).toBeNull();
    expect(within(articles[0]).queryByRole("button", { name: /reaction/i })).toBeNull();
    expect(within(articles[0]).queryByText("👍")).toBeNull();
    expect(within(articles[0]).queryByText("🔥")).toBeNull();

    expect(within(articles[1]).getByText("# general")).toBeInTheDocument();
    expect(within(articles[1]).getByText(/original message deleted/i)).toBeInTheDocument();
    expect(within(articles[1]).getByText("reply to deleted root")).toBeInTheDocument();
    expect(screen.queryByText("bob-only root")).toBeNull();
    expect(screen.queryByText("bob-only newest reply")).toBeNull();

    fireEvent.click(
      within(articles[0]).getByRole("link", { name: /open full thread in # random/i }),
    );

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("root alice joined")).toBeInTheDocument());
    expect(within(panel).getByText("old preview reply")).toBeInTheDocument();
    expect(history.get()).toBe("/channel/200?thread=20");
    expect(state.threadFetches).toContain(20);
  });
});
