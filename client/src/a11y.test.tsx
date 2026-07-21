import { afterEach, beforeEach, describe, expect, test } from "vitest";
import { screen } from "@testing-library/react";
import { renderNative } from "./test/render";
import { captureReactDiagnostics, type ReactDiagnosticsCapture } from "./test/setup";
import Modal from "./components/modal";
import LoginScreen from "./pages/login";
import { AuthProvider, useAuth } from "./contexts/auth";
import { expectNoA11yViolations } from "./test/a11y";

let diagnostics: ReactDiagnosticsCapture;

function AnonymousLogin() {
  return useAuth().status === "anonymous" ? <LoginScreen /> : null;
}

beforeEach(() => {
  diagnostics = captureReactDiagnostics();
});

afterEach(() => {
  diagnostics.stop();
  expect(diagnostics.diagnostics).toEqual([]);
});

describe("Accessibility", () => {
  test("open modal has no axe violations", async () => {
    const { container } = renderNative(
      <Modal open onClose={() => {}} title="Create Channel">
        <label>
          Channel name
          <input type="text" />
        </label>
        <button type="button">Save</button>
      </Modal>,
    );
    await expectNoA11yViolations(container, "Modal");
  });

  test("login screen has no axe violations", async () => {
    const { container } = renderNative(
      <AuthProvider>
        <AnonymousLogin />
      </AuthProvider>,
    );
    expect(await screen.findByRole("heading", { name: "Sign in" })).toBeInTheDocument();
    expect(screen.getByLabelText("Server URL")).toBeInTheDocument();
    expect(screen.getByLabelText("Username")).toBeInTheDocument();
    expect(screen.getByLabelText("Password")).toBeInTheDocument();
    await expectNoA11yViolations(container, "LoginScreen");
  });
});
