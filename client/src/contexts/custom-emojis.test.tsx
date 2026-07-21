import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { renderNative } from "../test/render";
import { afterEach, describe, expect, test, vi } from "vitest";
import { http, HttpResponse } from "msw";
import { AuthProvider, useAuth } from "./auth";
import { CustomEmojisProvider, useCustomEmojis } from "./custom-emojis";
import { EventsProvider } from "./events";
import type { CustomEmoji } from "../api";
import { DEV_USER } from "../test/msw/handlers";
import { resetMswState, server } from "../test/msw/server";
import { FakeEventSource, latestFakeEventSource } from "../test/msw/sse";

const TEST_SERVER = import.meta.env.VITE_HAMLET_DEFAULT_SERVER_URL ?? "http://127.0.0.1:3030";

function deferred<T = void>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

function emoji(id: number, name: string, updatedAt = 1) {
  return {
    id,
    name,
    image_url: `/uploads/emojis/${id}.webp?v=${updatedAt}`,
    animated: false,
    created_by_user_id: 1,
    created_at: 1,
    updated_at: updatedAt,
    deleted_at: null,
  };
}

function deletedEmoji(id: number, name: string, updatedAt: number) {
  return { ...emoji(id, name, updatedAt), deleted_at: updatedAt };
}

function Probe() {
  const registry = useCustomEmojis();
  const deleted = registry.byId(2);
  return (
    <div>
      <p>all {registry.allEmojis.length}</p>
      <p>active {registry.activeEmojis.length}</p>
      <p>status {registry.status}</p>
      {registry.error ? <p>error {registry.error.message}</p> : null}
      {deleted ? <p>lookup {deleted.name}</p> : null}
      {registry.activeEmojis.map((entry) => (
        <span key={entry.id}>{entry.name}</span>
      ))}
    </div>
  );
}

function mountProbe() {
  return renderNative(
    <AuthProvider>
      <CustomEmojisProvider>
        <Probe />
      </CustomEmojisProvider>
    </AuthProvider>,
  );
}

