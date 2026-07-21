import { act, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";
import type { User } from "../api";
import { renderNative } from "../test/render";

const getMeMock = vi.hoisted(() => vi.fn<(signal?: AbortSignal) => Promise<User | null>>());
const loginMock = vi.hoisted(() =>
  vi.fn<(username: string, password: string) => Promise<Response>>(),
);
const registerMock = vi.hoisted(() =>
  vi.fn<(username: string, password: string, email?: string) => Promise<Response>>(),
);
const logoutMock = vi.hoisted(() => vi.fn<() => Promise<void>>());

vi.mock("../api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../api")>();
  return {
    ...actual,
    getMe: getMeMock,
    login: loginMock,
    register: registerMock,
    logout: logoutMock,
  };
});

import { AuthProvider, useAuth, type AuthContextValue } from "./auth";

const USER: User = {
  id: 1,
  username: "alice",
  display_name: null,
  email: null,
  email_verified: false,
  avatar_url: null,
};

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

let currentAuth: AuthContextValue;
const actionSnapshots: Array<Pick<AuthContextValue, "login" | "register" | "logout" | "refresh">> =
  [];
const valueSnapshots: AuthContextValue[] = [];

function Probe() {
  const auth = useAuth();
  currentAuth = auth;
  actionSnapshots.push({
    login: auth.login,
    register: auth.register,
    logout: auth.logout,
    refresh: auth.refresh,
  });
  valueSnapshots.push(auth);
  return (
    <div>
      <p data-testid="status">{auth.status}</p>
      <p data-testid="user">{auth.user?.username ?? "none"}</p>
      <p data-testid="error">
        {auth.error instanceof Error ? auth.error.message : auth.error === null ? "none" : "error"}
      </p>
    </div>
  );
}

function mount() {
  return renderNative(
    <AuthProvider>
      <Probe />
    </AuthProvider>,
  );
}

function okResponse() {
  return new Response(null, { status: 200 });
}

function expectStableActions() {
  const first = actionSnapshots[0];
  expect(first).toBeDefined();
  for (const snapshot of actionSnapshots) {
    expect(snapshot.login).toBe(first?.login);
    expect(snapshot.register).toBe(first?.register);
    expect(snapshot.logout).toBe(first?.logout);
    expect(snapshot.refresh).toBe(first?.refresh);
  }
}

afterEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  actionSnapshots.length = 0;
  valueSnapshots.length = 0;
});

