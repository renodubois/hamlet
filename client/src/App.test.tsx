import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { act, screen } from "@testing-library/react";
import { http, HttpResponse } from "msw";

import { assertExists, renderNative } from "./test/render";
import { captureReactDiagnostics, type ReactDiagnosticsCapture } from "./test/setup";
import { AuthProvider } from "./contexts/auth";
import { FakeEventSource } from "./test/msw/sse";
import { resetMswState, server } from "./test/msw/server";
import { DEV_USER } from "./test/msw/handlers";
import { makeMessage } from "./test/fixtures";
import App from "./App";
import ChannelView from "./pages/channel";

let diagnostics: ReactDiagnosticsCapture;

beforeEach(() => {
  diagnostics = captureReactDiagnostics();
});

afterEach(() => {
  diagnostics.stop();
  expect(diagnostics.diagnostics).toEqual([]);
});

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
  return renderNative(
    <AuthProvider>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route element={<App />}>
            <Route index element={null} />
            <Route path="channel/:id" element={<ChannelView />} />
          </Route>
        </Routes>
      </MemoryRouter>
    </AuthProvider>,
  );
}

function CurrentLocation() {
  const location = useLocation();
  return <p>Current route: {location.pathname}</p>;
}

describe("App auth boundary", () => {
  test("shows no login or authenticated providers until auth resolves", async () => {
    let resolveMe!: () => void;
    const pendingMe = new Promise<void>((resolve) => {
      resolveMe = resolve;
    });
    const testServer = import.meta.env.VITE_HAMLET_DEFAULT_SERVER_URL ?? "http://127.0.0.1:3030";
    server.use(
      http.get(`${testServer}/me`, async () => {
        await pendingMe;
        return new HttpResponse(null, { status: 401 });
      }),
    );

    renderNative(
      <AuthProvider>
        <MemoryRouter initialEntries={["/channel/100"]}>
          <Routes>
            <Route element={<App />}>
              <Route path="login" element={<p>Login route</p>} />
              <Route path="channel/:id" element={<p>Protected channel</p>} />
            </Route>
          </Routes>
        </MemoryRouter>
      </AuthProvider>,
    );

    expect(screen.queryByText("Login route")).toBeNull();
    expect(screen.queryByText("Protected channel")).toBeNull();
    expect(FakeEventSource.instances).toHaveLength(0);

    await act(async () => {
      resolveMe();
      await pendingMe;
    });
    expect(await screen.findByText("Login route")).toBeInTheDocument();
    expect(FakeEventSource.instances).toHaveLength(0);
  });
});

describe("App shell layout", () => {
  test("the authenticated root selects the first text channel", async () => {
    seedAuthedChannel();
    renderNative(
      <AuthProvider>
        <MemoryRouter initialEntries={["/"]}>
          <Routes>
            <Route element={<App />}>
              <Route index element={<CurrentLocation />} />
              <Route path="channel/:id" element={<CurrentLocation />} />
            </Route>
          </Routes>
        </MemoryRouter>
      </AuthProvider>,
    );

    expect(await screen.findByText("Current route: /channel/100")).toBeInTheDocument();
  });

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
    expect(messagesRegion).toHaveClass("min-h-0", "flex-1", "overflow-y-auto", "overscroll-y-none");
    expect(composer).toHaveClass("flex-shrink-0");
  });
});
