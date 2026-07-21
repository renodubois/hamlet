import { http, HttpResponse } from "msw";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { act, screen, fireEvent, waitFor } from "@testing-library/react";
import { AuthProvider, useAuth } from "../contexts/auth";
import { resetMswState, server } from "../test/msw/server";
import { assertExists, renderNative } from "../test/render";
import { captureReactDiagnostics, type ReactDiagnosticsCapture } from "../test/setup";
import LoginScreen from "./login";

const TEST_SERVER = import.meta.env.VITE_HAMLET_DEFAULT_SERVER_URL ?? "http://127.0.0.1:3030";
let diagnostics: ReactDiagnosticsCapture;

beforeEach(() => {
  diagnostics = captureReactDiagnostics();
});

afterEach(() => {
  diagnostics.stop();
  expect(diagnostics.diagnostics).toEqual([]);
});

function Harness() {
  const auth = useAuth();
  const user = auth.user;
  return user ? <div data-testid="welcome">welcome {user.username}</div> : <LoginScreen />;
}

function mount() {
  return renderNative(
    <AuthProvider>
      <Harness />
    </AuthProvider>,
  );
}

async function fillAndSubmit(username: string, password: string) {
  fireEvent.input(screen.getByLabelText("Username"), { target: { value: username } });
  fireEvent.input(screen.getByLabelText("Password"), { target: { value: password } });
  const form = assertExists(screen.getByRole("button", { name: /sign in/i }).closest("form"));
  fireEvent.submit(form);
}

describe("Login flow", () => {
  test("exposes persistent accessible field labels", () => {
    mount();

    expect(screen.getByLabelText("Server URL")).toBeInTheDocument();
    expect(screen.getByLabelText("Username")).toBeInTheDocument();
    expect(screen.getByLabelText("Password")).toBeInTheDocument();
  });

  test("rejects wrong credentials with an inline error", async () => {
    mount();
    await fillAndSubmit("baipas", "wrong");
    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent("Invalid username or password");
    });
    expect(screen.queryByTestId("welcome")).toBeNull();
  });

  test("logs the user in on correct credentials and swaps to the welcome view", async () => {
    mount();
    await fillAndSubmit("baipas", "password");
    await waitFor(() => {
      expect(screen.getByTestId("welcome")).toHaveTextContent("welcome baipas");
    });
  });

  test("switching to register mode shows the email field", () => {
    mount();
    fireEvent.click(screen.getByRole("button", { name: /create one/i }));
    expect(screen.getByPlaceholderText(/email/i)).toBeInTheDocument();
  });

  test("hides registration affordances when the server disables account creation", async () => {
    resetMswState({ accountRegistrationEnabled: false });
    mount();

    await waitFor(() => {
      expect(screen.queryByRole("button", { name: /create one/i })).toBeNull();
    });
    expect(screen.queryByPlaceholderText(/email/i)).toBeNull();
  });

  test("password typing does not refetch the public server config", async () => {
    const configRequests = vi.fn();
    server.use(
      http.get(`${TEST_SERVER}/config`, () => {
        configRequests();
        return HttpResponse.json({ account_registration_enabled: true });
      }),
    );
    mount();
    await waitFor(() => expect(configRequests).toHaveBeenCalled());
    const requestsAfterMount = configRequests.mock.calls.length;

    fireEvent.input(screen.getByLabelText("Password"), { target: { value: "secret" } });

    await waitFor(() => expect(screen.getByLabelText("Password")).toHaveValue("secret"));
    expect(configRequests).toHaveBeenCalledTimes(requestsAfterMount);
  });

  test("a stale server config response cannot override the newest server", async () => {
    let resolveOld: (() => void) | undefined;
    const oldResponse = new Promise<void>((resolve) => {
      resolveOld = resolve;
    });
    const newestServer = "http://newest.example.test";
    const newestRequest = vi.fn();
    server.use(
      http.get(`${TEST_SERVER}/config`, async () => {
        await oldResponse;
        return HttpResponse.json({ account_registration_enabled: false });
      }),
      http.get(`${newestServer}/config`, () => {
        newestRequest();
        return HttpResponse.json({ account_registration_enabled: true });
      }),
    );
    mount();

    fireEvent.input(screen.getByPlaceholderText("Server URL"), {
      target: { value: `${newestServer}/` },
    });
    await waitFor(() => expect(newestRequest).toHaveBeenCalled());
    expect(screen.getByRole("button", { name: /create one/i })).toBeInTheDocument();

    await act(async () => {
      resolveOld?.();
      await oldResponse;
    });
    expect(screen.getByRole("button", { name: /create one/i })).toBeInTheDocument();
  });

  test("ignores a successful server config completion after unmount", async () => {
    let resolveConfig: (() => void) | undefined;
    const configResponse = new Promise<void>((resolve) => {
      resolveConfig = resolve;
    });
    const configRequests = vi.fn();
    server.use(
      http.get(`${TEST_SERVER}/config`, async () => {
        configRequests();
        await configResponse;
        return HttpResponse.json({ account_registration_enabled: false });
      }),
    );
    const view = mount();
    await waitFor(() => expect(configRequests).toHaveBeenCalled());

    view.unmount();
    await act(async () => {
      resolveConfig?.();
      await configResponse;
    });

    expect(screen.queryByRole("heading", { name: /sign in/i })).toBeNull();
  });

  test("ignores a failed server config completion after unmount", async () => {
    let resolveConfig: (() => void) | undefined;
    const configResponse = new Promise<void>((resolve) => {
      resolveConfig = resolve;
    });
    const configRequests = vi.fn();
    server.use(
      http.get(`${TEST_SERVER}/config`, async () => {
        configRequests();
        await configResponse;
        return HttpResponse.error();
      }),
    );
    const view = mount();
    await waitFor(() => expect(configRequests).toHaveBeenCalled());

    view.unmount();
    await act(async () => {
      resolveConfig?.();
      await configResponse;
    });

    expect(screen.queryByRole("heading", { name: /sign in/i })).toBeNull();
  });

  test("returns to login mode if registration becomes disabled", async () => {
    let resolveConfig: (() => void) | undefined;
    const configResponse = new Promise<void>((resolve) => {
      resolveConfig = resolve;
    });
    server.use(
      http.get(`${TEST_SERVER}/config`, async () => {
        await configResponse;
        return HttpResponse.json({ account_registration_enabled: false });
      }),
    );
    mount();
    fireEvent.click(screen.getByRole("button", { name: /create one/i }));
    expect(screen.getByPlaceholderText(/email/i)).toBeInTheDocument();

    await act(async () => {
      resolveConfig?.();
      await configResponse;
    });
    await waitFor(() => {
      expect(screen.getByRole("heading", { name: /sign in/i })).toBeInTheDocument();
    });
    expect(screen.queryByPlaceholderText(/email/i)).toBeNull();
  });
});
