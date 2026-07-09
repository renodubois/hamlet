import { describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "../test/testing-library";
import * as ReactRouter from "react-router-dom";

const makeRouter = (ReactRouter as any)["create" + "MemoryRouter"];
import { AuthProvider } from "../contexts/auth";
import { ChannelsProvider } from "../contexts/channels";
import { EventsProvider } from "../contexts/events";
import { ReadStatesProvider } from "../contexts/read-states";
import { FakeEventSource } from "../test/msw/sse";
import { resetMswState } from "../test/msw/server";
import { makeAttachment } from "../test/fixtures";
import { DEV_USER } from "../test/msw/handlers";
import { assertExists } from "../test/render";
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
  const router = makeRouter(
    [
      { path: "/threads", element: <ThreadsView /> },
      { path: "/channel/:id", element: <ChannelView /> },
    ],
    { initialEntries: [path] },
  );
  const history = {
    get: () => `${router.state.location.pathname}${router.state.location.search}`,
    set: ({ value }: { value: string }) => void router.navigate(value),
    back: () => void router.navigate(-1),
    forward: () => void router.navigate(1),
  };

  const result = render(() => (
    <AuthProvider>
      <EventsProvider>
        <ReadStatesProvider>
          <ChannelsProvider>
            <ReactRouter.RouterProvider router={router} />
          </ChannelsProvider>
        </ReadStatesProvider>
      </EventsProvider>
    </AuthProvider>
  ));

  return { ...result, history };
}

function expectAttributeEndsWith(element: Element, attribute: string, suffix: string) {
  const value = element.getAttribute(attribute);
  expect(value).not.toBeNull();
  expect(value?.endsWith(suffix)).toBe(true);
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
        mentions: [],
        attachments: [makeAttachment({ id: 9010, message_id: 10 })],
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
        mentions: [],
        attachments: [],
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
        mentions: [],
        attachments: [makeAttachment({ id: 9001, message_id: 20 })],
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
        mentions: [],
        attachments: [],
        embeds: [],
      },
      {
        id: 12,
        user_id: DEV_USER.id,
        channel_id: 100,
        parent_id: 10,
        created_at: 1_700_000_011_000_000,
        deleted_at: 1_700_000_012_000_000,
        text: "",
        username: DEV_USER.username,
        display_name: null,
        avatar_url: null,
        suppress_embeds: true,
        mentions: [],
        attachments: [makeAttachment({ id: 9011, message_id: 12 })],
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
        mentions: [],
        attachments: [],
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
        mentions: [],
        attachments: [],
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
        mentions: [],
        attachments: [makeAttachment({ id: 9002, message_id: 23 })],
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
        mentions: [],
        attachments: [],
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
        mentions: [],
        attachments: [makeAttachment({ id: 9003, message_id: 25 })],
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
        mentions: [],
        attachments: [],
        embeds: [],
      },
    ],
  };
  return state;
}

