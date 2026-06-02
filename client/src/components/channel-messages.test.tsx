import { beforeEach, describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
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

import ChannelMessages from "./channel-messages";
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
) {
  return render(() => (
    <ChannelMessages
      messages={messages}
      loading={false}
      error={null}
      currentUserId={currentUserId}
      onOpenThread={onOpenThread}
      onReactionsChange={onReactionsChange}
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
        element.classList.contains("whitespace-pre-wrap"),
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

  test("renders reaction rows after body and embeds but before the thread summary", () => {
    const placed = makeMessage({
      id: 610,
      user_id: OTHER_ID,
      channel_id: 1,
      text: "body before embed",
      username: "them",
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

    const body = screen.getByText("body before embed");
    const embed = screen.getByRole("link", { name: "Example embed" });
    const reaction = screen.getByRole("button", { name: /👍 1 reaction\. add your reaction/i });
    const summary = screen.getByRole("button", { name: /open thread with 1 reply/i });

    expect(body.compareDocumentPosition(embed) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
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
