import { beforeEach, describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import type { Message } from "../api";
import { expectNoA11yViolations } from "../test/a11y";
import { makeMessage } from "../test/fixtures";
import { assertExists } from "../test/render";

const customEmojiContext = vi.hoisted(() => ({
  current: undefined as
    | {
        byId: (id: number) => import("../api").CustomEmoji | null;
        activeEmojis?: () => import("../api").CustomEmoji[];
      }
    | undefined,
}));

vi.mock("../api", async () => {
  const actual = await vi.importActual<typeof import("../api")>("../api");
  return {
    ...actual,
    editMessage: vi.fn().mockResolvedValue(undefined),
    deleteMessage: vi.fn().mockResolvedValue(undefined),
    setMessageEmbedsSuppressed: vi.fn().mockResolvedValue({
      id: 0,
      channel_id: 0,
      suppress_embeds: true,
      embeds: [],
    }),
  };
});

vi.mock("../contexts/custom-emojis", () => ({
  useOptionalCustomEmojis: () => customEmojiContext.current,
}));

import ChannelMessages from "./channel-messages";
import { deleteMessage, editMessage, setMessageEmbedsSuppressed } from "../api";

const SELF_ID = 1;
const OTHER_ID = 2;

const ownMessage: Message = makeMessage({
  id: 100,
  user_id: SELF_ID,
  channel_id: 1,
  text: "hello from me",
  username: "me",
  display_name: null,
  avatar_url: null,
});

const otherMessage: Message = makeMessage({
  id: 200,
  user_id: OTHER_ID,
  channel_id: 1,
  text: "hello from them",
  username: "them",
  display_name: null,
  avatar_url: null,
});

function mount(messages: Message[], currentUserId: number | null) {
  return render(() => (
    <ChannelMessages
      messages={messages}
      loading={false}
      error={null}
      currentUserId={currentUserId}
    />
  ));
}

beforeEach(() => {
  customEmojiContext.current = undefined;
  vi.clearAllMocks();
});

describe("<ChannelMessages> message text rendering", () => {
  test("renders stored literal emoji shortcodes without converting them", () => {
    const legacyShortcode = makeMessage({
      id: 500,
      user_id: OTHER_ID,
      channel_id: 1,
      text: "legacy :grinning: shortcode",
      username: "them",
    });

    mount([legacyShortcode], SELF_ID);

    expect(screen.getByText("legacy :grinning: shortcode")).toBeInTheDocument();
    expect(screen.queryByText("legacy 😀 shortcode")).toBeNull();
  });

  test("preserves emoji glyphs while linkifying nearby URLs", () => {
    const linkedEmoji = makeMessage({
      id: 501,
      user_id: OTHER_ID,
      channel_id: 1,
      text: "glyph 😀 and literal :grinning: before https://example.com after",
      username: "them",
    });

    mount([linkedEmoji], SELF_ID);

    const link = screen.getByRole("link", { name: "https://example.com" });
    expect(link).toHaveAttribute("href", "https://example.com");
    expect(assertExists(link.parentElement, "message text")).toHaveTextContent(
      "glyph 😀 and literal :grinning: before https://example.com after",
    );
  });

  test("renders embeds for emoji-containing messages", () => {
    const embeddedEmoji = makeMessage({
      id: 502,
      user_id: SELF_ID,
      channel_id: 1,
      text: "emoji 😀 preview https://example.com",
      username: "me",
      embeds: [
        {
          id: 9500,
          message_id: 502,
          url: "https://example.com",
          title: "Example domain",
          description: "A description.",
          image_url: null,
          site_name: "Example",
          embed_type: "link",
          iframe_url: null,
          iframe_width: null,
          iframe_height: null,
        },
      ],
    });

    mount([embeddedEmoji], SELF_ID);

    const messageLink = screen.getByRole("link", { name: "https://example.com" });
    expect(assertExists(messageLink.parentElement, "message text")).toHaveTextContent(
      "emoji 😀 preview https://example.com",
    );
    expect(screen.getByRole("link", { name: /example domain/i })).toHaveAttribute(
      "href",
      "https://example.com",
    );
  });

  test("renders known static and animated custom emoji markers as images by id", () => {
    customEmojiContext.current = {
      byId: (id) => {
        if (id === 123) {
          return {
            id: 123,
            name: "renamed_party",
            image_url: "/uploads/emojis/123.webp?v=2",
            animated: false,
            created_by_user_id: 1,
            created_at: 1,
            updated_at: 2,
            deleted_at: null,
          };
        }
        if (id === 456) {
          return {
            id: 456,
            name: "dance",
            image_url: "/uploads/emojis/456.gif?v=3",
            animated: true,
            created_by_user_id: 1,
            created_at: 1,
            updated_at: 3,
            deleted_at: null,
          };
        }
        return null;
      },
    };
    const customEmojiMessage = makeMessage({
      id: 503,
      user_id: OTHER_ID,
      channel_id: 1,
      text: "old name <:party:123> and animated <a:dance:456> stay by id",
      username: "them",
    });

    mount([customEmojiMessage], SELF_ID);

    const image = screen.getByRole("img", { name: ":renamed_party:" });
    expect(image).toHaveAttribute("title", ":renamed_party:");
    expect(image.getAttribute("src")).toContain("/uploads/emojis/123.webp?v=2");

    const animatedImage = screen.getByRole("img", { name: ":dance:" });
    expect(animatedImage).toHaveAttribute("title", ":dance:");
    expect(animatedImage.getAttribute("src")).toContain("/uploads/emojis/456.gif?v=3");
  });

  test("renders soft-deleted custom emoji markers in old messages", () => {
    customEmojiContext.current = {
      byId: (id) =>
        id === 123
          ? {
              id: 123,
              name: "party",
              image_url: "/uploads/emojis/123.webp?v=3",
              animated: false,
              created_by_user_id: 1,
              created_at: 1,
              updated_at: 3,
              deleted_at: 4,
            }
          : null,
    };
    const customEmojiMessage = makeMessage({
      id: 505,
      user_id: OTHER_ID,
      channel_id: 1,
      text: "old <:party:123> still renders",
      username: "them",
    });

    mount([customEmojiMessage], SELF_ID);

    const image = screen.getByRole("img", { name: ":party:" });
    expect(image).toHaveAttribute("title", ":party: (deleted)");
    expect(image.getAttribute("src")).toContain("/uploads/emojis/123.webp?v=3");
  });

  test("does not parse custom emoji markers inside link tokens and falls back for unknown ids", () => {
    customEmojiContext.current = { byId: () => null };
    const customEmojiMessage = makeMessage({
      id: 504,
      user_id: OTHER_ID,
      channel_id: 1,
      text: "missing <:ghost:999> link https://example.com/%3C:party:123%3E",
      username: "them",
    });

    mount([customEmojiMessage], SELF_ID);

    expect(screen.getByText(":ghost:")).toHaveAttribute(
      "title",
      "Custom emoji <:ghost:999> is unavailable",
    );
    expect(screen.queryByRole("img", { name: /party/i })).toBeNull();
    expect(
      screen.getByRole("link", { name: "https://example.com/%3C:party:123%3E" }),
    ).toHaveAttribute("href", "https://example.com/%3C:party:123%3E");
  });
});

describe("<ChannelMessages> hover action toolbar", () => {
  test("renders Edit and Delete buttons on the user's own message", async () => {
    mount([ownMessage], SELF_ID);
    expect(screen.getByRole("toolbar", { name: /message actions/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^edit$/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^delete$/i })).toBeInTheDocument();
  });

  test("does not render the toolbar on another user's message", async () => {
    mount([otherMessage], SELF_ID);
    expect(screen.queryByRole("toolbar", { name: /message actions/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /^edit$/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /^delete$/i })).toBeNull();
  });

  test("does not render the toolbar when currentUserId is null", async () => {
    mount([ownMessage], null);
    expect(screen.queryByRole("toolbar", { name: /message actions/i })).toBeNull();
  });

  test("clicking Edit swaps the row into edit mode", async () => {
    mount([ownMessage], SELF_ID);
    fireEvent.click(screen.getByRole("button", { name: /^edit$/i }));
    const input = (await screen.findByLabelText(/edit message/i)) as HTMLInputElement;
    expect(input.value).toBe(ownMessage.text);
  });

  test("editing a message via the toolbar calls editMessage", async () => {
    mount([ownMessage], SELF_ID);
    fireEvent.click(screen.getByRole("button", { name: /^edit$/i }));
    const input = (await screen.findByLabelText(/edit message/i)) as HTMLInputElement;
    fireEvent.input(input, { target: { value: "edited text" } });
    fireEvent.submit(assertExists(input.closest("form"), "form"));
    await waitFor(() => expect(editMessage).toHaveBeenCalledWith(ownMessage.id, "edited text"));
  });

  test("editing a message displays custom emoji markers as chips and preserves PUT text", async () => {
    const party = {
      id: 123,
      name: "party",
      image_url: "/uploads/emojis/123.webp?v=1",
      animated: false,
      created_by_user_id: SELF_ID,
      created_at: 1,
      updated_at: 1,
      deleted_at: null,
    };
    customEmojiContext.current = {
      byId: (id) => (id === 123 ? party : null),
      activeEmojis: () => [party],
    };
    const message = makeMessage({ ...ownMessage, text: "hello <:party:123>" });

    mount([message], SELF_ID);
    fireEvent.click(screen.getByRole("button", { name: /^edit$/i }));

    const input = (await screen.findByLabelText(/edit message/i)) as HTMLInputElement;
    expect(input.value).toBe("hello <:party:123>");
    expect(screen.getByRole("img", { name: /custom emoji :party:/i })).toBeInTheDocument();
    expect(screen.queryByText("<:party:123>")).toBeNull();

    expect(editMessage).not.toHaveBeenCalled();

    fireEvent.input(input, { target: { value: "hello <:party:123>!" } });
    fireEvent.submit(assertExists(input.closest("form"), "form"));
    await waitFor(() =>
      expect(editMessage).toHaveBeenCalledWith(message.id, "hello <:party:123>!"),
    );
  });

  test("clicking Delete opens the confirm dialog and confirming calls deleteMessage", async () => {
    mount([ownMessage], SELF_ID);
    fireEvent.click(screen.getByRole("button", { name: /^delete$/i }));
    const dialog = await screen.findByRole("dialog", { name: /delete message/i });
    expect(dialog).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    expect(deleteMessage).toHaveBeenCalledWith(ownMessage.id);
  });

  test("renders the display_name in place of username when set", async () => {
    const named: Message = {
      ...otherMessage,
      display_name: "Them The Great",
    };
    mount([named], SELF_ID);
    expect(screen.getByText("Them The Great")).toBeInTheDocument();
    expect(screen.queryByText("them")).toBeNull();
  });

  test("falls back to username when display_name is null", async () => {
    mount([otherMessage], SELF_ID);
    expect(screen.getByText("them")).toBeInTheDocument();
  });

  test("has no a11y violations with a mix of own and other messages", async () => {
    const { container } = mount([ownMessage, otherMessage], SELF_ID);
    await expectNoA11yViolations(container, "channel messages");
  });
});

describe("<ChannelMessages> embeds", () => {
  const embeddedOwnMessage: Message = makeMessage({
    id: 300,
    user_id: SELF_ID,
    channel_id: 1,
    text: "check this https://example.com",
    username: "me",
    embeds: [
      {
        id: 9000,
        message_id: 300,
        url: "https://example.com",
        title: "Example domain",
        description: "A description.",
        image_url: null,
        site_name: "Example",
        embed_type: "link",
        iframe_url: null,
        iframe_width: null,
        iframe_height: null,
      },
    ],
  });

  const embeddedOtherMessage: Message = makeMessage({
    id: 400,
    user_id: OTHER_ID,
    channel_id: 1,
    text: "look: https://example.com",
    username: "them",
    embeds: [
      {
        id: 9001,
        message_id: 400,
        url: "https://example.com",
        title: "Example domain",
        description: null,
        image_url: null,
        site_name: null,
        embed_type: "link",
        iframe_url: null,
        iframe_width: null,
        iframe_height: null,
      },
    ],
  });

  test("renders the embed title and description", async () => {
    mount([embeddedOwnMessage], SELF_ID);
    expect(screen.getByRole("link", { name: /example domain/i })).toBeInTheDocument();
    expect(screen.getByText("A description.")).toBeInTheDocument();
  });

  test("does not render embeds when suppress_embeds is true", async () => {
    const suppressed: Message = {
      ...embeddedOwnMessage,
      suppress_embeds: true,
    };
    mount([suppressed], SELF_ID);
    expect(screen.queryByRole("link", { name: /example domain/i })).toBeNull();
  });

  test("shows the Remove embed button only on the author's own messages", async () => {
    mount([embeddedOwnMessage, embeddedOtherMessage], SELF_ID);
    const removes = screen.getAllByRole("button", { name: /remove embed/i });
    // One button — only the author's embed is removable.
    expect(removes.length).toBe(1);
  });

  test("clicking Remove embed calls setMessageEmbedsSuppressed with suppress=true", async () => {
    mount([embeddedOwnMessage], SELF_ID);
    fireEvent.click(screen.getByRole("button", { name: /remove embed/i }));
    expect(setMessageEmbedsSuppressed).toHaveBeenCalledWith(embeddedOwnMessage.id, true);
  });
});
