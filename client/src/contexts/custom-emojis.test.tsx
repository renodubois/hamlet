import { render, screen, waitFor } from "@solidjs/testing-library";
import { For, Show } from "solid-js";
import { afterEach, describe, expect, test, vi } from "vitest";
import { AuthProvider } from "./auth";
import { CustomEmojisProvider, useCustomEmojis } from "./custom-emojis";
import { EventsProvider } from "./events";
import { DEV_USER } from "../test/msw/handlers";
import { resetMswState } from "../test/msw/server";
import { FakeEventSource, latestFakeEventSource } from "../test/msw/sse";

function Probe() {
  const registry = useCustomEmojis();
  const deleted = () => registry.byId(2);
  return (
    <div>
      <p>all {registry.allEmojis()?.length ?? 0}</p>
      <p>active {registry.activeEmojis().length}</p>
      <Show when={deleted()}>{(emoji) => <p>lookup {emoji().name}</p>}</Show>
      <For each={registry.activeEmojis()}>{(emoji) => <span>{emoji.name}</span>}</For>
    </div>
  );
}

function mountProbe() {
  return render(() => (
    <AuthProvider>
      <CustomEmojisProvider>
        <Probe />
      </CustomEmojisProvider>
    </AuthProvider>
  ));
}

function mountProbeWithEvents() {
  return render(() => (
    <AuthProvider>
      <EventsProvider>
        <CustomEmojisProvider>
          <Probe />
        </CustomEmojisProvider>
      </EventsProvider>
    </AuthProvider>
  ));
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("CustomEmojisProvider", () => {
  test("loads all emojis and exposes active emojis plus lookup by id", async () => {
    resetMswState({
      me: DEV_USER,
      customEmojis: [
        {
          id: 1,
          name: "party",
          image_url: "/uploads/emojis/1.webp?v=10",
          animated: false,
          created_by_user_id: 1,
          created_at: 10,
          updated_at: 10,
          deleted_at: null,
        },
        {
          id: 2,
          name: "retired",
          image_url: "/uploads/emojis/2.webp?v=20",
          animated: true,
          created_by_user_id: 1,
          created_at: 20,
          updated_at: 30,
          deleted_at: 40,
        },
      ],
    });

    mountProbe();

    await waitFor(() => expect(screen.getByText("all 2")).toBeInTheDocument());
    expect(screen.getByText("active 1")).toBeInTheDocument();
    expect(screen.getByText("lookup retired")).toBeInTheDocument();
    expect(screen.getByText("party")).toBeInTheDocument();
    expect(screen.queryByText("retired")).not.toBeInTheDocument();
  });

  test("loads the empty registry path", async () => {
    resetMswState({ me: DEV_USER, customEmojis: [] });

    mountProbe();

    await waitFor(() => expect(screen.getByText("all 0")).toBeInTheDocument());
    expect(screen.getByText("active 0")).toBeInTheDocument();
  });

  test("upserts renamed emojis from realtime events", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    resetMswState({
      me: DEV_USER,
      customEmojis: [
        {
          id: 1,
          name: "party",
          image_url: "/uploads/emojis/1.webp?v=10",
          animated: false,
          created_by_user_id: 1,
          created_at: 10,
          updated_at: 10,
          deleted_at: null,
        },
      ],
    });

    mountProbeWithEvents();

    await waitFor(() => expect(screen.getByText("party")).toBeInTheDocument());
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());
    latestFakeEventSource()?.push({
      kind: "emoji_updated",
      data: {
        id: 1,
        name: "renamed_party",
        image_url: "/uploads/emojis/1.webp?v=11",
        animated: false,
        created_by_user_id: 1,
        created_at: 10,
        updated_at: 11,
        deleted_at: null,
      },
    });

    await waitFor(() => expect(screen.getByText("renamed_party")).toBeInTheDocument());
    expect(screen.queryByText("party")).not.toBeInTheDocument();
  });

  test("realtime delete hides emojis from active list and restore shows them again", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    resetMswState({
      me: DEV_USER,
      customEmojis: [
        {
          id: 1,
          name: "party",
          image_url: "/uploads/emojis/1.webp?v=10",
          animated: false,
          created_by_user_id: 1,
          created_at: 10,
          updated_at: 10,
          deleted_at: null,
        },
      ],
    });

    mountProbeWithEvents();

    await waitFor(() => expect(screen.getByText("party")).toBeInTheDocument());
    await waitFor(() => expect(latestFakeEventSource()).toBeDefined());
    latestFakeEventSource()?.push({
      kind: "emoji_deleted",
      data: {
        id: 1,
        name: "party",
        image_url: "/uploads/emojis/1.webp?v=11",
        animated: false,
        created_by_user_id: 1,
        created_at: 10,
        updated_at: 11,
        deleted_at: 11,
      },
    });

    await waitFor(() => expect(screen.getByText("active 0")).toBeInTheDocument());
    expect(screen.queryByText("party")).not.toBeInTheDocument();

    latestFakeEventSource()?.push({
      kind: "emoji_updated",
      data: {
        id: 1,
        name: "party",
        image_url: "/uploads/emojis/1.webp?v=12",
        animated: false,
        created_by_user_id: 1,
        created_at: 10,
        updated_at: 12,
        deleted_at: null,
      },
    });

    await waitFor(() => expect(screen.getByText("active 1")).toBeInTheDocument());
    expect(screen.getByText("party")).toBeInTheDocument();
  });
});
