import { describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen, within } from "@solidjs/testing-library";
import type { Message } from "../api";
import { expectNoA11yViolations } from "../test/a11y";
import { makeMessage } from "../test/fixtures";
import { assertExists } from "../test/render";

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
    expect(editMessage).toHaveBeenCalledWith(ownMessage.id, "edited text");
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
