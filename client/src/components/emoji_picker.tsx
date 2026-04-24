import { createEffect, createMemo, createSignal, For, Show, onCleanup, type JSX } from "solid-js";
import { Portal } from "solid-js/web";
import { CONSERVATIVE_EMOJIS, type EmojiEntry } from "../emoji/emoji_data";
import { searchEmojis } from "../emoji/emoji_search";

const PICKER_WIDTH = 320;
const PICKER_MAX_HEIGHT = 360;
const VIEWPORT_MARGIN = 8;
const ANCHOR_GAP = 8;

function pickerPosition(anchor: HTMLElement | undefined, panel: HTMLElement | undefined) {
  const width = PICKER_WIDTH;
  const height = panel?.offsetHeight || PICKER_MAX_HEIGHT;
  const viewportWidth = window.innerWidth || width + VIEWPORT_MARGIN * 2;
  const viewportHeight = window.innerHeight || height + VIEWPORT_MARGIN * 2;

  if (!anchor) {
    return {
      left: VIEWPORT_MARGIN,
      top: VIEWPORT_MARGIN,
      width,
    };
  }

  const rect = anchor.getBoundingClientRect();
  const spaceAbove = rect.top - VIEWPORT_MARGIN;
  const preferredTop = rect.top - height - ANCHOR_GAP;
  const fallbackTop = rect.bottom + ANCHOR_GAP;
  const top =
    spaceAbove >= height + ANCHOR_GAP
      ? Math.max(VIEWPORT_MARGIN, preferredTop)
      : Math.min(fallbackTop, viewportHeight - height - VIEWPORT_MARGIN);
  const left = Math.max(
    VIEWPORT_MARGIN,
    Math.min(rect.left, viewportWidth - width - VIEWPORT_MARGIN),
  );

  return { left, top: Math.max(VIEWPORT_MARGIN, top), width };
}

export default function EmojiPicker(props: {
  open: boolean;
  anchor: () => HTMLElement | undefined;
  emojis?: readonly EmojiEntry[];
  onSelect: (emoji: string) => void;
  onClose: () => void;
}) {
  let panelRef: HTMLDivElement | undefined;
  let searchRef: HTMLInputElement | undefined;
  const [query, setQuery] = createSignal("");
  const [panelStyle, setPanelStyle] = createSignal<JSX.CSSProperties>({
    left: `${VIEWPORT_MARGIN}px`,
    top: `${VIEWPORT_MARGIN}px`,
    width: `${PICKER_WIDTH}px`,
  });

  const emojis = () => props.emojis ?? CONSERVATIVE_EMOJIS;
  const filtered = createMemo(() => searchEmojis(query(), emojis()));

  const updatePosition = () => {
    const position = pickerPosition(props.anchor(), panelRef);
    setPanelStyle({
      left: `${position.left}px`,
      top: `${position.top}px`,
      width: `${position.width}px`,
    });
  };

  createEffect(() => {
    if (!props.open) return;

    setQuery("");
    const previouslyFocused = document.activeElement as HTMLElement | null;
    queueMicrotask(() => {
      updatePosition();
      searchRef?.focus();
    });

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      props.onClose();
    };
    const handleMouseDown = (e: MouseEvent) => {
      const target = e.target as Node | null;
      if (!target) return;
      if (panelRef?.contains(target)) return;
      if (props.anchor()?.contains(target)) return;
      props.onClose();
    };

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    document.addEventListener("mousedown", handleMouseDown);

    onCleanup(() => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
      document.removeEventListener("mousedown", handleMouseDown);
      if (panelRef?.contains(document.activeElement)) previouslyFocused?.focus?.();
    });
  });

  const selectEmoji = (emoji: string) => {
    props.onSelect(emoji);
    props.onClose();
  };

  return (
    <Show when={props.open}>
      <Portal>
        <div
          ref={(el) => {
            panelRef = el;
          }}
          role="dialog"
          aria-label="Emoji picker"
          class="fixed z-50 max-h-[360px] overflow-hidden rounded-lg border border-gray-700 bg-gray-800 text-gray-100 shadow-xl"
          style={panelStyle()}
        >
          <div class="border-b border-gray-700 p-3">
            <input
              ref={(el) => {
                searchRef = el;
              }}
              aria-label="Search emojis"
              class="w-full rounded-md border border-gray-700 bg-gray-900 px-3 py-2 text-sm text-gray-100 placeholder:text-gray-500 outline-none focus:border-blue-400 focus:ring-2 focus:ring-blue-400/40"
              placeholder="Search emojis"
              value={query()}
              onInput={(e) => setQuery(e.currentTarget.value)}
            />
          </div>
          <div class="max-h-[300px] overflow-y-auto p-3">
            <Show
              when={filtered().length > 0}
              fallback={<p class="px-2 py-6 text-center text-sm text-gray-400">No emojis found.</p>}
            >
              <div class="grid grid-cols-8 gap-1" role="group" aria-label="Emoji results">
                <For each={filtered()}>
                  {(entry) => (
                    <button
                      type="button"
                      class="flex h-8 w-8 items-center justify-center rounded-md text-xl leading-none hover:bg-gray-700 focus:bg-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-400"
                      aria-label={`${entry.name} emoji`}
                      title={`${entry.name} ${entry.shortcodes[0] ?? ""}`.trim()}
                      onClick={() => selectEmoji(entry.emoji)}
                    >
                      {entry.emoji}
                    </button>
                  )}
                </For>
              </div>
            </Show>
          </div>
        </div>
      </Portal>
    </Show>
  );
}