describe("AuthProvider", () => {
  test("keeps initial auth unresolved until the latest Strict Mode lookup authenticates", async () => {
    const requests: Array<ReturnType<typeof deferred<User | null>>> = [];
    getMeMock.mockImplementation(() => {
      const request = deferred<User | null>();
      requests.push(request);
      return request.promise;
    });

    mount();

    expect(screen.getByTestId("status")).toHaveTextContent("loading");
    expect(screen.getByTestId("user")).toHaveTextContent("none");
    expect(requests.length).toBeGreaterThanOrEqual(2);

    await act(async () => requests.at(-1)?.resolve(USER));
    await waitFor(() => expect(screen.getByTestId("status")).toHaveTextContent("authenticated"));
    expect(screen.getByTestId("user")).toHaveTextContent("alice");

    await act(async () => {
      for (const request of requests.slice(0, -1)) request.resolve(null);
    });
    expect(screen.getByTestId("status")).toHaveTextContent("authenticated");
    expectStableActions();
  });

  test("publishes anonymous and authenticated initial sessions without an intermediate wrong state", async () => {
    getMeMock.mockResolvedValueOnce(null).mockResolvedValue(null);
    const anonymousView = mount();
    await waitFor(() => expect(screen.getByTestId("status")).toHaveTextContent("anonymous"));
    expect(screen.getByTestId("user")).toHaveTextContent("none");
    anonymousView.unmount();

    getMeMock.mockResolvedValue(USER);
    mount();
    await waitFor(() => expect(screen.getByTestId("status")).toHaveTextContent("authenticated"));
    expect(screen.getByTestId("user")).toHaveTextContent("alice");
  });

  test("only the latest login attempt and server can refresh visible auth", async () => {
    getMeMock.mockResolvedValue(null);
    const oldLogin = deferred<Response>();
    const newLogin = deferred<Response>();
    loginMock.mockImplementation((username) =>
      username === "old" ? oldLogin.promise : newLogin.promise,
    );
    mount();
    await waitFor(() => expect(screen.getByTestId("status")).toHaveTextContent("anonymous"));

    let oldResult!: Promise<string | null>;
    let newResult!: Promise<string | null>;
    act(() => {
      oldResult = currentAuth.login("http://old.example.test", "old", "password");
      newResult = currentAuth.login("http://new.example.test", "new", "password");
    });
    getMeMock.mockResolvedValue({ ...USER, id: 2, username: "new" });

    await act(async () => newLogin.resolve(okResponse()));
    await expect(newResult).resolves.toBeNull();
    await waitFor(() => expect(screen.getByTestId("user")).toHaveTextContent("new"));
    expect(localStorage.getItem("hamlet.serverUrl")).toBe("http://new.example.test");

    getMeMock.mockResolvedValue({ ...USER, id: 3, username: "old" });
    await act(async () => oldLogin.resolve(okResponse()));
    await expect(oldResult).resolves.toBeNull();
    expect(screen.getByTestId("user")).toHaveTextContent("new");
    expect(localStorage.getItem("hamlet.serverUrl")).toBe("http://new.example.test");
  });

  test("register selects the server before the request and awaits the session refresh", async () => {
    getMeMock.mockResolvedValue(null);
    registerMock.mockResolvedValue(okResponse());
    mount();
    await waitFor(() => expect(screen.getByTestId("status")).toHaveTextContent("anonymous"));
    getMeMock.mockResolvedValue({ ...USER, username: "registered" });

    await act(async () => {
      await expect(
        currentAuth.register("http://register.example.test", "registered", "secret", "a@b.test"),
      ).resolves.toBeNull();
    });

    expect(registerMock).toHaveBeenCalledWith("registered", "secret", "a@b.test");
    expect(localStorage.getItem("hamlet.serverUrl")).toBe("http://register.example.test");
    expect(screen.getByTestId("user")).toHaveTextContent("registered");
  });

  test("a same-server refresh retains authenticated state, providers, and context identity", async () => {
    const refreshRequest = deferred<User | null>();
    getMeMock.mockResolvedValue(USER);
    mount();
    await waitFor(() => expect(screen.getByTestId("status")).toHaveTextContent("authenticated"));
    const readyValue = currentAuth;
    getMeMock.mockImplementation(() => refreshRequest.promise);

    let refreshPromise!: Promise<void>;
    act(() => {
      refreshPromise = currentAuth.refresh();
    });
    expect(screen.getByTestId("status")).toHaveTextContent("authenticated");
    expect(screen.getByTestId("user")).toHaveTextContent("alice");
    expect(currentAuth).toBe(readyValue);

    await act(async () => refreshRequest.reject(new Error("refresh failed")));
    await expect(refreshPromise).resolves.toBeUndefined();
    await waitFor(() => expect(screen.getByTestId("error")).toHaveTextContent("refresh failed"));
    expect(screen.getByTestId("status")).toHaveTextContent("authenticated");
    expectStableActions();
  });

  test("logout clears visible auth immediately and keeps it clear when the request fails", async () => {
    getMeMock.mockResolvedValue(USER);
    logoutMock.mockRejectedValue(new Error("logout failed"));
    mount();
    await waitFor(() => expect(screen.getByTestId("status")).toHaveTextContent("authenticated"));

    let logoutPromise!: Promise<void>;
    act(() => {
      logoutPromise = currentAuth.logout();
    });
    const logoutResult = expect(logoutPromise).rejects.toThrow("logout failed");
    await waitFor(() => expect(screen.getByTestId("status")).toHaveTextContent("anonymous"));
    expect(screen.getByTestId("user")).toHaveTextContent("none");
    await logoutResult;
    expect(screen.getByTestId("status")).toHaveTextContent("anonymous");
    expectStableActions();
  });

  test("logout aborts an in-flight refresh and stale completion cannot restore auth", async () => {
    const refreshRequest = deferred<User | null>();
    let refreshSignal: AbortSignal | undefined;
    getMeMock.mockResolvedValue(USER);
    logoutMock.mockResolvedValue();
    mount();
    await waitFor(() => expect(screen.getByTestId("status")).toHaveTextContent("authenticated"));
    getMeMock.mockImplementation((signal) => {
      refreshSignal = signal;
      return refreshRequest.promise;
    });

    let refreshPromise!: Promise<void>;
    let logoutPromise!: Promise<void>;
    act(() => {
      refreshPromise = currentAuth.refresh();
      logoutPromise = currentAuth.logout();
    });

    expect(refreshSignal?.aborted).toBe(true);
    expect(screen.getByTestId("status")).toHaveTextContent("anonymous");
    expect(screen.getByTestId("user")).toHaveTextContent("none");
    await act(async () => refreshRequest.resolve(USER));
    await expect(refreshPromise).resolves.toBeUndefined();
    await expect(logoutPromise).resolves.toBeUndefined();
    expect(screen.getByTestId("status")).toHaveTextContent("anonymous");
    expect(screen.getByTestId("user")).toHaveTextContent("none");
  });

  test("stale rejected login and registration attempts return no old-server error", async () => {
    getMeMock.mockResolvedValue(null);
    const oldLogin = deferred<Response>();
    const oldRegister = deferred<Response>();
    const latestLogin = deferred<Response>();
    loginMock
      .mockImplementationOnce(() => oldLogin.promise)
      .mockImplementationOnce(() => latestLogin.promise);
    registerMock.mockImplementation(() => oldRegister.promise);
    mount();
    await waitFor(() => expect(screen.getByTestId("status")).toHaveTextContent("anonymous"));

    let loginResult!: Promise<string | null>;
    let registerResult!: Promise<string | null>;
    act(() => {
      loginResult = currentAuth.login("http://old.example.test", "old", "password");
      registerResult = currentAuth.register("http://new.example.test", "new", "password");
    });
    await act(async () => oldLogin.reject(new TypeError("old server failed")));
    await expect(loginResult).resolves.toBeNull();

    let newestResult!: Promise<string | null>;
    act(() => {
      newestResult = currentAuth.login("http://latest.example.test", "latest", "password");
    });
    await act(async () => oldRegister.reject(new TypeError("new server failed late")));
    await expect(registerResult).resolves.toBeNull();
    await act(async () => latestLogin.resolve(new Response(null, { status: 401 })));
    await expect(newestResult).resolves.toBe("Invalid username or password");
  });

  test("preserves user-facing login and registration errors", async () => {
    getMeMock.mockResolvedValue(null);
    loginMock.mockResolvedValue(new Response(null, { status: 401 }));
    registerMock
      .mockResolvedValueOnce(new Response(null, { status: 409 }))
      .mockResolvedValueOnce(new Response(null, { status: 403 }));
    mount();
    await waitFor(() => expect(screen.getByTestId("status")).toHaveTextContent("anonymous"));

    await act(async () => {
      await expect(currentAuth.login("http://server.test", "alice", "bad")).resolves.toBe(
        "Invalid username or password",
      );
    });
    await act(async () => {
      await expect(currentAuth.register("http://server.test", "alice", "bad")).resolves.toBe(
        "Username already taken",
      );
    });
    await act(async () => {
      await expect(currentAuth.register("http://server.test", "alice", "bad")).resolves.toBe(
        "Registration is disabled on this server",
      );
    });
  });
});
