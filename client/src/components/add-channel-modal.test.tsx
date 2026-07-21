import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, test, vi } from "vitest";
import { renderNative } from "../test/render";
import AddChannelModal from "./add-channel-modal";

const { createChannel } = vi.hoisted(() => ({ createChannel: vi.fn() }));

vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  createChannel,
}));

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function response(status: number): Response {
  return { ok: status >= 200 && status < 300, status } as Response;
}

function submitForm() {
  const form = screen.getByPlaceholderText("Channel name").closest("form");
  expect(form).not.toBeNull();
  fireEvent.submit(form as HTMLFormElement);
}

describe("<AddChannelModal>", () => {
  beforeEach(() => {
    createChannel.mockReset();
  });

  test("an old success cannot close a reopened modal or clear a newer pending create", async () => {
    const first = deferred<Response>();
    const second = deferred<Response>();
    createChannel
      .mockImplementationOnce(() => first.promise)
      .mockImplementationOnce(() => second.promise);
    const onClose = vi.fn();
    const { rerender } = renderNative(<AddChannelModal open={true} onClose={onClose} />);

    fireEvent.input(screen.getByPlaceholderText("Channel name"), { target: { value: "old" } });
    submitForm();
    expect(screen.getByRole("button", { name: "Creating..." })).toBeDisabled();

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    rerender(<AddChannelModal open={false} onClose={onClose} />);
    rerender(<AddChannelModal open={true} onClose={onClose} />);

    fireEvent.input(screen.getByPlaceholderText("Channel name"), {
      target: { value: "new draft" },
    });
    submitForm();
    expect(createChannel).toHaveBeenNthCalledWith(2, "new draft", "text");

    await act(async () => {
      first.resolve(response(201));
      await first.promise;
    });

    expect(onClose).toHaveBeenCalledTimes(1);
    expect(screen.getByPlaceholderText("Channel name")).toHaveValue("new draft");
    expect(screen.getByRole("button", { name: "Creating..." })).toBeDisabled();

    await act(async () => {
      second.resolve(response(500));
      await second.promise;
    });
    expect(await screen.findByText("Server error, please try again.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create" })).toBeEnabled();
  });

  test("an old error cannot overwrite a reopened draft", async () => {
    const request = deferred<Response>();
    createChannel.mockImplementationOnce(() => request.promise);
    const onClose = vi.fn();
    const { rerender } = renderNative(<AddChannelModal open={true} onClose={onClose} />);

    fireEvent.input(screen.getByPlaceholderText("Channel name"), { target: { value: "old" } });
    submitForm();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    rerender(<AddChannelModal open={false} onClose={onClose} />);
    rerender(<AddChannelModal open={true} onClose={onClose} />);
    fireEvent.input(screen.getByPlaceholderText("Channel name"), { target: { value: "keep me" } });

    await act(async () => {
      request.reject(new Error("offline"));
      await request.promise.catch(() => undefined);
    });

    await waitFor(() => expect(screen.getByPlaceholderText("Channel name")).toHaveValue("keep me"));
    expect(screen.queryByText("Could not reach server.")).toBeNull();
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  test("pending completion after unmount is ignored", async () => {
    const request = deferred<Response>();
    createChannel.mockImplementationOnce(() => request.promise);
    const onClose = vi.fn();
    const { unmount } = renderNative(<AddChannelModal open={true} onClose={onClose} />);

    fireEvent.input(screen.getByPlaceholderText("Channel name"), { target: { value: "old" } });
    submitForm();
    unmount();

    await act(async () => {
      request.resolve(response(201));
      await request.promise;
    });
    expect(onClose).not.toHaveBeenCalled();
  });
});
