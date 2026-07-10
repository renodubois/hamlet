import { beforeEach, describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "../test/testing-library";
import userEvent from "@testing-library/user-event";
import type { Message, PublicUser, SearchUsersOptions } from "../api";
import { expectNoA11yViolations } from "../test/a11y";
import { makeAttachment, makeMessage } from "../test/fixtures";
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
    addMessageReaction: vi
      .fn()
      .mockResolvedValue([
        { kind: "native", emoji: "👍", count: 1, me_reacted: true, reactors: ["You"] },
      ]),
    removeMessageReaction: vi.fn().mockResolvedValue([]),
  };
});

vi.mock("../contexts/custom-emojis", () => ({
  useOptionalCustomEmojis: () => customEmojiContext.current,
}));

import ChannelMessages, { channelMessageElementId } from "./channel-messages";
import {
  addMessageReaction,
  deleteMessage,
  editMessage,
  removeMessageReaction,
  setMessageEmbedsSuppressed,
} from "../api";

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

function mount(
  messages: Message[],
  currentUserId: number | null,
  onOpenThread?: (message: Message, options?: { focusComposer?: boolean }) => void,
  onReactionsChange?: (messageId: number, reactions: import("../api").ReactionSummary[]) => void,
  onReplyToMessage?: (message: Message) => void,
) {
  return render(() => (
    <ChannelMessages
      messages={messages}
      loading={false}
      error={null}
      currentUserId={currentUserId}
      onOpenThread={onOpenThread}
      onReactionsChange={onReactionsChange}
      onReplyToMessage={onReplyToMessage}
    />
  ));
}

function mountWithMentions(
  messages: Message[],
  options: {
    mentionUsers?: readonly PublicUser[];
    onMentionUsers?: (users: readonly PublicUser[]) => void;
    searchMentionUsers?: (options: SearchUsersOptions) => Promise<PublicUser[]>;
  },
) {
  return render(() => (
    <ChannelMessages
      messages={messages}
      loading={false}
      error={null}
      currentUserId={SELF_ID}
      mentionUsers={options.mentionUsers}
      onMentionUsers={options.onMentionUsers}
      searchMentionUsers={options.searchMentionUsers}
    />
  ));
}

function setInputSelection(input: HTMLInputElement, start: number, end = start) {
  input.focus();
  input.setSelectionRange(start, end);
  fireEvent.select(input);
}

beforeEach(() => {
  customEmojiContext.current = undefined;
  vi.clearAllMocks();
});

