import { createEffect, createUniqueId, onCleanup, Show, type JSX } from "solid-js";

// Stack of currently-mounted modal content elements, topmost last. Nested modals
// use this so only the topmost one handles keyboard input (Tab trap, Escape).
const modalStack: HTMLElement[] = [];

const FOCUSABLE_SELECTOR = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[tabindex]:not([tabindex='-1'])",
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
  let contentRef: HTMLDivElement | undefined;
  const setContentRef = (el: HTMLDivElement) => {
    contentRef = el;
  };
  const titleId = createUniqueId();

  createEffect(() => {
    if (!props.open) return;
    const content = contentRef;
    if (!content) return;

    const previouslyFocused = document.activeElement as HTMLElement | null;
    modalStack.push(content);

    // Initial focus: respect [autofocus] inside children, then first focusable,
    // else the dialog container itself (it has tabindex="-1").
    const autofocusEl = content.querySelector<HTMLElement>("[autofocus]");
    const initialFocus = autofocusEl ?? getFocusable(content)[0] ?? content;
    initialFocus.focus();

    const handler = (e: KeyboardEvent) => {
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

    onCleanup(() => {
      window.removeEventListener("keydown", handler);
      const i = modalStack.indexOf(content);
      if (i >= 0) modalStack.splice(i, 1);
      previouslyFocused?.focus?.();
    });
  });

  const sizeClass = () => (props.size === "lg" ? "w-4xl" : "w-96");

  return (
    <Show when={props.open}>
      <div
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
        onClick={(e) => {
          if (e.currentTarget === e.target) props.onClose();
        }}
      >
        <div
          ref={setContentRef}
          role="dialog"
          aria-modal="true"
          aria-labelledby={titleId}
          tabindex="-1"
          class={`max-h-[90vh] overflow-y-auto bg-gray-800 text-gray-100 rounded-lg p-6 shadow-xl outline-none ${sizeClass()}`}
        >
          <div class="flex items-center justify-between mb-4">
            <h2 id={titleId} class="text-lg font-semibold">
              {props.title}
            </h2>
            <button
              type="button"
              class="text-gray-400 hover:text-gray-100 text-xl leading-none"
              onClick={props.onClose}
              aria-label="Close"
            >
              ×
            </button>
          </div>
          {props.children}
        </div>
      </div>
    </Show>
  );
}
