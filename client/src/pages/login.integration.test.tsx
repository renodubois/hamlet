import { describe, expect, test } from "vitest";
import { render, screen, fireEvent, waitFor } from "@solidjs/testing-library";
import { Show } from "solid-js";
import { AuthProvider, useAuth } from "../auth_context";
import { assertExists } from "../test/render";
import LoginScreen from "./login";

function Harness() {
  const auth = useAuth();
  return (
    <Show when={auth.user()} fallback={<LoginScreen />}>
      {(user) => <div data-testid="welcome">welcome {user().username}</div>}
    </Show>
  );
}

function mount() {
  return render(() => (
    <AuthProvider>
      <Harness />
    </AuthProvider>
  ));
}

async function fillAndSubmit(username: string, password: string) {
  fireEvent.input(screen.getByPlaceholderText("Username"), { target: { value: username } });
  fireEvent.input(screen.getByPlaceholderText("Password"), { target: { value: password } });
  const form = assertExists(screen.getByRole("button", { name: /sign in/i }).closest("form"));
  fireEvent.submit(form);
}

describe("Login flow", () => {
  test("rejects wrong credentials with an inline error", async () => {
    mount();
    await fillAndSubmit("baipas", "wrong");
    await waitFor(() => {
      expect(screen.getByText(/invalid username or password/i)).toBeInTheDocument();
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
});
