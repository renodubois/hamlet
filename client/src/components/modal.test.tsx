import { StrictMode, useState } from "react";
import { act, fireEvent, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import { renderNative } from "../test/render";
import Modal from "./modal";

function mount(open: boolean, onClose = vi.fn()) {
  const result = renderNative(
    <Modal open={open} onClose={onClose} title="Hello">
      <p>body-content</p>
    </Modal>,
  );
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

  test("calls the latest onClose when Escape is pressed while open", () => {
    const firstClose = vi.fn();
    const latestClose = vi.fn();
    const view = mount(true, firstClose);
    view.rerender(
      <StrictMode>
        <Modal open onClose={latestClose} title="Hello">
          <p>body-content</p>
        </Modal>
      </StrictMode>,
    );

    fireEvent.keyDown(window, { key: "Escape" });
    expect(latestClose).toHaveBeenCalledTimes(1);
    expect(firstClose).not.toHaveBeenCalled();
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

    mount(true);
    const dialog = screen.getByRole("dialog");
    expect(dialog.contains(document.activeElement)).toBe(true);

    opener.remove();
  });

  test("respects [autoFocus] children when opened", () => {
    renderNative(
      <Modal open onClose={() => {}} title="Hello">
        <input placeholder="first" />
        <input placeholder="autoFocused" autoFocus />
      </Modal>,
    );
    expect(document.activeElement).toBe(screen.getByPlaceholderText("autoFocused"));
  });

  test("Tab from the last focusable wraps to the first", () => {
    renderNative(
      <Modal open onClose={() => {}} title="Hello">
        <button type="button">a</button>
        <button type="button">b</button>
      </Modal>,
    );
    const closeButton = screen.getByLabelText("Close");
    const lastButton = screen.getByText("b");
    lastButton.focus();
    fireEvent.keyDown(window, { key: "Tab" });
    expect(document.activeElement).toBe(closeButton);
  });

  test("Shift+Tab from the first focusable wraps to the last", () => {
    renderNative(
      <Modal open onClose={() => {}} title="Hello">
        <button type="button">a</button>
        <button type="button">b</button>
      </Modal>,
    );
    const closeButton = screen.getByLabelText("Close");
    const lastButton = screen.getByText("b");
    closeButton.focus();
    fireEvent.keyDown(window, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(lastButton);
  });

  test("Tab from outside the dialog is redirected back inside", () => {
    const outside = document.createElement("button");
    outside.textContent = "outside";
    document.body.appendChild(outside);

    renderNative(
      <Modal open onClose={() => {}} title="Hello">
        <button type="button">inside</button>
      </Modal>,
    );
    outside.focus();
    fireEvent.keyDown(window, { key: "Tab" });
    expect(screen.getByRole("dialog").contains(document.activeElement)).toBe(true);

    outside.remove();
  });

  test("restores focus to the previously focused element on close", () => {
    let setOpen: ((open: boolean) => void) | undefined;
    function Harness() {
      const [open, updateOpen] = useState(false);
      setOpen = updateOpen;
      return (
        <div>
          <button type="button">opener</button>
          <Modal open={open} onClose={() => updateOpen(false)} title="Hello">
            <button type="button">inside</button>
          </Modal>
        </div>
      );
    }

    renderNative(<Harness />);
    const opener = screen.getByText("opener");
    opener.focus();
    act(() => setOpen?.(true));
    expect(document.activeElement).not.toBe(opener);
    act(() => setOpen?.(false));
    expect(document.activeElement).toBe(opener);
  });

  test("rerendering controlled content preserves field focus and value", () => {
    const outside = document.createElement("button");
    document.body.appendChild(outside);
    outside.focus();

    function Harness() {
      const [value, setValue] = useState("");
      const [, setRenderCount] = useState(0);
      return (
        <Modal open onClose={() => {}} title="Hello">
          <input
            aria-label="Name"
            value={value}
            onChange={(event) => setValue(event.currentTarget.value)}
          />
          <button type="button" onClick={() => setRenderCount((count) => count + 1)}>
            Rerender
          </button>
        </Modal>
      );
    }

    renderNative(<Harness />);
    const input = screen.getByLabelText("Name");
    fireEvent.change(input, { target: { value: "alice" } });
    input.focus();
    fireEvent.click(screen.getByText("Rerender"));

    expect(input).toHaveValue("alice");
    expect(document.activeElement).toBe(input);
    expect(document.activeElement).not.toBe(outside);
    outside.remove();
  });

  test("removing an underlying modal does not steal focus from the surviving top modal", () => {
    let closeOuter: (() => void) | undefined;

    function Harness() {
      const [outerOpen, setOuterOpen] = useState(true);
      closeOuter = () => setOuterOpen(false);
      return (
        <>
          <Modal open={outerOpen} onClose={() => setOuterOpen(false)} title="Outer">
            <button type="button">outer-btn</button>
          </Modal>
          <Modal open onClose={() => {}} title="Inner">
            <input aria-label="inner-field" />
          </Modal>
        </>
      );
    }

    renderNative(<Harness />);
    const innerField = screen.getByLabelText("inner-field");
    innerField.focus();

    act(() => closeOuter?.());

    expect(screen.queryByRole("dialog", { name: "Outer" })).toBeNull();
    expect(document.activeElement).toBe(innerField);
  });

  test("only the topmost nested modal handles Tab and Escape", () => {
    const outerClose = vi.fn();
    const innerClose = vi.fn();
    renderNative(
      <>
        <Modal open onClose={outerClose} title="Outer">
          <button type="button">outer-btn</button>
        </Modal>
        <Modal open onClose={innerClose} title="Inner">
          <button type="button">inner-btn</button>
        </Modal>
      </>,
    );

    fireEvent.keyDown(window, { key: "Escape" });
    expect(innerClose).toHaveBeenCalledTimes(1);
    expect(outerClose).not.toHaveBeenCalled();

    const innerButton = screen.getByText("inner-btn");
    innerButton.focus();
    fireEvent.keyDown(window, { key: "Tab" });
    expect(document.activeElement).not.toBe(screen.getByText("outer-btn"));
  });
});
