import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { delay, http, HttpResponse } from "msw";
import { describe, expect, test, vi } from "vitest";
import { type User } from "../api";
import { AuthProvider } from "../contexts/auth";
import { CustomEmojisProvider } from "../contexts/custom-emojis";
import { expectNoA11yViolations } from "../test/a11y";
import { DEV_USER } from "../test/msw/handlers";
import { mswState, resetMswState, server } from "../test/msw/server";
import SettingsModal from "./settings-modal";

const TEST_SERVER = import.meta.env.VITE_HAMLET_DEFAULT_SERVER_URL ?? "http://127.0.0.1:3030";

const USER: User = {
  id: 1,
  username: "alice",
  display_name: null,
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

function modalProps(
  opts: {
    onLogout?: () => Promise<void>;
    onClose?: () => void;
    user?: User | null;
    onAvatarChange?: () => void;
  } = {},
) {
  return {
    onClose: opts.onClose ?? vi.fn(),
    onAvatarChange: opts.onAvatarChange ?? vi.fn(),
    onLogout: opts.onLogout ?? (async () => {}),
    user: opts.user ?? USER,
  };
}

function mount(
  open: boolean,
  opts: {
    onLogout?: () => Promise<void>;
    onClose?: () => void;
    user?: User | null;
    onAvatarChange?: () => void;
  } = {},
) {
  const props = modalProps(opts);
  const result = render(() => (
    <SettingsModal
      open={open}
      onClose={props.onClose}
      onLogout={props.onLogout}
      user={props.user}
      onAvatarChange={props.onAvatarChange}
    />
  ));
  return { ...result, onClose: props.onClose, onAvatarChange: props.onAvatarChange };
}

function mountWithCustomEmojiProvider(open = true) {
  const props = modalProps();
  const result = render(() => (
    <AuthProvider>
      <CustomEmojisProvider>
        <SettingsModal
          open={open}
          onClose={props.onClose}
          onLogout={props.onLogout}
          user={props.user}
          onAvatarChange={props.onAvatarChange}
        />
      </CustomEmojisProvider>
    </AuthProvider>
  ));
  return { ...result, onClose: props.onClose, onAvatarChange: props.onAvatarChange };
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
    expect(screen.getByLabelText("Camera")).toBeInTheDocument();
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

    const cropDialog = await screen.findByRole("dialog", { name: /crop your picture/i });

    fireEvent.click(within(cropDialog).getByRole("button", { name: /^save$/i }));

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

  test("Save button sends PUT /me with the new display name", async () => {
    resetMswState({ me: { ...DEV_USER, username: "alice" } });
    const onAvatarChange = vi.fn();
    mount(true, { onAvatarChange });

    const input = screen.getByLabelText(/display name/i) as HTMLInputElement;
    fireEvent.input(input, { target: { value: "Ally" } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() => {
      expect(mswState().displayNameUpdates).toContainEqual("Ally");
      expect(onAvatarChange).toHaveBeenCalled();
    });
  });

  test("Save sends null when the field is cleared to whitespace", async () => {
    resetMswState({ me: { ...DEV_USER, username: "alice", display_name: "Ally" } });
    mount(true, { user: { ...USER, display_name: "Ally" } });

    const input = screen.getByLabelText(/display name/i) as HTMLInputElement;
    fireEvent.input(input, { target: { value: "   " } });
    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() => {
      expect(mswState().displayNameUpdates).toContainEqual(null);
    });
  });

  test("Reset to username is only shown when a display name is set and it clears the name", async () => {
    // With no display name, the reset button is hidden.
    const { unmount } = mount(true);
    expect(screen.queryByRole("button", { name: /reset to username/i })).toBeNull();
    unmount();

    resetMswState({ me: { ...DEV_USER, username: "alice", display_name: "Ally" } });
    mount(true, { user: { ...USER, display_name: "Ally" } });
    fireEvent.click(screen.getByRole("button", { name: /reset to username/i }));

    await waitFor(() => {
      expect(mswState().displayNameUpdates).toContainEqual(null);
    });
  });

  test("shows the display name (and @username) in the header when set", () => {
    mount(true, { user: { ...USER, display_name: "Ally" } });
    expect(screen.getByRole("heading", { name: /settings/i })).toBeInTheDocument();
    expect(screen.getByText("Ally")).toBeInTheDocument();
    expect(screen.getByText("@alice")).toBeInTheDocument();
  });

  test("Custom Emojis tab shows a loading state from the registry", async () => {
    resetMswState({ me: DEV_USER });
    server.use(
      http.get(`${TEST_SERVER}/emojis`, async () => {
        await delay("infinite");
        return HttpResponse.json([]);
      }),
    );
    mountWithCustomEmojiProvider();

    fireEvent.click(screen.getByRole("tab", { name: "Custom Emojis" }));

    expect(await screen.findByText(/loading custom emojis/i)).toBeInTheDocument();
  });

  test("Custom Emojis tab shows an empty state from the registry", async () => {
    resetMswState({ me: DEV_USER, customEmojis: [] });
    mountWithCustomEmojiProvider();

    fireEvent.click(screen.getByRole("tab", { name: "Custom Emojis" }));

    expect(await screen.findByText(/no custom emojis yet/i)).toBeInTheDocument();
    expect(screen.getByText(/uploaded emojis will be listed here/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/emoji name/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/image file/i)).toHaveAttribute(
      "accept",
      "image/png,image/jpeg,image/webp,image/gif",
    );
  });

  test("Custom Emojis tab uploads a static emoji and updates the list", async () => {
    resetMswState({ me: DEV_USER, customEmojis: [] });
    mountWithCustomEmojiProvider();

    fireEvent.click(screen.getByRole("tab", { name: "Custom Emojis" }));
    await screen.findByText(/no custom emojis yet/i);

    fireEvent.input(screen.getByLabelText(/emoji name/i), { target: { value: "party" } });
    const file = new File([new Uint8Array([1, 2, 3])], "party.png", { type: "image/png" });
    fireEvent.change(screen.getByLabelText(/image file/i), { target: { files: [file] } });
    fireEvent.click(screen.getByRole("button", { name: /upload emoji/i }));

    await waitFor(() => {
      expect(mswState().uploadedCustomEmojis).toContainEqual({
        name: "party",
        size: 3,
        type: "image/png",
      });
    });
    expect(await screen.findByText(":party:")).toBeInTheDocument();
    expect(screen.getByText(/ID \d+/)).toBeInTheDocument();
    expect(screen.getByText("static")).toBeInTheDocument();
  });

  test("Custom Emojis tab accepts animated GIF uploads and previews the selected file", async () => {
    resetMswState({ me: DEV_USER, customEmojis: [] });
    const originalCreateObjectURL = Object.getOwnPropertyDescriptor(URL, "createObjectURL");
    const originalRevokeObjectURL = Object.getOwnPropertyDescriptor(URL, "revokeObjectURL");
    const createObjectURL = vi.fn(() => "blob:animated-gif");
    const revokeObjectURL = vi.fn();
    Object.defineProperty(URL, "createObjectURL", { configurable: true, value: createObjectURL });
    Object.defineProperty(URL, "revokeObjectURL", { configurable: true, value: revokeObjectURL });
    mountWithCustomEmojiProvider();

    fireEvent.click(screen.getByRole("tab", { name: "Custom Emojis" }));
    await screen.findByText(/no custom emojis yet/i);

    fireEvent.input(screen.getByLabelText(/emoji name/i), { target: { value: "dance" } });
    const file = new File([new Uint8Array([71, 73, 70])], "dance.gif", { type: "image/gif" });
    fireEvent.change(screen.getByLabelText(/image file/i), { target: { files: [file] } });

    const preview = await screen.findByRole("img", { name: /selected custom emoji preview/i });
    expect(preview).toHaveAttribute("src", "blob:animated-gif");

    fireEvent.click(screen.getByRole("button", { name: /upload emoji/i }));

    await waitFor(() => {
      expect(mswState().uploadedCustomEmojis).toContainEqual({
        name: "dance",
        size: 3,
        type: "image/gif",
      });
    });
    expect(await screen.findByText(":dance:")).toBeInTheDocument();
    expect(screen.getByText("animated")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: ":dance:" })).toHaveAttribute(
      "src",
      expect.stringContaining(`${TEST_SERVER}/uploads/emojis/dance.gif`),
    );

    if (originalCreateObjectURL) {
      Object.defineProperty(URL, "createObjectURL", originalCreateObjectURL);
    } else {
      delete (URL as unknown as { createObjectURL?: unknown }).createObjectURL;
    }
    if (originalRevokeObjectURL) {
      Object.defineProperty(URL, "revokeObjectURL", originalRevokeObjectURL);
    } else {
      delete (URL as unknown as { revokeObjectURL?: unknown }).revokeObjectURL;
    }
  });

  test("Custom Emojis upload validates names and file types before submit", async () => {
    resetMswState({ me: DEV_USER, customEmojis: [] });
    mountWithCustomEmojiProvider();

    fireEvent.click(screen.getByRole("tab", { name: "Custom Emojis" }));
    await screen.findByText(/no custom emojis yet/i);

    fireEvent.input(screen.getByLabelText(/emoji name/i), { target: { value: "bad-name" } });
    const file = new File([new Uint8Array([1])], "bad.txt", { type: "text/plain" });
    fireEvent.change(screen.getByLabelText(/image file/i), { target: { files: [file] } });

    expect(screen.getByRole("button", { name: /upload emoji/i })).toBeDisabled();
    expect(
      screen.getByText(/choose a PNG, JPEG, static WebP, animated GIF, or animated WebP image/i),
    ).toBeInTheDocument();
    expect(screen.getAllByText(/letters, numbers, or underscores/i).length).toBeGreaterThan(1);
  });

  test("Custom Emojis upload shows server validation errors", async () => {
    resetMswState({ me: DEV_USER, customEmojis: [] });
    server.use(
      http.post(`${TEST_SERVER}/emojis`, () =>
        HttpResponse.json(
          { error: { kind: "emoji_name_taken", message: "custom emoji name already exists" } },
          { status: 409 },
        ),
      ),
    );
    mountWithCustomEmojiProvider();

    fireEvent.click(screen.getByRole("tab", { name: "Custom Emojis" }));
    await screen.findByText(/no custom emojis yet/i);

    fireEvent.input(screen.getByLabelText(/emoji name/i), { target: { value: "party" } });
    const file = new File([new Uint8Array([1])], "party.webp", { type: "image/webp" });
    fireEvent.change(screen.getByLabelText(/image file/i), { target: { files: [file] } });
    fireEvent.click(screen.getByRole("button", { name: /upload emoji/i }));

    expect(await screen.findByRole("alert")).toHaveTextContent(/custom emoji name already exists/i);
  });

  test("Custom Emojis tab renames an existing emoji and updates the list", async () => {
    resetMswState({
      me: DEV_USER,
      customEmojis: [
        {
          id: 123,
          name: "party",
          image_url: "/uploads/emojis/123.webp?v=10",
          animated: false,
          created_by_user_id: 1,
          created_at: 10,
          updated_at: 10,
          deleted_at: null,
        },
      ],
    });
    mountWithCustomEmojiProvider();

    fireEvent.click(screen.getByRole("tab", { name: "Custom Emojis" }));
    await screen.findByText(":party:");

    fireEvent.input(screen.getByLabelText(/rename :party:/i), {
      target: { value: "renamed_party" },
    });
    fireEvent.click(screen.getByRole("button", { name: /save rename/i }));

    await waitFor(() => {
      expect(mswState().renamedCustomEmojis).toContainEqual({ id: 123, name: "renamed_party" });
    });
    expect(await screen.findByText(":renamed_party:")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(/renamed to :renamed_party:/i);
  });

  test("Custom Emojis tab soft-deletes after confirmation and moves to deleted view", async () => {
    const originalConfirm = window.confirm;
    Object.defineProperty(window, "confirm", { configurable: true, value: vi.fn(() => true) });
    resetMswState({
      me: DEV_USER,
      customEmojis: [
        {
          id: 123,
          name: "party",
          image_url: "/uploads/emojis/123.webp?v=10",
          animated: false,
          created_by_user_id: 1,
          created_at: 10,
          updated_at: 10,
          deleted_at: null,
        },
      ],
    });
    mountWithCustomEmojiProvider();

    fireEvent.click(screen.getByRole("tab", { name: "Custom Emojis" }));
    await screen.findByText(":party:");
    fireEvent.click(screen.getByRole("button", { name: /^delete$/i }));

    await waitFor(() => expect(mswState().deletedCustomEmojiIds).toContain(123));
    expect(screen.getByText(/no active custom emojis/i)).toBeInTheDocument();
    const deletedSection = screen.getByText(/deleted emojis/i).parentElement;
    expect(deletedSection).not.toBeNull();
    expect(within(deletedSection as HTMLElement).getByText(":party:")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^restore$/i })).toBeInTheDocument();
    Object.defineProperty(window, "confirm", { configurable: true, value: originalConfirm });
  });

  test("Custom Emojis tab restores deleted emojis and shows conflicts", async () => {
    resetMswState({
      me: DEV_USER,
      customEmojis: [
        {
          id: 123,
          name: "party",
          image_url: "/uploads/emojis/123.webp?v=10",
          animated: false,
          created_by_user_id: 1,
          created_at: 10,
          updated_at: 10,
          deleted_at: 20,
        },
        {
          id: 124,
          name: "party",
          image_url: "/uploads/emojis/124.webp?v=10",
          animated: false,
          created_by_user_id: 1,
          created_at: 10,
          updated_at: 10,
          deleted_at: null,
        },
      ],
    });
    mountWithCustomEmojiProvider();

    fireEvent.click(screen.getByRole("tab", { name: "Custom Emojis" }));
    await screen.findAllByText(":party:");
    fireEvent.click(screen.getByRole("button", { name: /^restore$/i }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/custom emoji name already exists/i);

    const originalConfirm = window.confirm;
    Object.defineProperty(window, "confirm", { configurable: true, value: vi.fn(() => true) });
    fireEvent.click(screen.getAllByRole("button", { name: /^delete$/i })[0]);
    await waitFor(() => expect(mswState().deletedCustomEmojiIds).toContain(124));

    fireEvent.click(screen.getAllByRole("button", { name: /^restore$/i })[0]);
    await waitFor(() => expect(mswState().restoredCustomEmojiIds).toContain(123));
    Object.defineProperty(window, "confirm", { configurable: true, value: originalConfirm });
  });

  test("Custom Emojis tab shows rename conflicts", async () => {
    resetMswState({
      me: DEV_USER,
      customEmojis: [
        {
          id: 123,
          name: "party",
          image_url: "/uploads/emojis/123.webp?v=10",
          animated: false,
          created_by_user_id: 1,
          created_at: 10,
          updated_at: 10,
          deleted_at: null,
        },
        {
          id: 124,
          name: "KEKW",
          image_url: "/uploads/emojis/124.webp?v=10",
          animated: false,
          created_by_user_id: 1,
          created_at: 10,
          updated_at: 10,
          deleted_at: null,
        },
      ],
    });
    mountWithCustomEmojiProvider();

    fireEvent.click(screen.getByRole("tab", { name: "Custom Emojis" }));
    await screen.findByText(":party:");

    fireEvent.input(screen.getByLabelText(/rename :party:/i), { target: { value: "kekW" } });
    fireEvent.click(screen.getAllByRole("button", { name: /save rename/i })[0]);

    expect(await screen.findByRole("alert")).toHaveTextContent(/custom emoji name already exists/i);
    expect(screen.getByText(":party:")).toBeInTheDocument();
  });

  test("Custom Emojis tab shows an error state from the registry", async () => {
    resetMswState({ me: DEV_USER });
    server.use(http.get(`${TEST_SERVER}/emojis`, () => new HttpResponse(null, { status: 500 })));
    mountWithCustomEmojiProvider();

    fireEvent.click(screen.getByRole("tab", { name: "Custom Emojis" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(/could not load custom emojis/i);
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
