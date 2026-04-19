import { describe, expect, test, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@solidjs/testing-library";
import { MemoryRouter, Route, createMemoryHistory } from "@solidjs/router";
import { AuthProvider } from "../auth_context";
import { ChannelsProvider } from "../channels_context";
import { EventsProvider } from "../events_context";
import { FakeEventSource, latestFakeEventSource } from "../test/msw/sse";
import { mswState, resetMswState } from "../test/msw/server";
import { DEV_USER } from "../test/msw/handlers";
import { assertExists } from "../test/render";
import ChannelView from "./channel";
import type { Message } from "../api";

vi.mock("../api", async () => {
  const actual = await vi.importActual<typeof import("../api")>("../api");
  return {
    ...actual,
    messagesEventSource: () => new FakeEventSource("/mock/messages") as unknown as EventSource,
  };
});

function mountAt(path: string) {
  const history = createMemoryHistory();
  history.set({ value: path });

  return render(() => (
    <AuthProvider>
      <EventsProvider>
        <ChannelsProvider>
          <MemoryRouter history={history}>
            <Route path="/channel/:id" component={ChannelView} />
          </MemoryRouter>
        </ChannelsProvider>
      </EventsProvider>
    </AuthProvider>
  ));
}

function seedAuthed() {
  const state = resetMswState();
  state.me = DEV_USER;
  state.messages["100"] = [
    { id: 1, user_id: 1, channel_id: 100, text: "hello", username: "alice", avatar_url: null },
    { id: 2, user_id: 2, channel_id: 100, text: "world", username: "bob", avatar_url: null },
  ];
  return state;
}

function nextMessageId(prev: Message[]): number {
  return prev.reduce((max, m) => Math.max(max, m.id), 0) + 1;
}

describe("Channel view integration", () => {
  test("loads initial messages from the server", async () => {
    seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => {
      expect(screen.getByText("hello")).toBeInTheDocument();
      expect(screen.getByText("world")).toBeInTheDocument();
    });
  });

  test("appends a message delivered over SSE", async () => {
    seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessage({
      id: nextMessageId([]),
      user_id: 3,
      channel_id: 100,
      text: "hot off the wire",
      username: "carol",
      avatar_url: null,
    });

    await waitFor(() => {
      expect(screen.getByText("hot off the wire")).toBeInTheDocument();
    });
  });

  test("ignores SSE messages destined for other channels", async () => {
    seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessage({
      id: 99,
      user_id: 3,
      channel_id: 999,
      text: "do not show",
      username: "carol",
      avatar_url: null,
    });

    // Give reactivity a tick to flush; then assert nothing appeared.
    await new Promise((r) => setTimeout(r, 10));
    expect(screen.queryByText("do not show")).toBeNull();
  });

  test("POSTs to /message/:id when the form is submitted", async () => {
    seedAuthed();
    mountAt("/channel/100");

    const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
    fireEvent.input(input, { target: { value: "typed message" } });
    const form = assertExists(input.closest("form"), "form");
    fireEvent.submit(form);

    await waitFor(() => {
      expect(mswState().sentMessages).toContainEqual({
        channel: "100",
        text: "typed message",
      });
    });
  });

  test("right-clicking own message and submitting edit PUTs to /message/:id", async () => {
    const state = resetMswState();
    state.me = DEV_USER;
    state.messages["100"] = [
      {
        id: 7,
        user_id: DEV_USER.id,
        channel_id: 100,
        text: "original",
        username: "baipas",
        avatar_url: null,
      },
    ];
    mountAt("/channel/100");

    const original = await screen.findByText("original");
    fireEvent.contextMenu(original);

    const editItem = await screen.findByRole("menuitem", { name: /edit message/i });
    fireEvent.click(editItem);

    const input = (await screen.findByLabelText(/edit message/i)) as HTMLInputElement;
    expect(input.value).toBe("original");
    fireEvent.input(input, { target: { value: "edited!" } });
    const form = assertExists(input.closest("form"), "form");
    fireEvent.submit(form);

    await waitFor(() => {
      expect(mswState().editedMessages).toContainEqual({ id: 7, text: "edited!" });
    });
  });

  test("does not open edit menu for other users' messages", async () => {
    const state = resetMswState();
    state.me = DEV_USER;
    state.messages["100"] = [
      {
        id: 8,
        user_id: 999,
        channel_id: 100,
        text: "someone else",
        username: "bob",
        avatar_url: null,
      },
    ];
    mountAt("/channel/100");

    const other = await screen.findByText("someone else");
    fireEvent.contextMenu(other);

    expect(screen.queryByRole("menuitem", { name: /edit message/i })).toBeNull();
  });

  test("right-clicking own message and confirming delete sends DELETE /message/:id", async () => {
    const state = resetMswState();
    state.me = DEV_USER;
    state.messages["100"] = [
      {
        id: 7,
        user_id: DEV_USER.id,
        channel_id: 100,
        text: "delete me",
        username: "baipas",
        avatar_url: null,
      },
    ];
    mountAt("/channel/100");

    const original = await screen.findByText("delete me");
    fireEvent.contextMenu(original);

    const deleteItem = await screen.findByRole("menuitem", { name: /delete message/i });
    fireEvent.click(deleteItem);

    const confirmBtn = await screen.findByRole("button", { name: /^delete$/i });
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(mswState().deletedMessageIds).toContain(7);
    });
  });

  test("canceling the delete confirmation does not call the server", async () => {
    const state = resetMswState();
    state.me = DEV_USER;
    state.messages["100"] = [
      {
        id: 9,
        user_id: DEV_USER.id,
        channel_id: 100,
        text: "keep me",
        username: "baipas",
        avatar_url: null,
      },
    ];
    mountAt("/channel/100");

    const original = await screen.findByText("keep me");
    fireEvent.contextMenu(original);

    const deleteItem = await screen.findByRole("menuitem", { name: /delete message/i });
    fireEvent.click(deleteItem);

    // The confirmation modal should be open.
    await screen.findByRole("dialog", { name: /delete message/i });

    const cancelBtn = screen.getAllByRole("button", { name: /cancel/i })[0];
    fireEvent.click(cancelBtn);

    await new Promise((r) => setTimeout(r, 10));
    expect(mswState().deletedMessageIds).not.toContain(9);
  });

  test("submitting an empty edit prompts delete confirmation and deletes on confirm", async () => {
    const state = resetMswState();
    state.me = DEV_USER;
    state.messages["100"] = [
      {
        id: 11,
        user_id: DEV_USER.id,
        channel_id: 100,
        text: "blank me",
        username: "baipas",
        avatar_url: null,
      },
    ];
    mountAt("/channel/100");

    const original = await screen.findByText("blank me");
    fireEvent.contextMenu(original);

    const editItem = await screen.findByRole("menuitem", { name: /edit message/i });
    fireEvent.click(editItem);

    const input = (await screen.findByLabelText(/edit message/i)) as HTMLInputElement;
    fireEvent.input(input, { target: { value: "" } });
    const form = assertExists(input.closest("form"), "form");
    fireEvent.submit(form);

    await screen.findByRole("dialog", { name: /delete message/i });
    // No edit should have been sent for the blank text.
    expect(mswState().editedMessages).not.toContainEqual({ id: 11, text: "" });

    const confirmBtn = await screen.findByRole("button", { name: /^delete$/i });
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(mswState().deletedMessageIds).toContain(11);
    });
  });

  test("removes message from UI when a message_deleted SSE event arrives", async () => {
    seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessageDeleted({ id: 1, channel_id: 100 });

    await waitFor(() => {
      expect(screen.queryByText("hello")).toBeNull();
      expect(screen.getByText("world")).toBeInTheDocument();
    });
  });

  test("updates displayed text when a message_updated SSE event arrives", async () => {
    seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => expect(screen.getByText("hello")).toBeInTheDocument());
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushMessageUpdated({
      id: 1,
      user_id: 1,
      channel_id: 100,
      text: "hello (edited)",
      username: "alice",
      avatar_url: null,
    });

    await waitFor(() => {
      expect(screen.getByText("hello (edited)")).toBeInTheDocument();
      expect(screen.queryByText("hello")).toBeNull();
    });
  });

  test("shows a typing indicator when another user's user_typing event arrives", async () => {
    seedAuthed();
    mountAt("/channel/100");

    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());

    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushUserTyping({ channel_id: 100, user_id: 2, username: "carol" });

    await waitFor(() => {
      expect(screen.getByText(/carol is typing/i)).toBeInTheDocument();
    });
  });

  test("does not show a typing indicator for the current user's own pings", async () => {
    const state = resetMswState();
    state.me = DEV_USER;
    // Own messages have a right-click "edit" menu — we use that as our signal
    // that the auth resource has resolved so currentUserId is known.
    state.messages["100"] = [
      {
        id: 42,
        user_id: DEV_USER.id,
        channel_id: 100,
        text: "mine",
        username: DEV_USER.username,
        avatar_url: null,
      },
    ];
    mountAt("/channel/100");

    const ownMessage = await screen.findByText("mine");
    await waitFor(() => {
      fireEvent.contextMenu(ownMessage);
      expect(screen.queryByRole("menuitem", { name: /edit message/i })).not.toBeNull();
    });
    // Dismiss the menu.
    fireEvent.keyDown(document, { key: "Escape" });

    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());
    const es = assertExists(latestFakeEventSource(), "latestFakeEventSource");
    es.pushUserTyping({ channel_id: 100, user_id: DEV_USER.id, username: DEV_USER.username });

    await new Promise((r) => setTimeout(r, 20));
    expect(screen.queryByTestId("typing-indicator")).toBeNull();
  });

  test("POSTs a typing ping to the current channel when the input changes", async () => {
    seedAuthed();
    mountAt("/channel/100");

    const input = (await screen.findByPlaceholderText(/send a new message/i)) as HTMLInputElement;
    fireEvent.input(input, { target: { value: "h" } });

    await waitFor(() => {
      expect(mswState().typingPings).toContain("100");
    });
  });

  test("renders avatars next to each message", async () => {
    const state = resetMswState();
    state.me = DEV_USER;
    state.messages["100"] = [
      {
        id: 1,
        user_id: 1,
        channel_id: 100,
        text: "hello",
        username: "alice",
        avatar_url: "/uploads/avatars/1.webp?v=1",
      },
      {
        id: 2,
        user_id: 2,
        channel_id: 100,
        text: "world",
        username: "bob",
        avatar_url: null,
      },
    ];
    mountAt("/channel/100");

    await waitFor(() => {
      const aliceAvatar = screen.getByRole("img", { name: /alice's avatar/i });
      expect(aliceAvatar.querySelector("img")).not.toBeNull();
      const bobAvatar = screen.getByRole("img", { name: /bob's avatar/i });
      expect(bobAvatar.querySelector("svg")).not.toBeNull();
    });
  });
});