describe("<ChannelMessages> scroll layout", () => {
  test("bottom-anchors short histories without flex-end clipping overflowing histories", () => {
    const messages = Array.from({ length: 40 }, (_, index) =>
      makeMessage({
        id: 1_000 + index,
        user_id: OTHER_ID,
        channel_id: 1,
        text: `history message ${index + 1}`,
        username: "them",
      }),
    );

    mount(messages, SELF_ID);

    const messagesSection = assertExists(
      screen.getByText("history message 1").closest("section"),
      "messages section",
    );
    expect(messagesSection).toHaveClass("min-h-full", "flex", "flex-col");
    expect(messagesSection).not.toHaveClass("justify-end");

    const spacer = assertExists(messagesSection.firstElementChild, "bottom anchor spacer");
    expect(spacer).toHaveClass("mt-auto");
    expect(spacer).toHaveAttribute("aria-hidden", "true");
    expect(messagesSection.children[1]).toHaveAttribute("data-message-id", "1000");
  });
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

  test("preserves literal newlines with wrapping-friendly message text styles", () => {
    const multiline = makeMessage({
      id: 506,
      user_id: OTHER_ID,
      channel_id: 1,
      text: "first line\nsecond line\nthird line",
      username: "them",
    });

    mount([multiline], SELF_ID);

    const messageText = screen.getByText(
      (_, element) =>
        element?.textContent === "first line\nsecond line\nthird line" &&
        element.className.includes("whitespace-pre-wrap"),
    );
    expect(messageText.textContent).toBe("first line\nsecond line\nthird line");
    expect(messageText).toHaveClass(
      "whitespace-pre-wrap",
      "break-words",
      "[overflow-wrap:anywhere]",
    );
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

  test("linkifies URLs before and after newlines while preserving channel text line breaks", () => {
    const text = "before https://before.test\nhttps://solo.test/path\nafter https://after.test end";
    const multilineUrls = makeMessage({
      id: 507,
      user_id: OTHER_ID,
      channel_id: 1,
      text,
      username: "them",
    });

    mount([multilineUrls], SELF_ID);

    const before = screen.getByRole("link", { name: "https://before.test" });
    const messageText = assertExists(before.parentElement, "message text");
    expect(messageText.textContent).toBe(text);
    expect(messageText).toHaveClass(
      "whitespace-pre-wrap",
      "break-words",
      "[overflow-wrap:anywhere]",
    );
    expect(within(messageText).getByRole("link", { name: "https://before.test" })).toHaveAttribute(
      "href",
      "https://before.test",
    );
    expect(
      within(messageText).getByRole("link", { name: "https://solo.test/path" }),
    ).toHaveAttribute("href", "https://solo.test/path");
    expect(within(messageText).getByRole("link", { name: "https://after.test" })).toHaveAttribute(
      "href",
      "https://after.test",
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

  test("renders hydrated user mention markers as safe inline labels with fallbacks", () => {
    customEmojiContext.current = {
      byId: (id) =>
        id === 123
          ? {
              id: 123,
              name: "party",
              image_url: "/uploads/emojis/123.webp?v=1",
              animated: false,
              created_by_user_id: SELF_ID,
              created_at: 1,
              updated_at: 1,
              deleted_at: null,
            }
          : null,
    };
    const mentioned = makeMessage({
      id: 508,
      user_id: OTHER_ID,
      channel_id: 1,
      text: "hi <@2> and <:party:123> missing <@999> malformed <@abc> https://example.com/%3C@2%3E again <@2>",
      username: "them",
      mentions: [
        {
          id: 2,
          username: "bob",
          display_name: "Bobby <Tables>",
          avatar_url: null,
        },
      ],
    });

    mount([mentioned], SELF_ID);

    const mentionLabels = screen.getAllByText("@Bobby <Tables>");
    expect(mentionLabels).toHaveLength(2);
    expect(mentionLabels[0]).toHaveAccessibleName("Mention Bobby <Tables> (@bob)");
    expect(mentionLabels[0]).toHaveAttribute("title", "@bob");
    const messageText = assertExists(mentionLabels[0].closest(".whitespace-pre-wrap"));
    expect(screen.getByRole("img", { name: ":party:" })).toBeInTheDocument();
    expect(messageText).toHaveTextContent("<@999>");
    expect(messageText).toHaveTextContent("malformed <@abc>");
    expect(screen.getByRole("link", { name: "https://example.com/%3C@2%3E" })).toHaveAttribute(
      "href",
      "https://example.com/%3C@2%3E",
    );
  });

  test("does not render body mention controls for deleted tombstone messages", () => {
    const deletedMention = makeMessage({
      id: 509,
      user_id: OTHER_ID,
      channel_id: 1,
      text: "deleted body <@2>",
      username: "them",
      deleted_at: 1_700_000_100,
      mentions: [
        {
          id: 2,
          username: "bob",
          display_name: "Bobby",
          avatar_url: null,
        },
      ],
    });

    mount([deletedMention], SELF_ID);

    expect(screen.getByLabelText(/original message deleted/i)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /mention bobby/i })).toBeNull();
    expect(screen.queryByText("@Bobby")).toBeNull();
    expect(screen.queryByText(/deleted body/i)).toBeNull();
  });

  test("emphasizes non-deleted rows that mention the current user independently from authored styling", () => {
    const selfUser: PublicUser = {
      id: SELF_ID,
      username: "me",
      display_name: "Me",
      avatar_url: null,
    };
    const otherUser: PublicUser = {
      id: OTHER_ID,
      username: "them",
      display_name: null,
      avatar_url: null,
    };
    const mentionedByOther = makeMessage({
      id: 530,
      user_id: OTHER_ID,
      channel_id: 1,
      text: "ping <@1>",
      username: "them",
      mentions: [selfUser],
    });
    const mentionedOtherUser = makeMessage({
      id: 531,
      user_id: OTHER_ID,
      channel_id: 1,
      text: "ping <@2>",
      username: "them",
      mentions: [otherUser],
    });
    const authoredBySelf = makeMessage({
      id: 532,
      user_id: SELF_ID,
      channel_id: 1,
      text: "authored by me",
      username: "me",
    });
    const authoredAndMentioned = makeMessage({
      id: 533,
      user_id: SELF_ID,
      channel_id: 1,
      text: "self ping <@1>",
      username: "me",
      mentions: [selfUser],
    });
    const deletedMention = makeMessage({
      id: 534,
      user_id: OTHER_ID,
      channel_id: 1,
      text: "deleted ping <@1>",
      username: "them",
      deleted_at: 1_700_000_100,
      mentions: [selfUser],
    });

    mount(
      [mentionedByOther, mentionedOtherUser, authoredBySelf, authoredAndMentioned, deletedMention],
      SELF_ID,
    );

    const mentionedRow = assertExists(document.getElementById(channelMessageElementId(530)));
    expect(mentionedRow).toHaveAttribute("data-mentioned-current-user", "true");
    expect(mentionedRow).not.toHaveAttribute("data-authored-by-current-user");
    expect(mentionedRow).toHaveClass("bg-yellow-50", "ring-yellow-300", "border-yellow-300");
    expect(within(mentionedRow).getByRole("button", { name: "Mention Me (@me)" })).toHaveClass(
      "bg-yellow-100",
      "font-semibold",
    );

    const otherMentionRow = assertExists(document.getElementById(channelMessageElementId(531)));
    expect(otherMentionRow).not.toHaveAttribute("data-mentioned-current-user");
    expect(
      within(otherMentionRow).getByRole("button", { name: "Mention them (@them)" }),
    ).toHaveClass("bg-blue-100", "font-medium");

    const authoredRow = assertExists(document.getElementById(channelMessageElementId(532)));
    expect(authoredRow).toHaveAttribute("data-authored-by-current-user", "true");
    expect(authoredRow).not.toHaveAttribute("data-mentioned-current-user");
    expect(authoredRow).toHaveClass("border-blue-400", "bg-blue-50/50");

    const bothRow = assertExists(document.getElementById(channelMessageElementId(533)));
    expect(bothRow).toHaveAttribute("data-authored-by-current-user", "true");
    expect(bothRow).toHaveAttribute("data-mentioned-current-user", "true");
    expect(bothRow).toHaveClass("border-blue-400", "bg-yellow-50", "ring-yellow-300");

    const deletedRow = assertExists(document.getElementById(channelMessageElementId(534)));
    expect(deletedRow).not.toHaveAttribute("data-mentioned-current-user");
    expect(deletedRow).not.toHaveClass("bg-yellow-50", "ring-yellow-300");
    expect(within(deletedRow).queryByRole("button", { name: /mention me/i })).toBeNull();
  });
});

describe("<ChannelMessages> attachments", () => {
  test("renders multiple photo thumbnails in a constrained aspect-ratio grid", () => {
    const withPhotos = makeMessage({
      id: 520,
      user_id: OTHER_ID,
      channel_id: 1,
      text: "two photos",
      username: "them",
      attachments: [
        makeAttachment({
          id: 8101,
          message_id: 520,
          position: 0,
          thumbnail_width: 640,
          thumbnail_height: 480,
        }),
        makeAttachment({
          id: 8102,
          message_id: 520,
          position: 1,
          thumbnail_width: 300,
          thumbnail_height: 500,
        }),
      ],
    });

    mount([withPhotos], SELF_ID);

    const grid = screen.getByRole("list", { name: /2 photo attachments/i });
    expect(grid).toHaveClass("grid", "gap-2", "max-w-xl", "grid-cols-2");
    expect(screen.getByRole("img", { name: /photo 1 of 2 from them/i })).toHaveAttribute(
      "decoding",
      "async",
    );
    expect(screen.getByRole("img", { name: /photo 2 of 2 from them/i })).toBeInTheDocument();
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
    const openButtons = screen.getAllByRole("button", { name: /open photo \d of 2 from them/i });
    expect(openButtons).toHaveLength(2);
    expect(openButtons[0]).toHaveStyle({
      aspectRatio: "640 / 480",
      width: "448px",
      "max-width": "100%",
    });
    expect(openButtons[1]).toHaveStyle({
      aspectRatio: "300 / 500",
      width: "230px",
    });
  });

  test("shows an accessible fallback when a thumbnail image fails", () => {
    localStorage.setItem("hamlet.serverUrl", "http://127.0.0.1:3030");
    const withBrokenPhoto = makeMessage({
      id: 521,
      user_id: OTHER_ID,
      channel_id: 1,
      text: "broken photo",
      username: "them",
      attachments: [makeAttachment({ id: 8201, message_id: 521 })],
    });

    mount([withBrokenPhoto], SELF_ID);

    const image = screen.getByRole("img", { name: /photo attachment from them/i });
    fireEvent.error(image);

    expect(screen.getByText("Photo unavailable")).toBeInTheDocument();
    expect(
      screen.getByRole("img", { name: /photo attachment from them unavailable/i }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /open photo attachment from them/i })).toBeEnabled();
  });

  test("opens a full-size attachment preview in an in-app dialog", () => {
    localStorage.setItem("hamlet.serverUrl", "http://127.0.0.1:3030");
    const withPhoto = makeMessage({
      id: 522,
      user_id: OTHER_ID,
      channel_id: 1,
      text: "open photo",
      username: "them",
      attachments: [makeAttachment({ id: 8301, message_id: 522 })],
    });

    mount([withPhoto], SELF_ID);

    fireEvent.click(screen.getByRole("button", { name: /open photo attachment from them/i }));

    const dialog = screen.getByRole("dialog", { name: /photo attachment from them/i });
    expect(
      within(dialog).getByRole("img", { name: /photo attachment from them/i }),
    ).toHaveAttribute("src", "http://127.0.0.1:3030/attachments/8301");

    fireEvent.click(within(dialog).getByRole("button", { name: /close/i }));

    expect(screen.queryByRole("dialog", { name: /photo attachment from them/i })).toBeNull();
  });

  test("attachment thumbnails pass axe checks", async () => {
    const withPhoto = makeMessage({
      id: 522,
      user_id: OTHER_ID,
      channel_id: 1,
      text: "accessible photo",
      username: "them",
      display_name: "Casey",
      attachments: [makeAttachment({ id: 8301, message_id: 522 })],
    });
    const { container } = mount([withPhoto], SELF_ID);

    await expectNoA11yViolations(container, "message attachment thumbnails");
  });

  test("does not render attachments for deleted tombstones", () => {
    mount(
      [
        makeMessage({
          id: 523,
          user_id: OTHER_ID,
          channel_id: 1,
          text: "",
          username: "them",
          deleted_at: 1_700_000_100,
          attachments: [makeAttachment({ id: 8401, message_id: 523 })],
        }),
      ],
      SELF_ID,
    );

    expect(screen.getByLabelText(/original message deleted/i)).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: /photo attachment/i })).toBeNull();
    expect(screen.queryByRole("img", { name: /photo/i })).toBeNull();
  });
});

