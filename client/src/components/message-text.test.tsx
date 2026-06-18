import { createSignal, type Setter } from "solid-js";
import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { describe, expect, test, vi } from "vitest";
import type { CustomEmoji, MentionUser } from "../api";
import { expectNoA11yViolations } from "../test/a11y";
import { assertExists } from "../test/render";
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

function customEmojiById(id: number): CustomEmoji | null {
  return id === PARTY.id ? PARTY : null;
}

describe("renderRichText", () => {
  test("renders mixed links, mentions, custom emoji, repeated markers, and malformed text safely", () => {
    const onMentionClick = vi.fn();
    render(() => (
      <div data-testid="rich-text">
        {renderRichText({
          text: "hi <@2> and <@1> <:party:123> missing <@999> <:ghost:999> malformed <@abc> javascript:alert(1) https://example.test/path again <@2>",
          mentions: [BOB, SELF],
          customEmojiById,
          currentUserId: SELF.id,
          onMentionClick,
        })}
      </div>
    ));

    const bobMentions = screen.getAllByRole("button", { name: "Mention Bobby (@bob)" });
    expect(bobMentions).toHaveLength(2);
    fireEvent.click(bobMentions[0]);
    expect(onMentionClick).toHaveBeenCalledWith(BOB, expect.any(MouseEvent));

    expect(screen.getByRole("button", { name: "Mention Casey (@casey)" })).toHaveClass(
      "bg-yellow-100",
      "text-yellow-900",
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
    expect(richText).toHaveTextContent("missing <@999>");
    expect(richText).toHaveTextContent("malformed <@abc>");
    expect(richText).toHaveTextContent("javascript:alert(1)");
    expect(screen.queryByRole("link", { name: /javascript/i })).toBeNull();
  });

  test("renders mentions as static labels when no click handler is provided", () => {
    render(() => (
      <div>
        {renderRichText({
          text: "hello <@2>",
          mentions: [BOB],
          customEmojiById,
          currentUserId: null,
        })}
      </div>
    ));

    expect(screen.queryByRole("button", { name: "Mention Bobby (@bob)" })).toBeNull();
    expect(screen.getByText("@Bobby")).toHaveAttribute("title", "@bob");
  });

  test("styles current-user mentions more strongly than other mentions with accessible names", () => {
    render(() => (
      <MessageText text="hello <@1> and <@2>" mentions={[SELF, BOB]} currentUserId={1} />
    ));

    expect(screen.getByRole("button", { name: "Mention Casey (@casey)" })).toHaveClass(
      "bg-yellow-100",
      "font-semibold",
      "text-yellow-900",
    );
    expect(screen.getByRole("button", { name: "Mention Bobby (@bob)" })).toHaveClass(
      "bg-blue-100",
      "font-medium",
      "text-blue-800",
    );
  });
});

describe("<MessageText> mention previews", () => {
  test("opens an accessible public profile preview from a mention control", async () => {
    localStorage.setItem("hamlet.serverUrl", "http://hamlet.test:4040");
    const { container } = render(() => (
      <MessageText text="hello <@2>" mentions={[BOB]} currentUserId={SELF.id} />
    ));

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

  test("falls back to username when a mentioned user has no display name", () => {
    render(() => <MessageText text="hello <@3>" mentions={[CAROL]} currentUserId={SELF.id} />);

    fireEvent.click(screen.getByRole("button", { name: "Mention carol (@carol)" }));

    const dialog = screen.getByRole("dialog", { name: "Profile preview for carol (@carol)" });
    expect(within(dialog).getByText("carol")).toBeInTheDocument();
    expect(within(dialog).getByText("@carol")).toBeInTheDocument();
  });

  test("switches previews and closes on Escape, outside click, and scroll", () => {
    render(() => (
      <MessageText text="hello <@2> then <@3>" mentions={[BOB, CAROL]} currentUserId={SELF.id} />
    ));

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

  test("closes on unmount", () => {
    const { unmount } = render(() => (
      <MessageText text="hello <@2>" mentions={[BOB]} currentUserId={SELF.id} />
    ));

    fireEvent.click(screen.getByRole("button", { name: "Mention Bobby (@bob)" }));
    expect(
      screen.getByRole("dialog", { name: "Profile preview for Bobby (@bob)" }),
    ).toBeInTheDocument();

    unmount();

    expect(screen.queryByRole("dialog")).toBeNull();
  });

  test("uses current hydrated public profile data for mention labels and open previews", async () => {
    localStorage.setItem("hamlet.serverUrl", "http://hamlet.test:4040");
    let setMentions: Setter<MentionUser[]> | undefined;
    render(() => {
      const [mentions, updateMentions] = createSignal<MentionUser[]>([
        { ...BOB, display_name: "Robert", avatar_url: "/avatars/bob-old.webp" },
      ]);
      setMentions = updateMentions;
      return <MessageText text="hello <@2>" mentions={mentions()} currentUserId={SELF.id} />;
    });

    fireEvent.click(screen.getByRole("button", { name: "Mention Robert (@bob)" }));
    expect(
      screen.getByRole("dialog", { name: "Profile preview for Robert (@bob)" }),
    ).toBeInTheDocument();

    setMentions?.([{ ...BOB, display_name: "Bobby Tables", avatar_url: "/avatars/bob-new.webp" }]);

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
