import { describe, expect, test, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";

import { render, screen } from "./test/testing-library";
import { AuthProvider } from "./contexts/auth";
import { FakeEventSource } from "./test/msw/sse";
import { resetMswState } from "./test/msw/server";
import { DEV_USER } from "./test/msw/handlers";
import { makeMessage } from "./test/fixtures";
import { assertExists } from "./test/render";
import App from "./App";
import ChannelView from "./pages/channel";

vi.mock("./api", async () => {
  const actual = await vi.importActual<typeof import("./api")>("./api");
  return {
    ...actual,
    messagesEventSource: () => new FakeEventSource("/mock/messages") as unknown as EventSource,
  };
});

function seedAuthedChannel() {
  const state = resetMswState();
  state.me = { ...DEV_USER, username: "alice", display_name: null, avatar_url: null };
  state.messages["100"] = Array.from({ length: 30 }, (_, index) =>
    makeMessage({
      id: index + 1,
      user_id: index % 2 === 0 ? DEV_USER.id : 2,
      channel_id: 100,
      text: `layout message ${index + 1}`,
      username: index % 2 === 0 ? "alice" : "bob",
    }),
  );
}

function renderAppAt(path: string) {
  return render(() => (
    <AuthProvider>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route element={<App />}>
            <Route index element={null} />
            <Route path="channel/:id" element={<ChannelView />} />
          </Route>
        </Routes>
      </MemoryRouter>
    </AuthProvider>
  ));
}

describe("App shell layout", () => {
  test("constrains channel routes so messages scroll above the pinned composer", async () => {
    seedAuthedChannel();
    renderAppAt("/channel/100");

    const input = await screen.findByRole("textbox", { name: /new message/i });
    const main = screen.getByRole("main");
    const routeFrame = assertExists(main.firstElementChild, "route frame") as HTMLElement;
    const channelRoot = assertExists(routeFrame.firstElementChild, "channel root") as HTMLElement;
    const messagesRegion = screen.getByRole("region", { name: /messages/i });
    const messagePane = assertExists(messagesRegion.parentElement, "message pane") as HTMLElement;
    const composer = assertExists(input.closest("section"), "message composer") as HTMLElement;

    expect(main).toHaveClass("min-h-0", "overflow-hidden");
    expect(routeFrame).toHaveClass("flex", "h-full", "min-h-0", "flex-1", "flex-col");
    expect(channelRoot).toHaveClass("h-full", "min-h-0", "flex-1", "overflow-hidden");
    expect(messagePane).toHaveClass("min-h-0", "flex-1", "overflow-hidden");
    expect(messagesRegion).toHaveClass("min-h-0", "flex-1", "overflow-y-auto");
    expect(composer).toHaveClass("flex-shrink-0");
  });
});
