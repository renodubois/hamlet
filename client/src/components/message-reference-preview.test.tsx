import { beforeEach, describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen } from "@solidjs/testing-library";
import type { CustomEmoji } from "../api";

const customEmojiContext = vi.hoisted(() => ({
  current: undefined as
    | {
        byId: (id: number) => CustomEmoji | null;
      }
    | undefined,
}));

vi.mock("../contexts/custom-emojis", () => ({
  useOptionalCustomEmojis: () => customEmojiContext.current,
}));

import MessageReferencePreview, {
  messageReferencePreviewText,
  type MessageReferencePreviewSource,
} from "./message-reference-preview";

function reference(
  overrides: Partial<MessageReferencePreviewSource> = {},
): MessageReferencePreviewSource {
  return {
    id: 10,
    user_id: 2,
    channel_id: 100,
    created_at: 1_700_000_000_000_000,
    text: "target text",
    username: "casey",
    display_name: "Casey",
    avatar_url: null,
    ...overrides,
  };
}

beforeEach(() => {
  customEmojiContext.current = undefined;
  localStorage.clear();
});

describe("<MessageReferencePreview>", () => {
  test("renders author attribution, one-line truncation, and non-navigational text", () => {
    render(() => (
      <MessageReferencePreview
        reference={reference({ text: `${"long ".repeat(30)}https://example.com` })}
      />
    ));

    const preview = screen.getByLabelText(/replying to casey:/i);
    expect(preview).toHaveTextContent("Casey");
    expect(preview).toHaveTextContent("…");
    expect(preview.querySelector(".truncate")).not.toBeNull();
    expect(screen.queryByRole("link")).toBeNull();
  });

  test("renders known custom emoji and safely degrades missing markers", () => {
    localStorage.setItem("hamlet.serverUrl", "http://127.0.0.1:3030");
    customEmojiContext.current = {
      byId: (id) =>
        id === 123
          ? {
              id: 123,
              name: "renamed_party",
              image_url: "/uploads/emojis/123.webp?v=2",
              animated: false,
              created_by_user_id: 1,
              created_at: 1,
              updated_at: 2,
              deleted_at: null,
            }
          : null,
    };

    render(() => (
      <MessageReferencePreview
        reference={reference({ text: "custom <:party:123> missing <:ghost:999>" })}
      />
    ));

    expect(screen.getByRole("img", { name: ":renamed_party:" })).toHaveAttribute(
      "src",
      "http://127.0.0.1:3030/uploads/emojis/123.webp?v=2",
    );
    expect(screen.getByText(":ghost:")).toHaveAttribute(
      "title",
      "Custom emoji <:ghost:999> is unavailable",
    );
    expect(screen.getByLabelText(/custom :party: missing :ghost:/i)).toBeInTheDocument();
  });

  test("renders attachment, deleted, and unavailable fallbacks with accessible labels", () => {
    const { unmount } = render(() => (
      <MessageReferencePreview reference={reference({ text: "", attachment_count: 2 })} />
    ));
    expect(screen.getByLabelText(/replying to casey: 2 attachments/i)).toHaveTextContent(
      "2 attachments",
    );
    unmount();

    const deleted = render(() => (
      <MessageReferencePreview
        reference={reference({ text: "hidden text", deleted_at: 1_700_000_100_000_000 })}
      />
    ));
    expect(screen.getByLabelText(/replying to deleted message by casey/i)).toHaveTextContent(
      "Original message deleted",
    );
    expect(screen.queryByText("hidden text")).toBeNull();
    deleted.unmount();

    render(() => <MessageReferencePreview reference={null} targetId={99} />);
    expect(screen.getByLabelText(/replying to unavailable message 99/i)).toHaveTextContent(
      "Original message unavailable",
    );
  });

  test("renders as a button and invokes activation when interactive", () => {
    const onActivate = vi.fn();
    render(() => <MessageReferencePreview reference={reference()} onActivate={onActivate} />);

    fireEvent.click(screen.getByRole("button", { name: /replying to casey: target text/i }));

    expect(onActivate).toHaveBeenCalledOnce();
  });

  test("plain preview text normalizes custom emoji markers for labels", () => {
    expect(messageReferencePreviewText(reference({ text: "hello\n<:party:123>" }))).toBe(
      "hello :party:",
    );
  });
});
