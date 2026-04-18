import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { describe, expect, test, vi } from "vitest";
import { type User } from "../api";
import { expectNoA11yViolations } from "../test/a11y";
import { DEV_USER } from "../test/msw/handlers";
import { mswState, resetMswState } from "../test/msw/server";
import SettingsModal from "./settings_modal";

const USER: User = {
  id: 1,
  username: "alice",
  email: null,
  email_verified: false,
  avatar_url: null,
};

// cropperjs is a web-components library that doesn't render meaningfully under
// happy-dom. Replace it with a stub whose selection returns a tiny canvas so
// the Save path can still produce a Blob for the upload request.
vi.mock("cropperjs", () => {
  class FakeSelection {
    aspectRatio = 1;
    initialCoverage = 0.8;
    async $toCanvas({ width = 16, height = 16 }: { width?: number; height?: number } = {}) {
      const canvas = document.createElement("canvas");
      canvas.width = width;
      canvas.height = height;
      // Monkey-patch toBlob because happy-dom's default is null.
      canvas.toBlob = (cb: (blob: Blob | null) => void) => {
        cb(new Blob([new Uint8Array([82, 73, 70, 70])], { type: "image/webp" }));
      };
      return canvas;
    }
  }
  return {
    default: class FakeCropper {
      selection = new FakeSelection();
      getCropperSelection() {
        return this.selection;
      }
      destroy() {}
    },
  };
});

function mount(
  open: boolean,
  opts: {
    onLogout?: () => Promise<void>;
    onClose?: () => void;
    user?: User | null;
    onAvatarChange?: () => void;
  } = {},
) {
  const onClose = opts.onClose ?? vi.fn();
  const onAvatarChange = opts.onAvatarChange ?? vi.fn();
  const result = render(() => (
    <SettingsModal
      open={open}
      onClose={onClose}
      onLogout={opts.onLogout ?? (async () => {})}
      user={opts.user ?? USER}
      onAvatarChange={onAvatarChange}
    />
  ));
  return { ...result, onClose, onAvatarChange };
}

describe("<SettingsModal>", () => {
  test("renders nothing when closed", () => {
    mount(false);
    expect(screen.queryByRole("tab", { name: "User Profile" })).toBeNull();
  });

  test("shows the User Profile section by default when open", () => {
    mount(true);
    const profileTab = screen.getByRole("tab", { name: "User Profile" });
    const voiceTab = screen.getByRole("tab", { name: "Voice & Video" });
    expect(profileTab).toHaveAttribute("aria-selected", "true");
    expect(voiceTab).toHaveAttribute("aria-selected", "false");
    expect(screen.getByText("alice")).toBeInTheDocument();
    expect(screen.getByLabelText(/choose profile picture/i)).toBeInTheDocument();
  });

  test("swaps to the Voice & Video Section when its tab is clicked", () => {
    mount(true);
    fireEvent.click(screen.getByRole("tab", { name: "Voice & Video" }));
    expect(screen.getByRole("tab", { name: "Voice & Video" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByLabelText("Input device")).toBeInTheDocument();
    expect(screen.getByLabelText("Output device")).toBeInTheDocument();
  });

  test("calls onClose when the close button is clicked", () => {
    const { onClose } = mount(true);
    fireEvent.click(screen.getAllByLabelText("Close")[0]);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  test("Log Out opens a confirmation modal and calls onLogout on confirm", async () => {
    const onLogout = vi.fn(async () => {});
    mount(true, { onLogout });
    fireEvent.click(screen.getByRole("button", { name: /log out/i }));
    expect(screen.getByText(/are you sure you want to log out/i)).toBeInTheDocument();
    const buttons = screen.getAllByRole("button", { name: /log out/i });
    fireEvent.click(buttons[buttons.length - 1]);
    await waitFor(() => expect(onLogout).toHaveBeenCalledTimes(1));
  });

  test("shows 'Remove picture' only when the user has an avatar", () => {
    const { unmount } = mount(true, { user: { ...USER, avatar_url: null } });
    expect(screen.queryByRole("button", { name: /remove picture/i })).toBeNull();
    unmount();

    mount(true, {
      user: { ...USER, avatar_url: "/uploads/avatars/1.webp?v=10" },
    });
    expect(screen.getByRole("button", { name: /remove picture/i })).toBeInTheDocument();
  });

  test("picking a file opens the cropper dialog", async () => {
    mount(true);
    const input = screen.getByLabelText(/choose profile picture/i) as HTMLInputElement;
    const file = new File([new Uint8Array([1, 2, 3])], "pic.png", { type: "image/png" });
    fireEvent.change(input, { target: { files: [file] } });

    await waitFor(() => {
      expect(screen.getByRole("dialog", { name: /crop your picture/i })).toBeInTheDocument();
    });
  });

  test("saving the cropper uploads the avatar and notifies the parent", async () => {
    resetMswState({ me: { ...DEV_USER, username: "alice" } });
    const onAvatarChange = vi.fn();
    mount(true, { onAvatarChange });
    const input = screen.getByLabelText(/choose profile picture/i) as HTMLInputElement;
    const file = new File([new Uint8Array([1, 2, 3])], "pic.png", { type: "image/png" });
    fireEvent.change(input, { target: { files: [file] } });

    await waitFor(() =>
      expect(screen.getByRole("dialog", { name: /crop your picture/i })).toBeInTheDocument(),
    );

    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() => {
      expect(mswState().uploadedAvatars.length).toBe(1);
      expect(onAvatarChange).toHaveBeenCalled();
    });
  });

  test("Remove picture calls DELETE /me/avatar and notifies the parent", async () => {
    resetMswState({ me: { ...DEV_USER, avatar_url: "/uploads/avatars/1.webp?v=5" } });
    const onAvatarChange = vi.fn();
    mount(true, {
      user: { ...USER, avatar_url: "/uploads/avatars/1.webp?v=5" },
      onAvatarChange,
    });
    fireEvent.click(screen.getByRole("button", { name: /remove picture/i }));
    await waitFor(() => {
      expect(mswState().deletedAvatar).toBe(true);
      expect(onAvatarChange).toHaveBeenCalled();
    });
  });

  test("has no axe violations when open on the Profile tab", async () => {
    const { container } = mount(true);
    await expectNoA11yViolations(container, "SettingsModal profile");
  });

  test("has no axe violations with the logout confirm open", async () => {
    const { container } = mount(true);
    fireEvent.click(screen.getByRole("button", { name: /log out/i }));
    await expectNoA11yViolations(container, "SettingsModal + confirm");
  });
});
