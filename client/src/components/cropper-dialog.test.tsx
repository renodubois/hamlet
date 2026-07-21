import { StrictMode } from "react";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, test, vi } from "vitest";
import { renderNative } from "../test/render";
import CropperDialog from "./cropper-dialog";

interface FakeCropperInstance {
  destroy: ReturnType<typeof vi.fn>;
  getCropperSelection: ReturnType<typeof vi.fn>;
}

const cropperMock = vi.hoisted(() => ({
  instances: [] as FakeCropperInstance[],
  canvasPromises: [] as Array<{
    resolve: (canvas: HTMLCanvasElement) => void;
    reject: (error: Error) => void;
  }>,
  deferCanvas: false,
}));

vi.mock("cropperjs", () => ({
  default: class FakeCropper {
    destroy = vi.fn();
    selection = {
      aspectRatio: 0,
      initialCoverage: 0,
      $toCanvas: vi.fn(() => {
        if (cropperMock.deferCanvas) {
          return new Promise<HTMLCanvasElement>((resolve, reject) => {
            cropperMock.canvasPromises.push({ resolve, reject });
          });
        }
        return Promise.resolve(makeCanvas());
      }),
    };
    getCropperSelection = vi.fn(() => this.selection);

    constructor() {
      cropperMock.instances.push(this);
    }
  },
}));

function makeCanvas() {
  const canvas = document.createElement("canvas");
  canvas.toBlob = (callback: BlobCallback) => callback(new Blob(["image"], { type: "image/webp" }));
  return canvas;
}

function activeCroppers() {
  return cropperMock.instances.filter((instance) => instance.destroy.mock.calls.length === 0);
}

function file(name: string) {
  return new File([name], name, { type: "image/png" });
}

let createdUrls: string[];
let revokedUrls: string[];

beforeEach(() => {
  cropperMock.instances.length = 0;
  cropperMock.canvasPromises.length = 0;
  cropperMock.deferCanvas = false;
  createdUrls = [];
  revokedUrls = [];
  vi.spyOn(URL, "createObjectURL").mockImplementation(() => {
    const url = `blob:test-${createdUrls.length + 1}`;
    createdUrls.push(url);
    return url;
  });
  vi.spyOn(URL, "revokeObjectURL").mockImplementation((url) => revokedUrls.push(url));
});

