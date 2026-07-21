import { useState, type MouseEvent as ReactMouseEvent } from "react";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import type { Channel, CustomEmoji, MentionUser } from "../api";
import { expectNoA11yViolations } from "../test/a11y";
import { assertExists, renderNative } from "../test/render";
import MessageText, { renderRichText } from "./message-text";

const SELF: MentionUser = {
  id: 1,
  username: "casey",
  display_name: "Casey",
  avatar_url: null,
};

const BOB: MentionUser = {
  id: 2,
  username: "bob",
  display_name: "Bobby",
  avatar_url: "/avatars/bob.webp",
};

const CAROL: MentionUser = {
  id: 3,
  username: "carol",
  display_name: null,
  avatar_url: null,
};

const PARTY: CustomEmoji = {
  id: 123,
  name: "party",
  image_url: "/uploads/emojis/123.webp?v=1",
  animated: false,
  created_by_user_id: 1,
  created_at: 1,
  updated_at: 1,
  deleted_at: null,
};

const CHANNELS: Channel[] = [
  { id: 100, name: "general", position: 0, type: "text" },
  { id: 200, name: "voice", position: 1, type: "voice" },
];

function customEmojiById(id: number): CustomEmoji | null {
  return id === PARTY.id ? PARTY : null;
}

describe("renderRichText", () => {
  test("renders mixed links, mentions, custom emoji, repeated markers, and malformed text safely", () => {
    const onMentionClick = vi.fn();
    renderNative(
      <div data-testid="rich-text">
        {renderRichText({
          text: "hi <@2> and <@1> <:party:123> see <#100> voice <#200> missing <@999> <#999> <:ghost:999> malformed <@abc> javascript:alert(1) https://example.test/path again <@2>",
          mentions: [BOB, SELF],
          channels: CHANNELS,
          customEmojiById,
          currentUserId: SELF.id,
          onMentionClick,
        })}
      </div>,
    );

    const bobMentions = screen.getAllByRole("button", { name: "Mention Bobby (@bob)" });
    expect(bobMentions).toHaveLength(2);
    fireEvent.click(bobMentions[0]);
    expect(onMentionClick).toHaveBeenCalledWith(
      BOB,
      expect.objectContaining({ nativeEvent: expect.any(MouseEvent) }),
    );

    expect(screen.getByRole("button", { name: "Mention Casey (@casey)" })).toHaveClass(
      "bg-primary/20",
      "text-primary",
    );
    expect(screen.getByRole("img", { name: ":party:" })).toHaveAttribute("title", ":party:");
    expect(screen.getByText(":ghost:")).toHaveAttribute(
      "title",
      "Custom emoji <:ghost:999> is unavailable",
    );
    expect(screen.getByRole("link", { name: "https://example.test/path" })).toHaveAttribute(
      "href",
      "https://example.test/path",
    );

    const richText = screen.getByTestId("rich-text");
    const channelLink = screen.getByRole("link", { name: "#general" });
    expect(channelLink).toHaveAttribute("href", "/channel/100");
    expect(screen.getByText("#voice")).toHaveAttribute("title", "#voice (voice)");
    expect(richText).toHaveTextContent("missing <@999> <#999>");
    expect(richText).toHaveTextContent("malformed <@abc>");
    expect(richText).toHaveTextContent("javascript:alert(1)");
    expect(screen.queryByRole("link", { name: /javascript/i })).toBeNull();
  });

  test("renders channel mentions as static labels through MessageText props", () => {
    renderNative(<MessageText text="join <#100> then <#999>" channels={CHANNELS} />);

    expect(screen.getByRole("link", { name: "#general" })).toHaveAttribute("href", "/channel/100");
    expect(screen.getByText(/then <#999>/)).toBeInTheDocument();
  });

  test("renders mentions as static labels when no click handler is provided", () => {
    renderNative(
      <div>
        {renderRichText({
          text: "hello <@2>",
          mentions: [BOB],
          customEmojiById,
          currentUserId: null,
        })}
      </div>,
    );

    expect(screen.queryByRole("button", { name: "Mention Bobby (@bob)" })).toBeNull();
    expect(screen.getByText("@Bobby")).toHaveAttribute("title", "@bob");
  });

  test("styles current-user mentions more strongly than other mentions with accessible names", () => {
    renderNative(
      <MessageText text="hello <@1> and <@2>" mentions={[SELF, BOB]} currentUserId={1} />,
    );

    expect(screen.getByRole("button", { name: "Mention Casey (@casey)" })).toHaveClass(
      "bg-primary/20",
      "font-semibold",
      "text-primary",
    );
    expect(screen.getByRole("button", { name: "Mention Bobby (@bob)" })).toHaveClass(
      "bg-primary/10",
      "font-medium",
      "text-primary",
    );
  });
});

describe("<MessageText> mention previews", () => {
  test("opens an accessible public profile preview from a mention control", async () => {
    localStorage.setItem("hamlet.serverUrl", "http://hamlet.test:4040");
    const { container } = renderNative(
      <MessageText text="hello <@2>" mentions={[BOB]} currentUserId={SELF.id} />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Mention Bobby (@bob)" }));

    const dialog = screen.getByRole("dialog", { name: "Profile preview for Bobby (@bob)" });
    expect(within(dialog).getByText("Bobby")).toBeInTheDocument();
    expect(within(dialog).getByText("@bob")).toBeInTheDocument();
    const avatar = within(dialog).getByRole("img", { name: "Bobby's avatar" });
    expect(assertExists(avatar.querySelector("img"), "avatar image").getAttribute("src")).toBe(
      "http://hamlet.test:4040/avatars/bob.webp",
    );

    await expectNoA11yViolations(container, "mention preview");
  });

  test("passes the synthetic event with the clicked button as currentTarget", () => {
    let clickedAnchor: HTMLButtonElement | null = null;
    const onMentionClick = vi.fn(
      (_user: MentionUser, event: ReactMouseEvent<HTMLButtonElement>) => {
        clickedAnchor = event.currentTarget;
      },
    );
    renderNative(
      <MessageText text="hello <@2>" mentions={[BOB]} onMentionClick={onMentionClick} />,
    );

    const mention = screen.getByRole("button", { name: "Mention Bobby (@bob)" });
    fireEvent.click(mention);

    expect(onMentionClick).toHaveBeenCalledOnce();
    expect(clickedAnchor).toBe(mention);
    expect(screen.getByRole("dialog")).toHaveStyle({
      left: `${Math.max(8, Math.min(mention.getBoundingClientRect().left, window.innerWidth - 264))}px`,
    });
  });

  test("falls back to username when a mentioned user has no display name", () => {
    renderNative(<MessageText text="hello <@3>" mentions={[CAROL]} currentUserId={SELF.id} />);

    fireEvent.click(screen.getByRole("button", { name: "Mention carol (@carol)" }));

    const dialog = screen.getByRole("dialog", { name: "Profile preview for carol (@carol)" });
    expect(within(dialog).getByText("carol")).toBeInTheDocument();
    expect(within(dialog).getByText("@carol")).toBeInTheDocument();
  });

  test("switches previews and closes on Escape, outside click, and scroll", () => {
    renderNative(
      <MessageText text="hello <@2> then <@3>" mentions={[BOB, CAROL]} currentUserId={SELF.id} />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Mention Bobby (@bob)" }));
    expect(
      screen.getByRole("dialog", { name: "Profile preview for Bobby (@bob)" }),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Mention carol (@carol)" }));
    expect(screen.queryByRole("dialog", { name: "Profile preview for Bobby (@bob)" })).toBeNull();
    expect(
      screen.getByRole("dialog", { name: "Profile preview for carol (@carol)" }),
    ).toBeInTheDocument();

    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Mention Bobby (@bob)" }));
    fireEvent.pointerDown(document.body);
    expect(screen.queryByRole("dialog")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Mention Bobby (@bob)" }));
    fireEvent.scroll(window);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  test("closes when the mention or its anchor disappears on rerender", async () => {
    const { rerender } = renderNative(
      <MessageText text="hello <@2>" mentions={[BOB]} currentUserId={SELF.id} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Mention Bobby (@bob)" }));

    rerender(<MessageText text="hello" mentions={[BOB]} currentUserId={SELF.id} />);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    rerender(<MessageText text="hello <@2>" mentions={[BOB]} currentUserId={SELF.id} />);
    fireEvent.click(screen.getByRole("button", { name: "Mention Bobby (@bob)" }));
    rerender(<MessageText text="hello <@2>" mentions={[]} currentUserId={SELF.id} />);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  test("closes on unmount", () => {
    const { unmount } = renderNative(
      <MessageText text="hello <@2>" mentions={[BOB]} currentUserId={SELF.id} />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Mention Bobby (@bob)" }));
    expect(
      screen.getByRole("dialog", { name: "Profile preview for Bobby (@bob)" }),
    ).toBeInTheDocument();

    unmount();

    expect(screen.queryByRole("dialog")).toBeNull();
  });

  test("uses current hydrated public profile data for mention labels and open previews", async () => {
    localStorage.setItem("hamlet.serverUrl", "http://hamlet.test:4040");
    function Harness() {
      const [mentions, setMentions] = useState<MentionUser[]>([
        { ...BOB, display_name: "Robert", avatar_url: "/avatars/bob-old.webp" },
      ]);
      return (
        <>
          <button
            type="button"
            onClick={() =>
              setMentions([
                { ...BOB, display_name: "Bobby Tables", avatar_url: "/avatars/bob-new.webp" },
              ])
            }
          >
            hydrate profile
          </button>
          <MessageText text="hello <@2>" mentions={mentions} currentUserId={SELF.id} />
        </>
      );
    }
    renderNative(<Harness />);

    fireEvent.click(screen.getByRole("button", { name: "Mention Robert (@bob)" }));
    expect(
      screen.getByRole("dialog", { name: "Profile preview for Robert (@bob)" }),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "hydrate profile" }));

    await waitFor(() => {
      const dialog = screen.getByRole("dialog", {
        name: "Profile preview for Bobby Tables (@bob)",
      });
      expect(within(dialog).getByText("Bobby Tables")).toBeInTheDocument();
      expect(
        assertExists(
          within(dialog).getByRole("img", { name: "Bobby Tables's avatar" }).querySelector("img"),
          "updated avatar image",
        ).getAttribute("src"),
      ).toBe("http://hamlet.test:4040/avatars/bob-new.webp");
    });
    expect(screen.getByRole("button", { name: "Mention Bobby Tables (@bob)" })).toBeInTheDocument();
  });
});
