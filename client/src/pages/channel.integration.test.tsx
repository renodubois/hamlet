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
