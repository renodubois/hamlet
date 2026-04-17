import { describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen } from "@solidjs/testing-library";
import Modal from "./modal";

function mount(open: boolean, onClose = vi.fn()) {
  const result = render(() => (
    <Modal open={open} onClose={onClose} title="Hello">
      <p>body-content</p>
    </Modal>
  ));
  return { ...result, onClose };
}

describe("<Modal>", () => {
  test("renders nothing when closed", () => {
    mount(false);
    expect(screen.queryByText("body-content")).toBeNull();
  });

  test("renders title and children when open", () => {
    mount(true);
    expect(screen.getByText("Hello")).toBeInTheDocument();
    expect(screen.getByText("body-content")).toBeInTheDocument();
  });

  test("calls onClose when the close button is clicked", () => {
    const { onClose } = mount(true);
    fireEvent.click(screen.getByLabelText("Close"));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  test("calls onClose when the backdrop is clicked", () => {
    const { onClose, container } = mount(true);
    const backdrop = container.querySelector(".fixed.inset-0") as HTMLElement;
    fireEvent.click(backdrop);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  test("does not call onClose when content inside the dialog is clicked", () => {
    const { onClose } = mount(true);
    fireEvent.click(screen.getByText("body-content"));
    expect(onClose).not.toHaveBeenCalled();
  });

  test("calls onClose when Escape is pressed while open", () => {
    const { onClose } = mount(true);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  test("does not react to Escape when closed", () => {
    const { onClose } = mount(false);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).not.toHaveBeenCalled();
  });
});