describe("<ChannelMessages> reactions", () => {
  test("renders native reaction pills with accessible pressed state", () => {
    mount(
      [
        makeMessage({
          id: 600,
          user_id: OTHER_ID,
          channel_id: 1,
          text: "reacted",
          username: "them",
          reactions: [{ kind: "native", emoji: "👍", count: 2, me_reacted: true }],
        }),
      ],
      SELF_ID,
    );

    const pill = screen.getByRole("button", { name: /👍 2 reactions\. remove your reaction/i });
    expect(pill).toHaveAttribute("aria-pressed", "true");
    expect(pill).toHaveTextContent("✓👍2");
  });

  test("renders custom reaction pills from summary image, label, and animation state", () => {
    mount(
      [
        makeMessage({
          id: 602,
          user_id: OTHER_ID,
          channel_id: 1,
          text: "custom reacted",
          username: "them",
          reactions: [
            {
              kind: "custom",
              emoji_id: 123,
              name: "dance",
              image_url: "/uploads/emojis/123.gif?v=2",
              animated: true,
              count: 3,
              me_reacted: false,
            },
          ],
        }),
      ],
      SELF_ID,
    );

    const pill = screen.getByRole("button", {
      name: /animated :dance: 3 reactions\. add your reaction/i,
    });
    expect(pill).toHaveAttribute("aria-pressed", "false");
    expect(assertExists(pill.querySelector("img")).getAttribute("src")).toContain(
      "/uploads/emojis/123.gif?v=2",
    );
    expect(pill).toHaveTextContent("3");
    expect(pill).not.toHaveTextContent("✓");
  });

  test("renders deleted custom reaction pills as visible but not addable", () => {
    const onReactionsChange = vi.fn();
    mount(
      [
        makeMessage({
          id: 606,
          user_id: OTHER_ID,
          channel_id: 1,
          text: "deleted custom reacted",
          username: "them",
          reactions: [
            {
              kind: "custom",
              emoji_id: 123,
              name: "ghost",
              image_url: "/uploads/emojis/123.webp?v=2",
              animated: false,
              deleted_at: 1_700_000_100,
              count: 2,
              me_reacted: false,
            },
          ],
        }),
      ],
      SELF_ID,
      undefined,
      onReactionsChange,
    );

    const pill = screen.getByRole("button", {
      name: /:ghost: \(deleted\) 2 reactions\. no longer available/i,
    });
    expect(pill).toBeDisabled();
    fireEvent.click(pill);
    expect(onReactionsChange).not.toHaveBeenCalled();
  });

  test("allows removing your own reaction to a deleted custom emoji", async () => {
    const onReactionsChange = vi.fn();
    mount(
      [
        makeMessage({
          id: 607,
          user_id: OTHER_ID,
          channel_id: 1,
          text: "my deleted custom reaction",
          username: "them",
          reactions: [
            {
              kind: "custom",
              emoji_id: 123,
              name: "ghost",
              image_url: "/uploads/emojis/123.webp?v=2",
              animated: false,
              deleted_at: 1_700_000_100,
              count: 1,
              me_reacted: true,
            },
          ],
        }),
      ],
      SELF_ID,
      undefined,
      onReactionsChange,
    );

    fireEvent.click(
      screen.getByRole("button", {
        name: /:ghost: \(deleted\) 1 reaction\. remove your reaction/i,
      }),
    );

    await waitFor(() =>
      expect(removeMessageReaction).toHaveBeenCalledWith(607, {
        kind: "custom",
        emoji_id: 123,
        name: "ghost",
        image_url: "/uploads/emojis/123.webp?v=2",
        animated: false,
      }),
    );
    expect(onReactionsChange).toHaveBeenNthCalledWith(1, 607, []);
  });

  test("shows capped reactor preview text on hover and keyboard focus", async () => {
    mount(
      [
        makeMessage({
          id: 604,
          user_id: OTHER_ID,
          channel_id: 1,
          text: "many reactions",
          username: "them",
          reactions: [
            {
              kind: "native",
              emoji: "👍",
              count: 6,
              me_reacted: true,
              reactors: ["You", "Alice", "Bob", "Carol", "Dana"],
            },
          ],
        }),
      ],
      SELF_ID,
    );

    const pill = screen.getByRole("button", {
      name: /👍 6 reactions\. remove your reaction/i,
    });
    expect(pill).toHaveAttribute("aria-describedby");
    expect(screen.queryByRole("tooltip")).toBeNull();

    fireEvent.mouseEnter(pill);
    expect(screen.getByRole("tooltip")).toHaveTextContent(
      "6 reactions: You, Alice, Bob, Carol, Dana and 1 more",
    );

    fireEvent.mouseLeave(pill);
    expect(screen.queryByRole("tooltip")).toBeNull();

    fireEvent.focus(pill);
    expect(screen.getByRole("tooltip")).toHaveTextContent(
      "6 reactions: You, Alice, Bob, Carol, Dana and 1 more",
    );
  });

  test("reaction pills pass axe checks with labels, pressed state, and preview descriptions", async () => {
    const { container } = mount(
      [
        makeMessage({
          id: 605,
          user_id: OTHER_ID,
          channel_id: 1,
          text: "accessible reactions",
          username: "them",
          reactions: [
            {
              kind: "native",
              emoji: "👍",
              count: 2,
              me_reacted: true,
              reactors: ["You", "Alice"],
            },
            {
              kind: "native",
              emoji: "❤️",
              count: 1,
              me_reacted: false,
              reactors: ["Alice"],
            },
          ],
        }),
      ],
      SELF_ID,
    );

    await expectNoA11yViolations(container, "reaction pills");
  });

  test("opens the reaction picker from a keyboard-focused Add Reaction button", async () => {
    const user = userEvent.setup();
    mount([otherMessage], SELF_ID);

    await user.tab();
    const addReaction = screen.getByRole("button", { name: /add reaction to message by them/i });
    expect(document.activeElement).toBe(addReaction);

    await user.keyboard("{Enter}");
    expect(await screen.findByRole("dialog", { name: /emoji picker/i })).toBeInTheDocument();

    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: /emoji picker/i })).toBeNull());

    addReaction.focus();
    await user.keyboard(" ");
    expect(await screen.findByRole("dialog", { name: /emoji picker/i })).toBeInTheDocument();
  });

  test("opens the emoji picker from Add Reaction and applies optimistic add", async () => {
    const onReactionsChange = vi.fn();
    mount([otherMessage], SELF_ID, undefined, onReactionsChange);

    fireEvent.click(screen.getByRole("button", { name: /add reaction to message by them/i }));
    const dialog = await screen.findByRole("dialog", { name: /emoji picker/i });
    fireEvent.input(within(dialog).getByRole("combobox", { name: /search and select emoji/i }), {
      target: { value: "thumb" },
    });
    const thumbsCell = await within(dialog).findByRole("gridcell", { name: /emoji :thumbsup:/i });
    fireEvent.click(within(thumbsCell).getByRole("button", { name: /emoji :thumbsup:/i }));

    await waitFor(() =>
      expect(addMessageReaction).toHaveBeenCalledWith(200, { kind: "native", emoji: "👍" }),
    );
    expect(onReactionsChange).toHaveBeenNthCalledWith(1, 200, [
      { kind: "native", emoji: "👍", count: 1, me_reacted: true, reactors: ["You"] },
    ]);
    expect(onReactionsChange).toHaveBeenLastCalledWith(200, [
      { kind: "native", emoji: "👍", count: 1, me_reacted: true, reactors: ["You"] },
    ]);
  });

  test("opens the reaction picker with active custom emojis and posts immutable custom ids", async () => {
    vi.mocked(addMessageReaction).mockResolvedValueOnce([
      {
        kind: "custom",
        emoji_id: 123,
        name: "party",
        image_url: "/uploads/emojis/123.webp?v=2",
        animated: false,
        count: 1,
        me_reacted: true,
        reactors: ["You"],
      },
    ]);
    customEmojiContext.current = {
      byId: (id) =>
        id === 123
          ? {
              id: 123,
              name: "party",
              image_url: "/uploads/emojis/123.webp?v=1",
              animated: false,
              created_by_user_id: SELF_ID,
              created_at: 1,
              updated_at: 1,
              deleted_at: null,
            }
          : null,
      activeEmojis: () => [
        {
          id: 123,
          name: "party",
          image_url: "/uploads/emojis/123.webp?v=1",
          animated: false,
          created_by_user_id: SELF_ID,
          created_at: 1,
          updated_at: 1,
          deleted_at: null,
        },
      ],
    };
    const onReactionsChange = vi.fn();
    mount([otherMessage], SELF_ID, undefined, onReactionsChange);

    fireEvent.click(screen.getByRole("button", { name: /add reaction to message by them/i }));
    const dialog = await screen.findByRole("dialog", { name: /emoji picker/i });
    fireEvent.input(within(dialog).getByRole("combobox", { name: /search and select emoji/i }), {
      target: { value: "party" },
    });
    const partyCell = await within(dialog).findByRole("gridcell", { name: /emoji :party:/i });
    fireEvent.click(within(partyCell).getByRole("button", { name: /emoji :party:/i }));

    await waitFor(() =>
      expect(addMessageReaction).toHaveBeenCalledWith(200, {
        kind: "custom",
        emoji_id: 123,
        name: "party",
        image_url: "/uploads/emojis/123.webp?v=1",
        animated: false,
      }),
    );
    expect(onReactionsChange).toHaveBeenNthCalledWith(1, 200, [
      {
        kind: "custom",
        emoji_id: 123,
        name: "party",
        image_url: "/uploads/emojis/123.webp?v=1",
        animated: false,
        deleted_at: null,
        count: 1,
        me_reacted: true,
        reactors: ["You"],
      },
    ]);
    expect(onReactionsChange).toHaveBeenLastCalledWith(200, [
      {
        kind: "custom",
        emoji_id: 123,
        name: "party",
        image_url: "/uploads/emojis/123.webp?v=2",
        animated: false,
        count: 1,
        me_reacted: true,
        reactors: ["You"],
      },
    ]);
  });

  test("toggles an existing pill and rolls back failed mutations", async () => {
    vi.mocked(removeMessageReaction).mockRejectedValueOnce(new Error("nope"));
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const onReactionsChange = vi.fn();
    const reacted = makeMessage({
      id: 601,
      user_id: OTHER_ID,
      channel_id: 1,
      text: "reacted",
      username: "them",
      reactions: [{ kind: "native", emoji: "👍", count: 1, me_reacted: true }],
    });

    mount([reacted], SELF_ID, undefined, onReactionsChange);
    fireEvent.click(screen.getByRole("button", { name: /👍 1 reaction\. remove your reaction/i }));

    await waitFor(() =>
      expect(removeMessageReaction).toHaveBeenCalledWith(601, { kind: "native", emoji: "👍" }),
    );
    expect(onReactionsChange).toHaveBeenNthCalledWith(1, 601, []);
    expect(onReactionsChange).toHaveBeenLastCalledWith(601, reacted.reactions);
    expect(errorSpy).toHaveBeenCalledWith("failed to update reaction", expect.any(Error));
    errorSpy.mockRestore();
  });

  test("toggles custom pills using the immutable custom id", async () => {
    const onReactionsChange = vi.fn();
    const reacted = makeMessage({
      id: 603,
      user_id: OTHER_ID,
      channel_id: 1,
      text: "custom reacted",
      username: "them",
      reactions: [
        {
          kind: "custom",
          emoji_id: 123,
          name: "dance",
          image_url: "/uploads/emojis/123.gif?v=2",
          animated: true,
          count: 1,
          me_reacted: true,
        },
      ],
    });

    mount([reacted], SELF_ID, undefined, onReactionsChange);
    fireEvent.click(
      screen.getByRole("button", { name: /animated :dance: 1 reaction\. remove your reaction/i }),
    );

    await waitFor(() =>
      expect(removeMessageReaction).toHaveBeenCalledWith(603, {
        kind: "custom",
        emoji_id: 123,
        name: "dance",
        image_url: "/uploads/emojis/123.gif?v=2",
        animated: true,
      }),
    );
    expect(onReactionsChange).toHaveBeenNthCalledWith(1, 603, []);
  });
});

