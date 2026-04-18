import { describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import SettingsModal from "./settings_modal";
import { expectNoA11yViolations } from "../test/a11y";

function mount(open: boolean, onLogout: () => Promise<void> = async () => {}, onClose = vi.fn()) {
  const result = render(() => <SettingsModal open={open} onClose={onClose} onLogout={onLogout} />);
  return { ...result, onClose };
}

describe("<SettingsModal>", () => {
  test("renders nothing when closed", () => {
    mount(false);
    expect(screen.queryByText(/user profile settings/i)).toBeNull();
    expect(screen.queryByRole("tab", { name: "User Profile" })).toBeNull();
  });

  test("shows the User Profile section by default when open", () => {
    mount(true);
    const profileTab = screen.getByRole("tab", { name: "User Profile" });
    const testTab = screen.getByRole("tab", { name: "Test Section" });
    expect(profileTab).toHaveAttribute("aria-selected", "true");
    expect(testTab).toHaveAttribute("aria-selected", "false");
    expect(screen.getByText(/user profile settings go here/i)).toBeInTheDocument();
    expect(screen.queryByText(/test section content/i)).toBeNull();
  });

  test("swaps to the Test Section when its tab is clicked", () => {
    mount(true);
    fireEvent.click(screen.getByRole("tab", { name: "Test Section" }));
    expect(screen.getByRole("tab", { name: "Test Section" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByRole("tab", { name: "User Profile" })).toHaveAttribute(
      "aria-selected",
      "false",
    );
    expect(screen.getByText(/test section content/i)).toBeInTheDocument();
    expect(screen.queryByText(/user profile settings go here/i)).toBeNull();
  });

  test("calls onClose when the close button is clicked", () => {
    const { onClose } = mount(true);
    fireEvent.click(screen.getByLabelText("Close"));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  test("Log Out is not a tab and does not swap the panel when clicked", () => {
    mount(true);
    const tabs = screen.getAllByRole("tab");
    expect(tabs.map((t) => t.textContent?.trim())).toEqual(["User Profile", "Test Section"]);
    fireEvent.click(screen.getByRole("button", { name: /log out/i }));
    // Profile tab remains selected after opening the confirm
    expect(screen.getByRole("tab", { name: "User Profile" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  test("clicking Log Out opens a confirmation modal", () => {
    mount(true);
    expect(screen.queryByText(/are you sure you want to log out/i)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /log out/i }));
    expect(screen.getByText(/are you sure you want to log out/i)).toBeInTheDocument();
  });

  test("confirming logout calls onLogout", async () => {
    const onLogout = vi.fn(async () => {});
    mount(true, onLogout);
    fireEvent.click(screen.getByRole("button", { name: /log out/i }));
    // Two buttons now match /log out/i — the sidebar entry (reopens confirm)
    // and the confirm dialog's primary action. Pick the last one, which is
    // inside the confirm modal.
    const buttons = screen.getAllByRole("button", { name: /log out/i });
    fireEvent.click(buttons[buttons.length - 1]);
    await waitFor(() => expect(onLogout).toHaveBeenCalledTimes(1));
  });

  test("cancel closes the confirm modal and keeps settings open", () => {
    mount(true);
    fireEvent.click(screen.getByRole("button", { name: /log out/i }));
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByText(/are you sure you want to log out/i)).toBeNull();
    // Settings modal is still open
    expect(screen.getByRole("tab", { name: "User Profile" })).toBeInTheDocument();
  });

  test("has no axe violations when open", async () => {
    const { container } = mount(true);
    await expectNoA11yViolations(container, "SettingsModal");
  });

  test("has no axe violations with the logout confirm open", async () => {
    const { container } = mount(true);
    fireEvent.click(screen.getByRole("button", { name: /log out/i }));
    await expectNoA11yViolations(container, "SettingsModal + confirm");
  });
});