describe("Threads view integration", () => {
  test("renders hydrated mention labels in participated thread previews", async () => {
    const state = resetMswState();
    state.me = DEV_USER;
    state.channels = [{ id: 100, name: "general", position: 0, type: "text" }];
    const bob = {
      id: 2,
      username: "bob",
      display_name: "Bobby <Tables>",
      avatar_url: null,
    };
    state.messages = {
      "100": [
        {
          id: 10,
          user_id: DEV_USER.id,
          channel_id: 100,
          parent_id: null,
          created_at: 1_700_000_000_000_000,
          text: "root preview <@2>",
          username: DEV_USER.username,
          display_name: null,
          avatar_url: null,
          suppress_embeds: false,
          mentions: [bob],
          attachments: [],
          embeds: [],
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
          text: "reply preview <@2>",
          username: "bob",
          display_name: null,
          avatar_url: null,
          suppress_embeds: false,
          mentions: [bob],
          attachments: [],
          embeds: [],
        },
      ],
    };
    mountAt();

    const article = await screen.findByRole("article");
    expect(
      within(article).getByText(
        (_, element) => element?.textContent === "root preview @Bobby <Tables>",
      ),
    ).toBeInTheDocument();
    expect(
      within(article).getByText(
        (_, element) => element?.textContent === "reply preview @Bobby <Tables>",
      ),
    ).toBeInTheDocument();
    const mentionLabels = within(article).getAllByText("@Bobby <Tables>");
    expect(mentionLabels).toHaveLength(2);
    expect(mentionLabels[0]).toHaveAttribute("title", "@bob");
    expect(within(article).queryByText("<@2>")).toBeNull();
  });

  test("emphasizes participated previews from non-deleted root and recent-reply mention metadata", async () => {
    const state = resetMswState();
    state.me = DEV_USER;
    state.channels = [{ id: 100, name: "general", position: 0, type: "text" }];
    const selfUser = {
      id: DEV_USER.id,
      username: DEV_USER.username,
      display_name: DEV_USER.display_name,
      avatar_url: DEV_USER.avatar_url,
    };
    state.messages = {
      "100": [
        {
          id: 40,
          user_id: 2,
          channel_id: 100,
          parent_id: null,
          created_at: 1_700_000_020_000_000,
          text: "root ping <@1>",
          username: "bob",
          display_name: null,
          avatar_url: null,
          suppress_embeds: false,
          mentions: [selfUser],
          attachments: [],
          embeds: [],
        },
        {
          id: 50,
          user_id: DEV_USER.id,
          channel_id: 100,
          parent_id: null,
          created_at: 1_700_000_030_000_000,
          deleted_at: 1_700_000_031_000_000,
          text: "deleted root ping <@1>",
          username: DEV_USER.username,
          display_name: null,
          avatar_url: null,
          suppress_embeds: true,
          mentions: [selfUser],
          attachments: [],
          embeds: [],
        },
      ],
    };
    state.threadReplies = {
      "40": [
        {
          id: 41,
          user_id: DEV_USER.id,
          channel_id: 100,
          parent_id: 40,
          created_at: 1_700_000_021_000_000,
          text: "participant reply",
          username: DEV_USER.username,
          display_name: null,
          avatar_url: null,
          suppress_embeds: false,
          mentions: [],
          attachments: [],
          embeds: [],
        },
        {
          id: 42,
          user_id: 2,
          channel_id: 100,
          parent_id: 40,
          created_at: 1_700_000_022_000_000,
          text: "recent ping <@1>",
          username: "bob",
          display_name: null,
          avatar_url: null,
          suppress_embeds: false,
          mentions: [selfUser],
          attachments: [],
          embeds: [],
        },
      ],
      "50": [
        {
          id: 51,
          user_id: 2,
          channel_id: 100,
          parent_id: 50,
          created_at: 1_700_000_032_000_000,
          text: "normal reply",
          username: "bob",
          display_name: null,
          avatar_url: null,
          suppress_embeds: false,
          mentions: [],
          attachments: [],
          embeds: [],
        },
        {
          id: 52,
          user_id: 2,
          channel_id: 100,
          parent_id: 50,
          created_at: 1_700_000_033_000_000,
          deleted_at: 1_700_000_034_000_000,
          text: "deleted reply ping <@1>",
          username: "bob",
          display_name: null,
          avatar_url: null,
          suppress_embeds: true,
          mentions: [selfUser],
          attachments: [],
          embeds: [],
        },
      ],
    };
    mountAt();

    const rootText = await screen.findByText(
      (_, element) => element?.textContent === "root ping @baipas",
    );
    const rootPreview = assertExists(
      rootText.closest('[data-message-id="40"]') as HTMLElement | null,
      "mentioned root preview",
    );
    const mentionedArticle = assertExists(
      rootText.closest("article") as HTMLElement | null,
      "mentioned preview article",
    );
    await waitFor(() => {
      expect(mentionedArticle).toHaveAttribute("data-mentioned-current-user", "true");
      expect(rootPreview).toHaveAttribute("data-mentioned-current-user", "true");
    });
    expect(mentionedArticle).toHaveClass("border-yellow-300", "bg-yellow-50/40");
    expect(rootPreview).toHaveClass("bg-yellow-50", "ring-yellow-300");
    expect(
      within(rootPreview).getByRole("button", { name: "Mention baipas (@baipas)" }),
    ).toHaveClass("bg-yellow-100", "font-semibold");

    const recentText = await screen.findByText(
      (_, element) => element?.textContent === "recent ping @baipas",
    );
    const recentPreview = assertExists(
      recentText.closest('[data-message-id="42"]') as HTMLElement | null,
      "mentioned recent reply preview",
    );
    expect(recentPreview).toHaveAttribute("data-mentioned-current-user", "true");
    expect(recentPreview).toHaveClass("bg-yellow-50", "ring-yellow-300");

    const quietArticle = assertExists(
      (await screen.findByText("normal reply")).closest("article") as HTMLElement | null,
      "preview with only deleted mention metadata",
    );
    expect(quietArticle).not.toHaveAttribute("data-mentioned-current-user");
    const deletedRoot = assertExists(
      screen
        .getByText(/original message deleted/i)
        .closest('[data-message-id="50"]') as HTMLElement | null,
      "deleted root preview",
    );
    expect(deletedRoot).not.toHaveAttribute("data-mentioned-current-user");
    const deletedReply = assertExists(
      screen.getByText(/reply deleted/i).closest('[data-message-id="52"]') as HTMLElement | null,
      "deleted reply preview",
    );
    expect(deletedReply).not.toHaveAttribute("data-mentioned-current-user");
    expect(within(quietArticle).queryByRole("button", { name: /mention baipas/i })).toBeNull();
  });

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
    expectAttributeEndsWith(
      within(articles[0]).getByRole("img", { name: /photo attachment from bob/i }),
      "src",
      "/attachments/9001/thumbnail",
    );
    expectAttributeEndsWith(
      within(articles[0]).getByRole("img", { name: /photo attachment from charlie/i }),
      "src",
      "/attachments/9002/thumbnail",
    );
    expectAttributeEndsWith(
      within(articles[0]).getByRole("img", { name: /photo attachment from baipas/i }),
      "src",
      "/attachments/9003/thumbnail",
    );
    expect(
      within(articles[0]).getByRole("button", { name: /open photo attachment from bob/i }),
    ).toBeEnabled();
    expect(within(articles[0]).queryByText(/\.png/i)).toBeNull();
    expect(within(articles[0]).queryByText("old preview reply")).toBeNull();
    expect(within(articles[0]).queryByRole("button", { name: /reaction/i })).toBeNull();
    expect(within(articles[0]).queryByText("👍")).toBeNull();
    expect(within(articles[0]).queryByText("🔥")).toBeNull();

    expect(within(articles[1]).getByText("# general")).toBeInTheDocument();
    expect(within(articles[1]).getByText(/original message deleted/i)).toBeInTheDocument();
    expect(within(articles[1]).getByText("reply to deleted root")).toBeInTheDocument();
    expect(within(articles[1]).getByText(/reply deleted/i)).toBeInTheDocument();
    expect(within(articles[1]).queryByRole("list", { name: /photo attachment/i })).toBeNull();
    expect(within(articles[1]).queryByRole("img", { name: /photo/i })).toBeNull();
    expect(screen.queryByText("bob-only root")).toBeNull();
    expect(screen.queryByText("bob-only newest reply")).toBeNull();

    fireEvent.click(
      within(articles[0]).getByRole("link", { name: /open full thread in # random/i }),
    );

    const panel = await screen.findByRole("complementary", { name: /thread panel/i });
    await waitFor(() => expect(within(panel).getByText("root alice joined")).toBeInTheDocument());
    expect(within(panel).getByText("old preview reply")).toBeInTheDocument();
    expectAttributeEndsWith(
      within(panel).getByRole("img", { name: /photo attachment from bob/i }),
      "src",
      "/attachments/9001/thumbnail",
    );
    expectAttributeEndsWith(
      within(panel).getByRole("img", { name: /photo attachment from charlie/i }),
      "src",
      "/attachments/9002/thumbnail",
    );
    expectAttributeEndsWith(
      within(panel).getByRole("img", { name: /photo attachment from baipas/i }),
      "src",
      "/attachments/9003/thumbnail",
    );
    expect(history.get()).toBe("/channel/200?thread=20");
    expect(state.threadFetches).toContain(20);
  });
});
