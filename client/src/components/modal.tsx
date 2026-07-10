import { useLayoutEffect, useRef } from "react";

import { useStableDomId, If, type JSX } from "../hooks/react-state";

// Stack of currently-mounted modal content elements, topmost last. Nested modals
// use this so only the topmost one handles keyboard input (Tab trap, Escape).
const modalStack: HTMLElement[] = [];

const FOCUSABLE_SELECTOR = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[tabIndex]:not([tabIndex='-1'])",
].join(",");

function getFocusable(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
}

export default function Modal(props: {
  open: boolean;
  onClose: () => void;
  title: string;
  size?: "sm" | "lg";
  children: JSX.Element;
}) {
  const contentRef = useRef<HTMLDivElement | null>(null);
  const setContentRef = (el: HTMLDivElement | null) => {
    contentRef.current = el;
  };
  const titleId = useStableDomId();

  useLayoutEffect(() => {
    if (!props.open) return;
    const content = contentRef.current;
    if (!content) return;

    const previouslyFocused = document.activeElement as HTMLElement | null;
    modalStack.push(content);

    // Initial focus: respect [autoFocus] inside children, then first focusable,
    // else the dialog container itself (it has tabIndex="-1").
    const activeInside = content.contains(document.activeElement)
      ? (document.activeElement as HTMLElement)
      : null;
    const autoFocusEl = getFocusable(content).find(
      (element) =>
        element.hasAttribute("autofocus") ||
        ("autofocus" in element && Boolean((element as HTMLInputElement).autofocus)),
    );
    const initialFocus = activeInside ?? autoFocusEl ?? getFocusable(content)[0] ?? content;
    initialFocus.focus();

    const handler = (e: any) => {
      if (modalStack[modalStack.length - 1] !== content) return;
      if (e.key === "Escape") {
        e.preventDefault();
        props.onClose();
        return;
      }
      if (e.key !== "Tab") return;

      const focusables = getFocusable(content);
      if (focusables.length === 0) {
        e.preventDefault();
        content.focus();
        return;
      }
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      const active = document.activeElement;

      if (!content.contains(active)) {
        e.preventDefault();
        (e.shiftKey ? last : first).focus();
        return;
      }
      if (e.shiftKey && active === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && active === last) {
        e.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", handler);

    return () => {
      window.removeEventListener("keydown", handler);
      const i = modalStack.indexOf(content);
      if (i >= 0) modalStack.splice(i, 1);
      previouslyFocused?.focus?.();
    };
  });

  const sizeClass = () => (props.size === "lg" ? "w-4xl" : "w-96");

  return (
    <If when={props.open}>
      <div
        className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
        onClick={(e) => {
          if (e.currentTarget === e.target) props.onClose();
        }}
      >
        <div
          ref={setContentRef}
          role="dialog"
          aria-modal="true"
          aria-labelledby={titleId}
          tabIndex={-1}
          className={`max-h-[90vh] overflow-y-auto rounded-xl border border-border bg-popover p-6 text-popover-foreground shadow-xl outline-none ${sizeClass()}`}
        >
          <div className="flex items-center justify-between mb-4">
            <h2 id={titleId} className="text-lg font-semibold">
              {props.title}
            </h2>
            <button
              type="button"
              className="rounded-md text-xl leading-none text-muted-foreground transition-colors hover:text-foreground"
              onClick={props.onClose}
              aria-label="Close"
            >
              ×
            </button>
          </div>
          {props.children}
        </div>
      </div>
    </If>
  );
}