function mountProbeWithEvents() {
  return renderNative(
    <AuthProvider>
      <EventsProvider>
        <CustomEmojisProvider>
          <Probe />
        </CustomEmojisProvider>
      </EventsProvider>
    </AuthProvider>,
  );
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

  test("replays SSE updates over a delayed snapshot", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    resetMswState({ me: DEV_USER });
    const snapshot = deferred<void>();
    let snapshotRequested = false;
    server.use(
      http.get(`${TEST_SERVER}/emojis`, async () => {
        snapshotRequested = true;
        await snapshot.promise;
        return HttpResponse.json([emoji(1, "stale_party")]);
      }),
    );

    mountProbeWithEvents();
    await waitFor(() => {
      expect(snapshotRequested).toBe(true);
      expect(latestFakeEventSource()).toBeDefined();
    });
    latestFakeEventSource()?.push({ kind: "emoji_updated", data: emoji(1, "live_party", 2) });
    expect(await screen.findByText("live_party")).toBeInTheDocument();

    await act(async () => {
      snapshot.resolve();
      await snapshot.promise;
    });

    await waitFor(() => expect(screen.getByText("live_party")).toBeInTheDocument());
    expect(screen.queryByText("stale_party")).toBeNull();
    expect(screen.getByText("all 1")).toBeInTheDocument();
  });

  test("rejects older same-ID SSE rename, delete, and restore versions", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    resetMswState({ me: DEV_USER, customEmojis: [emoji(1, "party", 10)] });

    mountProbeWithEvents();
    expect(await screen.findByText("party")).toBeInTheDocument();
    const source = assertEventSource();

    source.push({ kind: "emoji_updated", data: emoji(1, "new_name", 30) });
    source.push({ kind: "emoji_updated", data: emoji(1, "old_name", 20) });
    expect(await screen.findByText("new_name")).toBeInTheDocument();
    expect(screen.queryByText("old_name")).toBeNull();

    source.push({ kind: "emoji_deleted", data: deletedEmoji(1, "new_name", 50) });
    source.push({ kind: "emoji_updated", data: emoji(1, "new_name", 40) });
    await waitFor(() => expect(screen.getByText("active 0")).toBeInTheDocument());

    source.push({ kind: "emoji_updated", data: emoji(1, "new_name", 70) });
    source.push({ kind: "emoji_deleted", data: deletedEmoji(1, "new_name", 60) });
    await waitFor(() => expect(screen.getByText("active 1")).toBeInTheDocument());
    expect(screen.getByText("new_name")).toBeInTheDocument();
  });

  test("latest-started same-ID HTTP mutation wins when responses resolve out of order", async () => {
    resetMswState({ me: DEV_USER, customEmojis: [emoji(1, "party", 10)] });
    const firstRename = deferred<void>();
    const secondRename = deferred<void>();
    const removeResponse = deferred<void>();
    const restoreResponse = deferred<void>();
    let renameRequests = 0;
    server.use(
      http.patch(`${TEST_SERVER}/emojis/1`, async () => {
        renameRequests += 1;
        const request = renameRequests;
        await (request === 1 ? firstRename.promise : secondRename.promise);
        return HttpResponse.json(emoji(1, request === 1 ? "older_intent" : "latest_intent", 20));
      }),
      http.delete(`${TEST_SERVER}/emojis/1`, async () => {
        await removeResponse.promise;
        return HttpResponse.json(deletedEmoji(1, "latest_intent", 30));
      }),
      http.post(`${TEST_SERVER}/emojis/1/restore`, async () => {
        await restoreResponse.promise;
        return HttpResponse.json(emoji(1, "latest_intent", 30));
      }),
    );

    function MutationProbe() {
      const registry = useCustomEmojis();
      return (
        <>
          <Probe />
          <button type="button" onClick={() => void registry.rename(1, "older_intent")}>
            first rename
          </button>
          <button type="button" onClick={() => void registry.rename(1, "latest_intent")}>
            second rename
          </button>
          <button type="button" onClick={() => void registry.remove(1)}>
            remove
          </button>
          <button type="button" onClick={() => void registry.restore(1)}>
            restore
          </button>
        </>
      );
    }
    renderNative(
      <AuthProvider>
        <CustomEmojisProvider>
          <MutationProbe />
        </CustomEmojisProvider>
      </AuthProvider>,
    );
    expect(await screen.findByText("party")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "first rename" }));
    fireEvent.click(screen.getByRole("button", { name: "second rename" }));
    await waitFor(() => expect(renameRequests).toBe(2));
    secondRename.resolve();
    expect(await screen.findByText("latest_intent")).toBeInTheDocument();
    firstRename.resolve();
    await act(async () => await firstRename.promise);
    expect(screen.queryByText("older_intent")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "remove" }));
    fireEvent.click(screen.getByRole("button", { name: "restore" }));
    restoreResponse.resolve();
    await act(async () => await restoreResponse.promise);
    removeResponse.resolve();
    await act(async () => await removeResponse.promise);
    expect(screen.getByText("active 1")).toBeInTheDocument();
  });

  test("equal-version duplicate SSE delivery preserves emoji and array identity", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    const original = emoji(1, "party", 10);
    resetMswState({ me: DEV_USER, customEmojis: [original] });
    const arrays: (readonly CustomEmoji[])[] = [];

    function IdentityProbe() {
      const registry = useCustomEmojis();
      arrays.push(registry.allEmojis);
      return <Probe />;
    }
    renderNative(
      <AuthProvider>
        <EventsProvider>
          <CustomEmojisProvider>
            <IdentityProbe />
          </CustomEmojisProvider>
        </EventsProvider>
      </AuthProvider>,
    );
    expect(await screen.findByText("party")).toBeInTheDocument();
    const before = arrays.at(-1);

    act(() => {
      assertEventSource().push({ kind: "emoji_updated", data: { ...original } });
    });

    expect(arrays.at(-1)).toBe(before);
    expect(arrays.at(-1)?.[0]).toBe(before?.[0]);
  });

  test("uses SSE delivery order for distinct DTOs tied at the wire timestamp", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    resetMswState({ me: DEV_USER, customEmojis: [emoji(1, "party", 10)] });

    mountProbeWithEvents();
    expect(await screen.findByText("party")).toBeInTheDocument();
    const source = assertEventSource();
    source.push({ kind: "emoji_deleted", data: deletedEmoji(1, "party", 20) });
    source.push({ kind: "emoji_updated", data: emoji(1, "party", 20) });

    await waitFor(() => expect(screen.getByText("active 1")).toBeInTheDocument());
    // updated_at/deleted_at are second-resolution and provide no total order for
    // distinct same-second DTOs. SSE's ordered delivery is the strongest safe
    // tie-breaker available without a server revision/cursor field.
    expect(screen.getByText("party")).toBeInTheDocument();
  });

  test("deduplicates the same HTTP mutation and SSE delivery by id", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    resetMswState({ me: DEV_USER, customEmojis: [] });
    const created = emoji(9, "party");
    server.use(http.post(`${TEST_SERVER}/emojis`, () => HttpResponse.json(created)));

    function CreateProbe() {
      const registry = useCustomEmojis();
      return (
        <>
          <Probe />
          <button
            type="button"
            onClick={() => void registry.create("party", new Blob(["x"], { type: "image/png" }))}
          >
            create
          </button>
        </>
      );
    }
    renderNative(
      <AuthProvider>
        <EventsProvider>
          <CustomEmojisProvider>
            <CreateProbe />
          </CustomEmojisProvider>
        </EventsProvider>
      </AuthProvider>,
    );
    await waitFor(() => expect(screen.getByText("status ready")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: "create" }));
    latestFakeEventSource()?.push({ kind: "emoji_created", data: { ...created } });

    await waitFor(() => expect(screen.getByText("party")).toBeInTheDocument());
    expect(screen.getByText("all 1")).toBeInTheDocument();
  });

  test("clears the prior registry when the authenticated user key changes", async () => {
    const state = resetMswState({ me: DEV_USER, customEmojis: [emoji(1, "first_user")] });
    const secondSnapshot = deferred<void>();
    let emojiRequests = 0;
    server.use(
      http.get(`${TEST_SERVER}/emojis`, async () => {
        emojiRequests += 1;
        if (emojiRequests > 1) await secondSnapshot.promise;
        return HttpResponse.json(
          emojiRequests > 1 ? [emoji(2, "second_user")] : [emoji(1, "first_user")],
        );
      }),
    );

    function AuthChangeProbe() {
      const auth = useAuth();
      return (
        <>
          <button type="button" onClick={() => void auth.refresh()}>
            change auth
          </button>
          <CustomEmojisProvider>
            <Probe />
          </CustomEmojisProvider>
        </>
      );
    }
    renderNative(
      <AuthProvider>
        <AuthChangeProbe />
      </AuthProvider>,
    );
    expect(await screen.findByText("first_user")).toBeInTheDocument();

    state.me = { ...DEV_USER, id: DEV_USER.id + 1, username: "other" };
    fireEvent.click(screen.getByRole("button", { name: "change auth" }));
    await waitFor(() => {
      expect(screen.getByText("all 0")).toBeInTheDocument();
      expect(screen.getByText("status loading")).toBeInTheDocument();
      expect(screen.queryByText("first_user")).toBeNull();
    });

    await act(async () => {
      secondSnapshot.resolve();
      await secondSnapshot.promise;
    });
    expect(await screen.findByText("second_user")).toBeInTheDocument();
  });

  test("reconnect invalidates a still-pending pre-connection snapshot", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    resetMswState({ me: DEV_USER });
    const firstSnapshot = deferred<void>();
    const secondSnapshot = deferred<void>();
    let requests = 0;
    server.use(
      http.get(`${TEST_SERVER}/emojis`, async () => {
        requests += 1;
        const request = requests;
        await (request === 1 ? firstSnapshot.promise : secondSnapshot.promise);
        return HttpResponse.json([emoji(request, request === 1 ? "stale" : "fresh")]);
      }),
    );

    mountProbeWithEvents();
    await waitFor(() => expect(requests).toBe(1));
    assertEventSource().open();
    await waitFor(() => expect(requests).toBe(2));

    await act(async () => {
      firstSnapshot.resolve();
      await firstSnapshot.promise;
    });
    expect(screen.queryByText("stale")).toBeNull();

    await act(async () => {
      secondSnapshot.resolve();
      await secondSnapshot.promise;
    });
    expect(await screen.findByText("fresh")).toBeInTheDocument();
    expect(screen.queryByText("stale")).toBeNull();
  });

  test("keeps lookup and action identities stable under Strict Mode updates", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    resetMswState({ me: DEV_USER, customEmojis: [emoji(1, "party")] });
    const values: ReturnType<typeof useCustomEmojis>[] = [];

    function IdentityProbe() {
      const registry = useCustomEmojis();
      values.push(registry);
      return <Probe />;
    }
    renderNative(
      <AuthProvider>
        <EventsProvider>
          <CustomEmojisProvider>
            <IdentityProbe />
          </CustomEmojisProvider>
        </EventsProvider>
      </AuthProvider>,
    );
    await waitFor(() => expect(screen.getByText("party")).toBeInTheDocument());
    const before = values.at(-1);
    latestFakeEventSource()?.push({ kind: "emoji_updated", data: emoji(1, "renamed", 2) });
    await waitFor(() => expect(screen.getByText("renamed")).toBeInTheDocument());
    const after = values.at(-1);

    expect(after?.byId).toBe(before?.byId);
    expect(after?.refresh).toBe(before?.refresh);
    expect(after?.create).toBe(before?.create);
    expect(after?.rename).toBe(before?.rename);
    expect(after?.remove).toBe(before?.remove);
    expect(after?.restore).toBe(before?.restore);
  });
});

function assertEventSource(): FakeEventSource {
  const eventSource = latestFakeEventSource();
  if (!eventSource) throw new Error("Expected an EventSource");
  return eventSource;
}