describe("<CropperDialog>", () => {
  test("replacement and close revoke each URL and destroy each Cropper owner exactly once", async () => {
    const firstFile = file("first.png");
    const secondFile = file("second.png");
    const view = renderNative(
      <CropperDialog open file={firstFile} onCancel={() => {}} onSave={async () => {}} />,
    );

    await waitFor(() => expect(activeCroppers()).toHaveLength(1));
    view.rerender(
      <StrictMode>
        <CropperDialog open file={secondFile} onCancel={() => {}} onSave={async () => {}} />
      </StrictMode>,
    );
    await waitFor(() => {
      expect(activeCroppers()).toHaveLength(1);
      expect(document.querySelector("img")).toHaveAttribute(
        "src",
        expect.stringContaining("blob:test-"),
      );
    });

    view.rerender(
      <StrictMode>
        <CropperDialog open={false} file={secondFile} onCancel={() => {}} onSave={async () => {}} />
      </StrictMode>,
    );
    await waitFor(() => expect(activeCroppers()).toHaveLength(0));

    for (const url of createdUrls) {
      expect(revokedUrls.filter((revokedUrl) => revokedUrl === url)).toHaveLength(1);
    }
    for (const instance of cropperMock.instances) {
      expect(instance.destroy).toHaveBeenCalledTimes(1);
    }
  });

  test("saving and error rerenders preserve the active Cropper instance", async () => {
    cropperMock.deferCanvas = true;
    renderNative(
      <CropperDialog open file={file("avatar.png")} onCancel={() => {}} onSave={async () => {}} />,
    );
    await waitFor(() => expect(activeCroppers()).toHaveLength(1));
    const active = activeCroppers()[0];
    const instanceCount = cropperMock.instances.length;

    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByRole("button", { name: "Saving..." })).toBeDisabled();
    expect(activeCroppers()[0]).toBe(active);
    expect(cropperMock.instances).toHaveLength(instanceCount);

    cropperMock.canvasPromises[0].reject(new Error("canvas failed"));
    expect(await screen.findByText("canvas failed")).toBeInTheDocument();
    expect(activeCroppers()[0]).toBe(active);
    expect(cropperMock.instances).toHaveLength(instanceCount);
  });

  test("ignores an asynchronous canvas completion after close", async () => {
    cropperMock.deferCanvas = true;
    const onSave = vi.fn(async () => {});
    const avatar = file("avatar.png");
    const view = renderNative(
      <CropperDialog open file={avatar} onCancel={() => {}} onSave={onSave} />,
    );
    await waitFor(() => expect(activeCroppers()).toHaveLength(1));

    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await screen.findByRole("button", { name: "Saving..." });
    const pendingCanvas = cropperMock.canvasPromises[0];
    view.rerender(
      <StrictMode>
        <CropperDialog open={false} file={avatar} onCancel={() => {}} onSave={onSave} />
      </StrictMode>,
    );
    await waitFor(() => expect(activeCroppers()).toHaveLength(0));

    pendingCanvas.resolve(makeCanvas());
    await Promise.resolve();
    expect(onSave).not.toHaveBeenCalled();
  });

  test("ignores an asynchronous canvas completion after file replacement", async () => {
    cropperMock.deferCanvas = true;
    const onSave = vi.fn(async () => {});
    const view = renderNative(
      <CropperDialog open file={file("first.png")} onCancel={() => {}} onSave={onSave} />,
    );
    await waitFor(() => expect(activeCroppers()).toHaveLength(1));

    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await screen.findByRole("button", { name: "Saving..." });
    const pendingCanvas = cropperMock.canvasPromises[0];
    view.rerender(
      <StrictMode>
        <CropperDialog open file={file("replacement.png")} onCancel={() => {}} onSave={onSave} />
      </StrictMode>,
    );
    await waitFor(() => expect(activeCroppers()).toHaveLength(1));

    pendingCanvas.resolve(makeCanvas());
    await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).toBeEnabled());
    expect(onSave).not.toHaveBeenCalled();
  });

  test("a pending onSave from a replaced file cannot reset the current save", async () => {
    let resolveFirst: (() => void) | undefined;
    let resolveSecond: (() => void) | undefined;
    const firstSave = new Promise<void>((resolve) => {
      resolveFirst = resolve;
    });
    const secondSave = new Promise<void>((resolve) => {
      resolveSecond = resolve;
    });
    const onSave = vi
      .fn<(blob: Blob) => Promise<void>>()
      .mockImplementationOnce(() => firstSave)
      .mockImplementationOnce(() => secondSave);
    const replacement = file("replacement.png");
    const view = renderNative(
      <CropperDialog open file={file("first.png")} onCancel={() => {}} onSave={onSave} />,
    );
    await waitFor(() => expect(activeCroppers()).toHaveLength(1));

    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1));
    view.rerender(
      <StrictMode>
        <CropperDialog open file={replacement} onCancel={() => {}} onSave={onSave} />
      </StrictMode>,
    );
    await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(2));

    await act(async () => {
      resolveFirst?.();
      await firstSave;
    });
    expect(screen.getByRole("button", { name: "Saving..." })).toBeDisabled();

    await act(async () => {
      resolveSecond?.();
      await secondSave;
    });
    await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).toBeEnabled());
  });

  test("a pending onSave from before close cannot reset a reopened save", async () => {
    let resolveFirst: (() => void) | undefined;
    let resolveSecond: (() => void) | undefined;
    const firstSave = new Promise<void>((resolve) => {
      resolveFirst = resolve;
    });
    const secondSave = new Promise<void>((resolve) => {
      resolveSecond = resolve;
    });
    const onSave = vi
      .fn<(blob: Blob) => Promise<void>>()
      .mockImplementationOnce(() => firstSave)
      .mockImplementationOnce(() => secondSave);
    const avatar = file("avatar.png");
    const view = renderNative(
      <CropperDialog open file={avatar} onCancel={() => {}} onSave={onSave} />,
    );
    await waitFor(() => expect(activeCroppers()).toHaveLength(1));

    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1));
    view.rerender(
      <StrictMode>
        <CropperDialog open={false} file={avatar} onCancel={() => {}} onSave={onSave} />
      </StrictMode>,
    );
    view.rerender(
      <StrictMode>
        <CropperDialog open file={avatar} onCancel={() => {}} onSave={onSave} />
      </StrictMode>,
    );
    await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(2));

    await act(async () => {
      resolveFirst?.();
      await firstSave;
    });
    expect(screen.getByRole("button", { name: "Saving..." })).toBeDisabled();

    await act(async () => {
      resolveSecond?.();
      await secondSave;
    });
    await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).toBeEnabled());
  });
});
