import { createEffect, onCleanup, Show, type JSX } from "solid-js";

export default function Modal(props: {
  open: boolean;
  onClose: () => void;
  title: string;
  children: JSX.Element;
}) {
  createEffect(() => {
    if (!props.open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") props.onClose();
    };
    window.addEventListener("keydown", handler);
    onCleanup(() => window.removeEventListener("keydown", handler));
  });

  return (
    <Show when={props.open}>
      <div
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
        onClick={(e) => {
          if (e.currentTarget === e.target) props.onClose();
        }}
      >
        <div class="bg-gray-800 text-gray-100 rounded-lg p-6 w-96 shadow-xl">
          <div class="flex items-center justify-between mb-4">
            <h2 class="text-lg font-semibold">{props.title}</h2>
            <button
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
