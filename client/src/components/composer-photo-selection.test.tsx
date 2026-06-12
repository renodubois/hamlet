import { fireEvent, render, screen, within } from "@solidjs/testing-library";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vitest";
import { MESSAGE_PHOTO_MAX_BYTES } from "../constants";
import { expectNoA11yViolations } from "../test/a11y";
import {
  PhotoAttachControl,
  SelectedPhotoPreviewList,
  createComposerPhotoSelection,
} from "./composer-photo-selection";

function mockObjectUrls() {
  const originalCreateObjectURL = Object.getOwnPropertyDescriptor(URL, "createObjectURL");
  const originalRevokeObjectURL = Object.getOwnPropertyDescriptor(URL, "revokeObjectURL");
  let nextUrl = 0;
  const createObjectURL = vi.fn(
    (file: Blob | MediaSource) => `blob:${(file as File).name ?? "photo"}-${nextUrl++}`,
  );
  const revokeObjectURL = vi.fn();

  Object.defineProperty(URL, "createObjectURL", { configurable: true, value: createObjectURL });
  Object.defineProperty(URL, "revokeObjectURL", { configurable: true, value: revokeObjectURL });

  return {
    createObjectURL,
    revokeObjectURL,
    restore() {
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
    },
  };
}

function renderHarness() {
  let fileInput: HTMLInputElement | null = null;
  const result = render(() => {
    const selection = createComposerPhotoSelection();

    return (
      <form>
        <SelectedPhotoPreviewList
          photos={selection.photos()}
          error={selection.error()}
          errorId="photo-error"
          onRemove={selection.removePhoto}
        />
        <PhotoAttachControl
          onFilesSelected={selection.addFiles}
          describedBy={selection.error() ? "photo-error" : undefined}
        />
        <input aria-label="Message text" />
      </form>
    );
  });

  fileInput = result.container.querySelector('input[type="file"]');
  if (!fileInput) throw new Error("file input not found");
  return { ...result, fileInput };
}

function photo(name: string, type = "image/png") {
  return new File(["photo"], name, { type });
}

describe("composer photo selection", () => {
  test("exposes a keyboard-focusable attach-photo control without IPC", async () => {
    const urls = mockObjectUrls();
    const user = userEvent.setup();
    const { container, fileInput, unmount } = renderHarness();

    try {
      const attach = screen.getByRole("button", { name: /attach photos/i });
      expect(attach).toHaveClass("focus:outline-none", "focus:ring-2", "focus:ring-blue-400");
      expect(fileInput).toHaveAttribute("type", "file");
      expect(fileInput).toHaveAttribute("accept", "image/jpeg,image/png,image/webp");
      expect(fileInput).toHaveAttribute("multiple");

      const filePickerClick = vi.spyOn(fileInput, "click").mockImplementation(() => undefined);
      fireEvent.click(attach);
      expect(filePickerClick).toHaveBeenCalledTimes(1);
      filePickerClick.mockRestore();

      await user.tab();
      expect(attach).toHaveFocus();
      await expectNoA11yViolations(container, "composer photo attach control");
    } finally {
      unmount();
      urls.restore();
    }
  });

  test("previews selected photos and revokes object URLs when a sender removes one", () => {
    const urls = mockObjectUrls();
    const { fileInput, unmount } = renderHarness();

    try {
      fireEvent.change(fileInput, {
        target: { files: [photo("one.png"), photo("two.webp", "image/webp")] },
      });

      expect(urls.createObjectURL).toHaveBeenCalledTimes(2);
      const list = screen.getByRole("list", { name: /2 selected photos/i });
      expect(
        within(list).getByRole("img", { name: /selected photo 1: one\.png/i }),
      ).toHaveAttribute("src", "blob:one.png-0");
      expect(
        within(list).getByRole("img", { name: /selected photo 2: two\.webp/i }),
      ).toHaveAttribute("src", "blob:two.webp-1");

      fireEvent.click(screen.getByRole("button", { name: /remove selected photo 1: one\.png/i }));

      expect(urls.revokeObjectURL).toHaveBeenCalledWith("blob:one.png-0");
      expect(screen.queryByRole("img", { name: /one\.png/i })).toBeNull();
      expect(screen.getByRole("list", { name: /1 selected photo/i })).toBeInTheDocument();
    } finally {
      unmount();
      urls.restore();
    }
  });

  test.each([
    [
      "too many photos",
      Array.from({ length: 5 }, (_, index) => photo(`photo-${index}.png`)),
      /up to 4 photos/i,
    ],
    [
      "unsupported type",
      [photo("animated.gif", "image/gif")],
      /animated\.gif must be a JPEG, PNG, or WebP image/i,
    ],
  ])("announces %s validation errors and leaves previews unchanged", (_, files, message) => {
    const urls = mockObjectUrls();
    const { fileInput, unmount } = renderHarness();

    try {
      fireEvent.change(fileInput, { target: { files } });

      expect(screen.getByRole("alert")).toHaveTextContent(message);
      expect(screen.queryByRole("list", { name: /selected photos?/i })).toBeNull();
      expect(urls.createObjectURL).not.toHaveBeenCalled();
    } finally {
      unmount();
      urls.restore();
    }
  });

  test("announces too-large photo validation errors accessibly", () => {
    const urls = mockObjectUrls();
    const { fileInput, unmount } = renderHarness();
    const huge = photo("huge.png");
    Object.defineProperty(huge, "size", { value: MESSAGE_PHOTO_MAX_BYTES + 1 });

    try {
      fireEvent.change(fileInput, { target: { files: [huge] } });

      expect(screen.getByRole("alert")).toHaveTextContent(/huge\.png is larger than 10 MB/i);
      expect(screen.queryByRole("list", { name: /selected photos?/i })).toBeNull();
      expect(urls.createObjectURL).not.toHaveBeenCalled();
    } finally {
      unmount();
      urls.restore();
    }
  });

  test("revokes selected photo object URLs on cleanup", () => {
    const urls = mockObjectUrls();
    const { fileInput, unmount } = renderHarness();

    fireEvent.change(fileInput, { target: { files: [photo("cleanup.png")] } });
    unmount();

    expect(urls.revokeObjectURL).toHaveBeenCalledWith("blob:cleanup.png-0");
    urls.restore();
  });
});
