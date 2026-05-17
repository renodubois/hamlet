import { describe, test } from "vitest";
import { render } from "@solidjs/testing-library";
import Modal from "./components/modal";
import LoginScreen from "./pages/login";
import { AuthProvider } from "./contexts/auth";
import { expectNoA11yViolations } from "./test/a11y";

describe("Accessibility", () => {
  test("open modal has no axe violations", async () => {
    const { container } = render(() => (
      <Modal open onClose={() => {}} title="Create Channel">
        <label>
          Channel name
          <input type="text" />
        </label>
        <button type="button">Save</button>
      </Modal>
    ));
    await expectNoA11yViolations(container, "Modal");
  });

  test("login screen has no axe violations", async () => {
    const { container } = render(() => (
      <AuthProvider>
        <LoginScreen />
      </AuthProvider>
    ));
    await expectNoA11yViolations(container, "LoginScreen");
  });
});
