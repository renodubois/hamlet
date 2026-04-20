import { describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen, within } from "@solidjs/testing-library";
import { createResource } from "solid-js";
import type { Message } from "../api";
import { expectNoA11yViolations } from "../test/a11y";
import { assertExists } from "../test/render";

vi.mock("../api", async () => {
  const actual = await vi.importActual<typeof import("../api")>("../api");
  return {
    ...actual,
    editMessage: vi.fn().mockResolvedValue(undefined),
    deleteMessage: vi.fn().mockResolvedValue(undefined),
  };
});

import ChannelMessages from "./channel_messages";
import { deleteMessage, editMessage } from "../api";

const SELF_ID = 1;
const OTHER_ID = 2;

const ownMessage: Message = {
  id: 100,
  user_id: SELF_ID,
  channel_id: 1,
  text: "hello from me",
  username: "me",
  display_name: null,
  avatar_url: null,
};

const otherMessage: Message = {
  id: 200,
  user_id: OTHER_ID,
  channel_id: 1,
  text: "hello from them",
  username: "them",
  display_name: null,
  avatar_url: null,
};

async function mount(messages: Message[], currentUserId: number | null) {
  const [resource] = createResource(() => Promise.resolve(messages));
  const result = render(() => (
    <ChannelMessages messages={resource} currentUserId={currentUserId} />
  ));
  // Let the resource resolve so `messages()` returns data before assertions.
  await Promise.resolve();
  await Promise.resolve();
  return result;
}

describe("<ChannelMessages> hover action toolbar", () => {
  test("renders Edit and Delete buttons on the user's own message", async () => {
    await mount([ownMessage], SELF_ID);
    expect(screen.getByRole("toolbar", { name: /message actions/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^edit$/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^delete$/i })).toBeInTheDocument();
  });

  test("does not render the toolbar on another user's message", async () => {
    await mount([otherMessage], SELF_ID);
    expect(screen.queryByRole("toolbar", { name: /message actions/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /^edit$/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /^delete$/i })).toBeNull();
  });

  test("does not render the toolbar when currentUserId is null", async () => {
    await mount([ownMessage], null);
    expect(screen.queryByRole("toolbar", { name: /message actions/i })).toBeNull();
  });

  test("clicking Edit swaps the row into edit mode", async () => {
    await mount([ownMessage], SELF_ID);
    fireEvent.click(screen.getByRole("button", { name: /^edit$/i }));
    const input = (await screen.findByLabelText(/edit message/i)) as HTMLInputElement;
    expect(input.value).toBe(ownMessage.text);
  });

  test("editing a message via the toolbar calls editMessage", async () => {
    await mount([ownMessage], SELF_ID);
    fireEvent.click(screen.getByRole("button", { name: /^edit$/i }));
    const input = (await screen.findByLabelText(/edit message/i)) as HTMLInputElement;
    fireEvent.input(input, { target: { value: "edited text" } });
    fireEvent.submit(assertExists(input.closest("form"), "form"));
    expect(editMessage).toHaveBeenCalledWith(ownMessage.id, "edited text");
  });

  test("clicking Delete opens the confirm dialog and confirming calls deleteMessage", async () => {
    await mount([ownMessage], SELF_ID);
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
    await mount([named], SELF_ID);
    expect(screen.getByText("Them The Great")).toBeInTheDocument();
    expect(screen.queryByText("them")).toBeNull();
  });

  test("falls back to username when display_name is null", async () => {
    await mount([otherMessage], SELF_ID);
    expect(screen.getByText("them")).toBeInTheDocument();
  });

  test("has no a11y violations with a mix of own and other messages", async () => {
    const { container } = await mount([ownMessage, otherMessage], SELF_ID);
    await expectNoA11yViolations(container, "channel messages");
  });
});