describe("<ChannelMessages> hover action toolbar", () => {
  test("renders Add Reaction, Edit, and Delete buttons on the user's own message", async () => {
    mount([ownMessage], SELF_ID);
    expect(screen.getByRole("toolbar", { name: /message actions/i })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /add reaction to message by me/i }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^edit$/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^delete$/i })).toBeInTheDocument();
  });

  test("renders Add Reaction but not Edit/Delete on another user's message", async () => {
    mount([otherMessage], SELF_ID);
    expect(screen.getByRole("toolbar", { name: /message actions/i })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /add reaction to message by them/i }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^edit$/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /^delete$/i })).toBeNull();
  });

  test("does not render reaction rows or action toolbar for deleted tombstones", async () => {
    mount(
      [
        makeMessage({
          id: 204,
          user_id: OTHER_ID,
          channel_id: 1,
          text: "",
          username: "them",
          deleted_at: 1_700_000_100,
          reactions: [{ kind: "native", emoji: "👍", count: 2, me_reacted: true }],
          thread_summary: { reply_count: 1, last_reply_created_at: 1_700_000_200 },
        }),
      ],
      SELF_ID,
      vi.fn(),
    );

    expect(screen.getByLabelText(/original message deleted/i)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /👍/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /add reaction/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /reply in thread/i })).toBeNull();
    expect(screen.getByRole("button", { name: /open thread with 1 reply/i })).toBeInTheDocument();
  });

  test("does not render the toolbar when currentUserId is null", async () => {
    mount([ownMessage], null);
    expect(screen.queryByRole("toolbar", { name: /message actions/i })).toBeNull();
  });

  test("renders Add Reaction alongside a top-level reply-in-thread action", async () => {
    const onOpenThread = vi.fn();
    mount([otherMessage], SELF_ID, onOpenThread);
    expect(
      screen.getByRole("button", { name: /add reaction to message by them/i }),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /reply in thread to message by them/i }));
    expect(onOpenThread).toHaveBeenCalledWith(otherMessage, { focusComposer: true });
  });

  test("renders inline Reply for own and other top-level messages without replacing thread Reply", () => {
    const onOpenThread = vi.fn();
    const onReplyToMessage = vi.fn();
    mount([ownMessage, otherMessage], SELF_ID, onOpenThread, undefined, onReplyToMessage);

    fireEvent.click(screen.getByRole("button", { name: /reply inline to message by me/i }));
    fireEvent.click(screen.getByRole("button", { name: /reply inline to message by them/i }));

    expect(onReplyToMessage).toHaveBeenNthCalledWith(1, ownMessage);
    expect(onReplyToMessage).toHaveBeenNthCalledWith(2, otherMessage);
    expect(
      screen.getByRole("button", { name: /reply in thread to message by me/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /reply in thread to message by them/i }),
    ).toBeInTheDocument();
    expect(onOpenThread).not.toHaveBeenCalled();
  });

  test("reply toolbar labels identify rich referenced messages and pass axe", async () => {
    localStorage.setItem("hamlet.serverUrl", "http://hamlet.test:4040");
    const onOpenThread = vi.fn();
    const onReplyToMessage = vi.fn();
    const richMessage = makeMessage({
      ...otherMessage,
      id: 703,
      text: "message with photos, embeds, reactions, and a thread",
      attachments: [makeAttachment({ id: 8701, message_id: 703 })],
      embeds: [
        {
          id: 9701,
          message_id: 703,
          url: "https://example.com/rich",
          title: "Rich embed",
          description: "An embedded preview",
          image_url: null,
          site_name: "Example",
          embed_type: "link",
          iframe_url: null,
          iframe_width: null,
          iframe_height: null,
        },
      ],
      reactions: [{ kind: "native", emoji: "👍", count: 1, me_reacted: false }],
      thread_summary: { reply_count: 2, last_reply_created_at: 1_700_000_000_000_000 },
    });
    const { container } = mount([richMessage], SELF_ID, onOpenThread, undefined, onReplyToMessage);

    const toolbar = screen.getByRole("toolbar", {
      name: /message actions for message by them: message with photos, embeds, reactions, and a thread/i,
    });
    const inlineReply = within(toolbar).getByRole("button", {
      name: /reply inline to message by them: message with photos, embeds, reactions, and a thread/i,
    });
    const threadReply = within(toolbar).getByRole("button", {
      name: /reply in thread to message by them: message with photos, embeds, reactions, and a thread/i,
    });

    expect(inlineReply).toHaveTextContent("Reply");
    expect(threadReply).toHaveTextContent("Thread");
    fireEvent.click(inlineReply);
    fireEvent.click(threadReply);

    expect(onReplyToMessage).toHaveBeenCalledWith(richMessage);
    expect(onOpenThread).toHaveBeenCalledWith(richMessage, { focusComposer: true });
    await expectNoA11yViolations(container, "rich message reply toolbar");
  });

  test("inline Reply action is keyboard focusable and activatable", async () => {
    const user = userEvent.setup();
    const onReplyToMessage = vi.fn();
    mount([otherMessage], SELF_ID, undefined, undefined, onReplyToMessage);
    const button = screen.getByRole("button", { name: /reply inline to message by them/i });

    button.focus();
    expect(document.activeElement).toBe(button);

    await user.keyboard("{Enter}");
    expect(onReplyToMessage).toHaveBeenCalledWith(otherMessage);

    await user.keyboard(" ");
    expect(onReplyToMessage).toHaveBeenCalledTimes(2);
  });

  test("does not render inline Reply for deleted messages or thread replies", () => {
    const onReplyToMessage = vi.fn();
    mount(
      [
        makeMessage({
          id: 701,
          user_id: OTHER_ID,
          channel_id: 1,
          text: "",
          username: "them",
          deleted_at: 1_700_000_100,
        }),
        { ...otherMessage, id: 702, parent_id: ownMessage.id },
      ],
      SELF_ID,
      undefined,
      undefined,
      onReplyToMessage,
    );

    expect(screen.queryByRole("button", { name: /reply inline/i })).toBeNull();
  });

  test("reply-in-thread action is keyboard focusable", async () => {
    mount([otherMessage], SELF_ID, vi.fn());
    const button = screen.getByRole("button", { name: /reply in thread to message by them/i });

    button.focus();

    expect(document.activeElement).toBe(button);
  });

  test("does not render a reply-in-thread action for thread replies", async () => {
    const onOpenThread = vi.fn();
    mount([{ ...otherMessage, parent_id: ownMessage.id }], SELF_ID, onOpenThread);
    expect(screen.queryByRole("button", { name: /reply in thread/i })).toBeNull();
  });

  test("renders a thread summary button only when a root has replies", async () => {
    mount(
      [
        {
          ...otherMessage,
          thread_summary: { reply_count: 2, last_reply_created_at: 1_700_000_000_000_000 },
        },
        {
          ...ownMessage,
          thread_summary: { reply_count: 0, last_reply_created_at: 1_700_000_100_000_000 },
        },
      ],
      SELF_ID,
    );

    expect(
      screen.getByRole("button", {
        name: /open thread with 2 replies, last reply 2023-11-14 22:13 UTC/i,
      }),
    ).toHaveTextContent("2 replies · Last reply 2023-11-14 22:13 UTC");
    expect(screen.queryByText(/0 replies/i)).toBeNull();
  });

  test("clicking a thread summary opens that message's thread", async () => {
    const onOpenThread = vi.fn();
    const messageWithReplies: Message = {
      ...otherMessage,
      thread_summary: { reply_count: 1, last_reply_created_at: 1_700_000_000_000_000 },
    };
    mount([messageWithReplies], SELF_ID, onOpenThread);

    fireEvent.click(screen.getByRole("button", { name: /open thread with 1 reply/i }));

    expect(onOpenThread).toHaveBeenCalledWith(messageWithReplies, { focusComposer: false });
  });

  test("editing keeps reaction rows visible and hides the hover Add Reaction trigger", async () => {
    const reacted = makeMessage({
      ...ownMessage,
      reactions: [{ kind: "native", emoji: "👍", count: 1, me_reacted: false }],
    });
    mount([reacted], SELF_ID);

    fireEvent.click(screen.getByRole("button", { name: /^edit$/i }));
    expect(await screen.findByLabelText(/edit message/i)).toBeInTheDocument();

    expect(
      screen.getByRole("button", { name: /👍 1 reaction\. add your reaction/i }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /add reaction to message by me/i })).toBeNull();
    expect(screen.queryByRole("toolbar", { name: /message actions/i })).toBeNull();
  });

  test("renders inline reply preview before the reply body and passes axe", async () => {
    const reply = makeMessage({
      id: 609,
      user_id: SELF_ID,
      channel_id: 1,
      text: "reply body",
      username: "me",
      reply_to_message_id: otherMessage.id,
      reply_to: {
        id: otherMessage.id,
        user_id: otherMessage.user_id,
        channel_id: otherMessage.channel_id,
        created_at: 1_700_000_000_000_000,
        deleted_at: null,
        text: "target text that is previewed",
        attachment_count: 0,
        username: otherMessage.username,
        display_name: otherMessage.display_name,
        avatar_url: otherMessage.avatar_url,
      },
    });
    const { container } = mount([otherMessage, reply], SELF_ID);

    const preview = screen.getByLabelText(/replying to them/i);
    const body = screen.getByText("reply body");

    expect(preview).toHaveTextContent("them");
    expect(preview).toHaveTextContent("target text that is previewed");
    expect(preview.compareDocumentPosition(body) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    await expectNoA11yViolations(container, "inline reply preview");
  });

  test("clicking an inline reply preview jumps to the referenced message", () => {
    const reply = makeMessage({
      id: 618,
      user_id: SELF_ID,
      channel_id: 1,
      text: "reply body",
      username: "me",
      reply_to_message_id: otherMessage.id,
      reply_to: {
        id: otherMessage.id,
        user_id: otherMessage.user_id,
        channel_id: otherMessage.channel_id,
        created_at: 1_700_000_000_000_000,
        deleted_at: null,
        text: "target text that is previewed",
        attachment_count: 0,
        username: otherMessage.username,
        display_name: otherMessage.display_name,
        avatar_url: otherMessage.avatar_url,
      },
    });
    mount([otherMessage, reply], SELF_ID);

    const targetRow = assertExists(
      document.getElementById(channelMessageElementId(otherMessage.id)),
      "referenced message row",
    );
    const scrollIntoView = vi.fn();
    targetRow.scrollIntoView = scrollIntoView;

    expect(targetRow).toHaveAttribute("data-message-id", String(otherMessage.id));
    fireEvent.click(
      screen.getByRole("button", { name: /replying to them: target text that is previewed/i }),
    );

    expect(scrollIntoView).toHaveBeenCalledWith({ block: "center", inline: "nearest" });
  });

  test("can inline-reply to an inline reply and renders only the direct target", () => {
    const original = makeMessage({
      id: 611,
      user_id: OTHER_ID,
      channel_id: 1,
      text: "original text should not be in the nested preview",
      username: "them",
    });
    const inlineTarget = makeMessage({
      id: 612,
      user_id: SELF_ID,
      channel_id: 1,
      parent_id: null,
      text: "direct inline target",
      username: "me",
      reply_to_message_id: original.id,
      reply_to: {
        id: original.id,
        user_id: original.user_id,
        channel_id: original.channel_id,
        created_at: 1_700_000_000_000_000,
        text: original.text,
        username: original.username,
        display_name: original.display_name,
        avatar_url: original.avatar_url,
      },
    });
    const nestedReply = makeMessage({
      id: 613,
      user_id: OTHER_ID,
      channel_id: 1,
      parent_id: null,
      text: "nested inline body",
      username: "them",
      reply_to_message_id: inlineTarget.id,
      reply_to: {
        id: inlineTarget.id,
        user_id: inlineTarget.user_id,
        channel_id: inlineTarget.channel_id,
        created_at: 1_700_000_001_000_000,
        text: inlineTarget.text,
        username: inlineTarget.username,
        display_name: inlineTarget.display_name,
        avatar_url: inlineTarget.avatar_url,
      },
    });
    const onReplyToMessage = vi.fn();
    mount([inlineTarget, nestedReply], SELF_ID, undefined, undefined, onReplyToMessage);

    fireEvent.click(screen.getByRole("button", { name: /reply inline to message by me/i }));
    const preview = screen.getByLabelText(/replying to me/i);

    expect(onReplyToMessage).toHaveBeenCalledWith(inlineTarget);
    expect(preview).toHaveTextContent("direct inline target");
    expect(preview).not.toHaveTextContent("original text should not be in the nested preview");
  });

  test("uses an attachment fallback for attachment-only inline reply previews", () => {
    const reply = makeMessage({
      id: 614,
      user_id: SELF_ID,
      channel_id: 1,
      text: "replying to a photo",
      username: "me",
      reply_to_message_id: otherMessage.id,
      reply_to: {
        id: otherMessage.id,
        user_id: otherMessage.user_id,
        channel_id: otherMessage.channel_id,
        created_at: 1_700_000_000_000_000,
        text: "",
        attachment_count: 1,
        username: otherMessage.username,
        display_name: otherMessage.display_name,
        avatar_url: otherMessage.avatar_url,
      },
    });
    mount([reply], SELF_ID);

    const preview = screen.getByLabelText(/replying to them: attachment/i);
    expect(preview).toHaveTextContent("Attachment");
    expect(preview).not.toHaveTextContent("No text");
  });

  test("renders deleted and unavailable inline reply preview fallbacks", () => {
    const deletedTargetReply = makeMessage({
      id: 615,
      user_id: SELF_ID,
      channel_id: 1,
      text: "reply to deleted",
      username: "me",
      reply_to_message_id: otherMessage.id,
      reply_to: {
        id: otherMessage.id,
        user_id: otherMessage.user_id,
        channel_id: otherMessage.channel_id,
        created_at: 1_700_000_000_000_000,
        deleted_at: 1_700_000_100_000_000,
        text: "hidden target text",
        username: otherMessage.username,
        display_name: otherMessage.display_name,
        avatar_url: otherMessage.avatar_url,
      },
    });
    const missingTargetReply = makeMessage({
      id: 616,
      user_id: SELF_ID,
      channel_id: 1,
      text: "reply to missing",
      username: "me",
      reply_to_message_id: 999,
      reply_to: null,
    });
    const deletedReplyWithReference = makeMessage({
      id: 617,
      user_id: SELF_ID,
      channel_id: 1,
      text: "",
      username: "me",
      deleted_at: 1_700_000_200_000_000,
      reply_to_message_id: otherMessage.id,
      reply_to: {
        id: otherMessage.id,
        user_id: otherMessage.user_id,
        channel_id: otherMessage.channel_id,
        created_at: 1_700_000_000_000_000,
        text: "should not render",
        username: otherMessage.username,
        display_name: otherMessage.display_name,
        avatar_url: otherMessage.avatar_url,
      },
    });
    mount([deletedTargetReply, missingTargetReply, deletedReplyWithReference], SELF_ID);

    expect(screen.getByLabelText(/replying to deleted message by them/i)).toHaveTextContent(
      "Original message deleted",
    );
    expect(screen.getByLabelText(/replying to unavailable message 999/i)).toHaveTextContent(
      "Original message unavailable",
    );
    expect(screen.queryByText("hidden target text")).toBeNull();
    expect(screen.queryByText("should not render")).toBeNull();
  });

  test("renders message content in reference, text, attachments, embeds, reactions, thread summary order", () => {
    localStorage.setItem("hamlet.serverUrl", "http://hamlet.test:4040");
    const placed = makeMessage({
      id: 610,
      user_id: OTHER_ID,
      channel_id: 1,
      text: "body before attachments",
      username: "them",
      display_name: "Riley",
      reply_to_message_id: 700,
      reply_to: {
        id: 700,
        user_id: 70,
        channel_id: 1,
        created_at: 1_700_000_000_000_000,
        text: "referenced caption",
        username: "morgan",
        display_name: "Morgan",
        avatar_url: null,
      },
      attachments: [makeAttachment({ id: 7001, message_id: 610 })],
      embeds: [
        {
          id: 9001,
          message_id: 610,
          url: "https://example.com/post",
          title: "Example embed",
          description: "embed description",
          image_url: null,
          site_name: "Example",
          embed_type: "link",
          iframe_url: null,
          iframe_width: null,
          iframe_height: null,
        },
      ],
      reactions: [{ kind: "native", emoji: "👍", count: 1, me_reacted: false }],
      thread_summary: { reply_count: 1, last_reply_created_at: 1_700_000_000_000_000 },
    });
    mount([placed], SELF_ID, vi.fn());

    const preview = screen.getByLabelText(/replying to morgan: referenced caption/i);
    const body = screen.getByText("body before attachments");
    const attachments = screen.getByRole("list", { name: /1 photo attachment/i });
    const image = screen.getByRole("img", { name: /photo attachment from riley/i });
    const fullImageButton = screen.getByRole("button", {
      name: /open photo attachment from riley/i,
    });
    const embed = screen.getByRole("link", { name: "Example embed" });
    const reaction = screen.getByRole("button", { name: /👍 1 reaction\. add your reaction/i });
    const summary = screen.getByRole("button", { name: /open thread with 1 reply/i });

    expect(image).toHaveAttribute("src", "http://hamlet.test:4040/attachments/7001/thumbnail");
    expect(fullImageButton).toBeEnabled();
    expect(preview.compareDocumentPosition(body) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(
      body.compareDocumentPosition(attachments) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      attachments.compareDocumentPosition(embed) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(embed.compareDocumentPosition(reaction) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(
      reaction.compareDocumentPosition(summary) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
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

  test("clearing a photo message edit saves an empty caption instead of prompting delete", async () => {
    const photoMessage = makeMessage({
      ...ownMessage,
      text: "caption to clear",
      attachments: [makeAttachment({ id: 8501, message_id: ownMessage.id })],
    });
    mount([photoMessage], SELF_ID);
    fireEvent.click(screen.getByRole("button", { name: /^edit$/i }));
    const input = (await screen.findByLabelText(/edit message/i)) as HTMLInputElement;

    fireEvent.input(input, { target: { value: "" } });
    fireEvent.submit(assertExists(input.closest("form"), "form"));

    await waitFor(() => expect(editMessage).toHaveBeenCalledWith(photoMessage.id, ""));
    expect(screen.queryByRole("dialog", { name: /delete message/i })).toBeNull();
    expect(deleteMessage).not.toHaveBeenCalled();
  });

  test("Shift+Enter inserts a newline while editing and Enter saves exact multiline text", async () => {
    mount([ownMessage], SELF_ID);
    fireEvent.click(screen.getByRole("button", { name: /^edit$/i }));
    const input = (await screen.findByLabelText(/edit message/i)) as HTMLInputElement;
    input.setSelectionRange(ownMessage.text.length, ownMessage.text.length);

    fireEvent.keyDown(input, { key: "Enter", shiftKey: true });

    await waitFor(() => {
      expect(input.value).toBe(`${ownMessage.text}\n`);
      expect(input.selectionStart).toBe(`${ownMessage.text}\n`.length);
    });

    const multilineText = `${ownMessage.text}\nsecond line`;
    fireEvent.input(input, { target: { value: multilineText } });
    input.setSelectionRange(multilineText.length, multilineText.length);
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() => expect(editMessage).toHaveBeenCalledWith(ownMessage.id, multilineText));
  });

  test("Save button preserves exact multiline edit text", async () => {
    mount([ownMessage], SELF_ID);
    fireEvent.click(screen.getByRole("button", { name: /^edit$/i }));
    const input = (await screen.findByLabelText(/edit message/i)) as HTMLInputElement;
    const multilineText = "first visible line\nsecond visible line\nthird visible line";

    fireEvent.input(input, { target: { value: multilineText } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(editMessage).toHaveBeenCalledWith(ownMessage.id, multilineText));
  });

  test("Escape dismisses edit autocomplete before a second Escape cancels the edit", async () => {
    mount([ownMessage], SELF_ID);
    fireEvent.click(screen.getByRole("button", { name: /^edit$/i }));
    const input = (await screen.findByLabelText(/edit message/i)) as HTMLInputElement;
    const draftWithAutocomplete = `${ownMessage.text} :sm`;

    fireEvent.input(input, { target: { value: draftWithAutocomplete } });
    input.setSelectionRange(draftWithAutocomplete.length, draftWithAutocomplete.length);
    await screen.findByRole("listbox", { name: /emoji suggestions/i });

    fireEvent.keyDown(input, { key: "Escape" });

    await waitFor(() =>
      expect(screen.queryByRole("listbox", { name: /emoji suggestions/i })).toBeNull(),
    );
    expect(screen.getByLabelText(/edit message/i)).toBeInTheDocument();
    expect(editMessage).not.toHaveBeenCalled();

    fireEvent.keyDown(input, { key: "Escape" });

    await waitFor(() => expect(screen.queryByLabelText(/edit message/i)).toBeNull());
    expect(editMessage).not.toHaveBeenCalled();
    expect(screen.getByText(ownMessage.text)).toBeInTheDocument();
  });

  test("Escape cancels a multiline edit without saving", async () => {
    mount([ownMessage], SELF_ID);
    fireEvent.click(screen.getByRole("button", { name: /^edit$/i }));
    const input = (await screen.findByLabelText(/edit message/i)) as HTMLInputElement;

    fireEvent.input(input, { target: { value: `${ownMessage.text}\nunsaved` } });
    fireEvent.keyDown(input, { key: "Escape" });

    await waitFor(() => expect(screen.queryByLabelText(/edit message/i)).toBeNull());
    expect(editMessage).not.toHaveBeenCalled();
    expect(screen.getByText(ownMessage.text)).toBeInTheDocument();
  });

  test("unchanged multiline edits compare exact strings and skip PUT", async () => {
    const multiline = makeMessage({ ...ownMessage, text: "same first line\nsame second line" });
    mount([multiline], SELF_ID);
    fireEvent.click(screen.getByRole("button", { name: /^edit$/i }));
    const input = (await screen.findByLabelText(/edit message/i)) as HTMLInputElement;

    fireEvent.submit(assertExists(input.closest("form"), "form"));

    await waitFor(() => expect(screen.queryByLabelText(/edit message/i)).toBeNull());
    expect(editMessage).not.toHaveBeenCalled();
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

  test("editing a message searches, chips, and saves mention markers", async () => {
    const bob: PublicUser = {
      id: 2,
      username: "bob",
      display_name: "Bobby",
      avatar_url: null,
    };
    const message = makeMessage({ ...ownMessage, text: "hello <@2>" });
    const onMentionUsers = vi.fn();
    const searchMentionUsers = vi.fn(async () => [bob]);
    vi.mocked(editMessage).mockResolvedValueOnce({
      ...message,
      text: "hello <@2> and <@2> ",
      mentions: [bob],
    });

    mountWithMentions([message], {
      mentionUsers: [bob],
      onMentionUsers,
      searchMentionUsers,
    });
    fireEvent.click(screen.getByRole("button", { name: /^edit$/i }));

    const input = (await screen.findByLabelText(/edit message/i)) as HTMLInputElement;
    expect(input.value).toBe("hello <@2>");
    expect(within(input).getByText("@Bobby")).toHaveClass("bg-blue-100", "text-blue-800");

    const draft = "hello <@2> and @bo";
    fireEvent.input(input, { target: { value: draft } });
    setInputSelection(input, draft.length);
    const listbox = await screen.findByRole("listbox", { name: /mention suggestions/i });
    expect(
      within(listbox).getByRole("option", { name: /mention bobby @bob/i }),
    ).toBeInTheDocument();

    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(input.value).toBe("hello <@2> and <@2> "));
    expect(editMessage).not.toHaveBeenCalled();

    fireEvent.submit(assertExists(input.closest("form"), "form"));
    await waitFor(() =>
      expect(editMessage).toHaveBeenCalledWith(message.id, "hello <@2> and <@2> "),
    );
    expect(onMentionUsers).toHaveBeenCalledWith([bob]);
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

  test("renders embed cards aligned with linkified multiline URLs", async () => {
    const baseEmbed = assertExists(embeddedOwnMessage.embeds[0], "base embed");
    const embeddedMultilineMessage: Message = makeMessage({
      ...embeddedOwnMessage,
      text: "one https://one.test\ntwo https://two.test/path",
      embeds: [
        {
          ...baseEmbed,
          id: 9100,
          url: "https://one.test",
          title: "One link",
        },
        {
          ...baseEmbed,
          id: 9101,
          url: "https://two.test/path",
          title: "Two link",
        },
      ],
    });

    mount([embeddedMultilineMessage], SELF_ID);

    expect(screen.getByRole("link", { name: "https://one.test" })).toHaveAttribute(
      "href",
      "https://one.test",
    );
    expect(screen.getByRole("link", { name: "https://two.test/path" })).toHaveAttribute(
      "href",
      "https://two.test/path",
    );
    expect(screen.getByRole("link", { name: /one link/i })).toHaveAttribute(
      "href",
      "https://one.test",
    );
    expect(screen.getByRole("link", { name: /two link/i })).toHaveAttribute(
      "href",
      "https://two.test/path",
    );
  });

  test("does not render embeds when suppress_embeds is true", async () => {
    const suppressed: Message = {
      ...embeddedOwnMessage,
      suppress_embeds: true,
    };
    mount([suppressed], SELF_ID);
    expect(screen.queryByRole("link", { name: /example domain/i })).toBeNull();
  });

  test("keeps suppress-embed behavior unchanged for multiline URL messages", async () => {
    const suppressed: Message = {
      ...embeddedOwnMessage,
      text: "first https://example.com\nsecond line",
      suppress_embeds: true,
    };
    mount([suppressed], SELF_ID);

    expect(screen.getByRole("link", { name: "https://example.com" })).toHaveAttribute(
      "href",
      "https://example.com",
    );
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
