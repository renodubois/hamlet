import { describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
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

  test("exposes role=dialog, aria-modal, and aria-labelledby pointing to the title", () => {
    mount(true);
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveAttribute("aria-modal", "true");
    const labelId = dialog.getAttribute("aria-labelledby") ?? "";
    expect(labelId).not.toBe("");
    expect(document.getElementById(labelId)).toHaveTextContent("Hello");
  });

  test("moves focus inside the dialog when opened", () => {
    const opener = document.createElement("button");
    opener.textContent = "opener";
    document.body.appendChild(opener);
    opener.focus();
    expect(document.activeElement).toBe(opener);

    mount(true);
    const dialog = screen.getByRole("dialog");
    expect(dialog.contains(document.activeElement)).toBe(true);

    opener.remove();
  });

  test("respects [autofocus] children when opened", () => {
    render(() => (
      <Modal open onClose={() => {}} title="Hello">
        <input placeholder="first" />
        <input placeholder="autofocused" autofocus />
      </Modal>
    ));
    expect(document.activeElement).toBe(screen.getByPlaceholderText("autofocused"));
  });

  test("Tab from the last focusable wraps to the first", () => {
    render(() => (
      <Modal open onClose={() => {}} title="Hello">
        <button type="button">a</button>
        <button type="button">b</button>
      </Modal>
    ));
    const closeBtn = screen.getByLabelText("Close");
    const b = screen.getByText("b");
    b.focus();
    expect(document.activeElement).toBe(b);
    fireEvent.keyDown(window, { key: "Tab" });
    expect(document.activeElement).toBe(closeBtn);
  });

  test("Shift+Tab from the first focusable wraps to the last", () => {
    render(() => (
      <Modal open onClose={() => {}} title="Hello">
        <button type="button">a</button>
        <button type="button">b</button>
      </Modal>
    ));
    const closeBtn = screen.getByLabelText("Close");
    const b = screen.getByText("b");
    closeBtn.focus();
    fireEvent.keyDown(window, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(b);
  });

  test("Tab from outside the dialog is redirected back inside", () => {
    const outside = document.createElement("button");
    outside.textContent = "outside";
    document.body.appendChild(outside);

    render(() => (
      <Modal open onClose={() => {}} title="Hello">
        <button type="button">inside</button>
      </Modal>
    ));
    outside.focus();
    expect(document.activeElement).toBe(outside);
    fireEvent.keyDown(window, { key: "Tab" });
    const dialog = screen.getByRole("dialog");
    expect(dialog.contains(document.activeElement)).toBe(true);

    outside.remove();
  });

  test("restores focus to the previously focused element on close", () => {
    const [open, setOpen] = createSignal(false);
    render(() => (
      <div>
        <button type="button">opener</button>
        <Modal open={open()} onClose={() => setOpen(false)} title="Hello">
          <button type="button">inside</button>
        </Modal>
      </div>
    ));
    const opener = screen.getByText("opener");
    opener.focus();
    setOpen(true);
    expect(document.activeElement).not.toBe(opener);
    setOpen(false);
    expect(document.activeElement).toBe(opener);
  });

  test("only the topmost nested modal handles Tab and Escape", () => {
    const outerClose = vi.fn();
    const innerClose = vi.fn();
    render(() => (
      <>
        <Modal open onClose={outerClose} title="Outer">
          <button type="button">outer-btn</button>
        </Modal>
        <Modal open onClose={innerClose} title="Inner">
          <button type="button">inner-btn</button>
        </Modal>
      </>
    ));

    // Escape only closes the inner (topmost) modal.
    fireEvent.keyDown(window, { key: "Escape" });
    expect(innerClose).toHaveBeenCalledTimes(1);
    expect(outerClose).not.toHaveBeenCalled();

    // Tab trap is scoped to the inner dialog — Tabbing from the inner button
    // wraps within the inner dialog, never reaching outer-btn.
    const innerBtn = screen.getByText("inner-btn");
    innerBtn.focus();
    fireEvent.keyDown(window, { key: "Tab" });
    const outerBtn = screen.getByText("outer-btn");
    expect(document.activeElement).not.toBe(outerBtn);
  });
});
