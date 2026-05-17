import {
  createEffect,
  createMemo,
  createSignal,
  For,
  Show,
  onCleanup,
  untrack,
  type JSX,
} from "solid-js";
import { Portal } from "solid-js/web";
import { CONSERVATIVE_EMOJIS, type EmojiEntry } from "../emoji/emoji-data";
import { searchEmojis } from "../emoji/emoji-search";

const PICKER_WIDTH = 320;
const PICKER_MAX_HEIGHT = 360;
const VIEWPORT_MARGIN = 8;
const ANCHOR_GAP = 8;
const EMOJI_RESULTS_GRID_ID = "emoji-results-grid";

export const EMOJI_GRID_COLUMNS = 8;

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

function chunkEmojis(emojis: readonly EmojiEntry[]): EmojiEntry[][] {
  const rows: EmojiEntry[][] = [];

  for (let index = 0; index < emojis.length; index += EMOJI_GRID_COLUMNS) {
    rows.push(emojis.slice(index, index + EMOJI_GRID_COLUMNS));
  }

  return rows;
}

function emojiLabel(entry: EmojiEntry): string {
  const shortcodes = entry.shortcodes.length > 0 ? entry.shortcodes.join(", ") : entry.emoji;
  return `Emoji ${shortcodes}`;
}

function emojiGridcellId(entry: EmojiEntry): string {
  const shortcodeKey = entry.shortcodes
    .join("-")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  const emojiKey = Array.from(entry.emoji)
    .map((char) => char.codePointAt(0)?.toString(16) ?? "emoji")
    .join("-");

  return `emoji-gridcell-${shortcodeKey || emojiKey}`;
}

function isArrowKey(key: string): boolean {
  return key === "ArrowLeft" || key === "ArrowRight" || key === "ArrowUp" || key === "ArrowDown";
}

function isModifiedKeyboardEvent(event: KeyboardEvent): boolean {
  return event.shiftKey || event.ctrlKey || event.metaKey || event.altKey;
}

function clampIndex(index: number, maxIndex: number): number {
  return Math.max(0, Math.min(index, maxIndex));
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
  let gridScrollRef: HTMLDivElement | undefined;
  let restoreFocusOnClose = true;
  const [query, setQuery] = createSignal("");
  const [activeIndex, setActiveIndex] = createSignal(-1);
  const [panelStyle, setPanelStyle] = createSignal<JSX.CSSProperties>({
    left: `${VIEWPORT_MARGIN}px`,
    top: `${VIEWPORT_MARGIN}px`,
    width: `${PICKER_WIDTH}px`,
  });

  const emojis = () => props.emojis ?? CONSERVATIVE_EMOJIS;
  const filtered = createMemo(() => searchEmojis(query(), emojis()));
  const rows = createMemo(() => chunkEmojis(filtered()));
  const activeEntry = createMemo(() => {
    const entry = filtered()[activeIndex()];
    return entry;
  });
  const activeGridcellId = createMemo(() => {
    const entry = activeEntry();
    return entry ? emojiGridcellId(entry) : undefined;
  });

  const updatePosition = () => {
    const position = pickerPosition(props.anchor(), panelRef);
    setPanelStyle({
      left: `${position.left}px`,
      top: `${position.top}px`,
      width: `${position.width}px`,
    });
  };

  const scrollActiveEmojiIntoView = (index: number) => {
    const entry = filtered()[index];
    if (!entry) return;

    document.getElementById(emojiGridcellId(entry))?.scrollIntoView?.({ block: "nearest" });
  };

  const moveActiveIndex = (delta: number) => {
    const resultCount = filtered().length;
    if (resultCount === 0) return;

    const currentIndex = activeIndex() >= 0 ? activeIndex() : 0;
    const nextIndex = clampIndex(currentIndex + delta, resultCount - 1);
    setActiveIndex(nextIndex);
    queueMicrotask(() => scrollActiveEmojiIntoView(nextIndex));
  };

  const selectEntry = (entry: EmojiEntry) => {
    props.onSelect(entry.emoji);
    props.onClose();
  };

  const handlePanelKeyDown = (event: KeyboardEvent) => {
    const activeElement = document.activeElement;
    if (!activeElement || !panelRef?.contains(activeElement)) return;

    if (event.key === "Tab") {
      restoreFocusOnClose = false;
      props.onClose();
      return;
    }

    if (event.isComposing) return;

    if (event.key === "Enter") {
      event.preventDefault();
      const entry = activeEntry();
      if (!entry) return;

      event.stopPropagation();
      selectEntry(entry);
      return;
    }

    if (!isArrowKey(event.key)) return;
    if (event.isComposing || isModifiedKeyboardEvent(event)) return;
    if (filtered().length === 0) return;

    event.preventDefault();
    event.stopPropagation();

    if (event.key === "ArrowLeft") {
      moveActiveIndex(-1);
    } else if (event.key === "ArrowRight") {
      moveActiveIndex(1);
    } else if (event.key === "ArrowUp") {
      moveActiveIndex(-EMOJI_GRID_COLUMNS);
    } else {
      moveActiveIndex(EMOJI_GRID_COLUMNS);
    }
  };

  createEffect(() => {
    if (!props.open) return;

    restoreFocusOnClose = true;
    setQuery("");
    setActiveIndex(untrack(() => (searchEmojis("", emojis()).length > 0 ? 0 : -1)));
    const previouslyFocused = document.activeElement as HTMLElement | null;
    queueMicrotask(() => {
      if (gridScrollRef) gridScrollRef.scrollTop = 0;
      updatePosition();
      searchRef?.focus();
    });

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      props.onClose();
    };
    const handleMouseDown = (event: MouseEvent) => {
      const target = event.target as Node | null;
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
      if (restoreFocusOnClose && panelRef?.contains(document.activeElement)) {
        previouslyFocused?.focus?.();
      }
    });
  });

  createEffect(() => {
    const resultCount = filtered().length;
    setActiveIndex(resultCount > 0 ? 0 : -1);

    if (props.open) {
      queueMicrotask(updatePosition);
    }
  });

  return (
    <Show when={props.open}>
      <Portal>
        <div
          ref={(el) => {
            panelRef = el;
          }}
          role="dialog"
          aria-label="Emoji picker"
          class="fixed z-50 flex max-h-[360px] flex-col overflow-hidden rounded-lg border border-gray-700 bg-gray-800 text-gray-100 shadow-xl"
          style={panelStyle()}
          onKeyDown={handlePanelKeyDown}
        >
          <div class="flex-shrink-0 border-b border-gray-700 p-3">
            <input
              ref={(el) => {
                searchRef = el;
              }}
              role="combobox"
              aria-label="Search and select emoji"
              aria-expanded="true"
              aria-haspopup="grid"
              aria-controls={EMOJI_RESULTS_GRID_ID}
              aria-activedescendant={activeGridcellId()}
              class="w-full rounded-md border border-gray-700 bg-gray-900 px-3 py-2 text-sm text-gray-100 placeholder:text-gray-500 outline-none focus:border-blue-400 focus:ring-2 focus:ring-blue-400/40"
              placeholder="Search emojis"
              spellcheck="false"
              value={query()}
              onInput={(event) => setQuery(event.currentTarget.value)}
            />
          </div>
          <div
            ref={(el) => {
              gridScrollRef = el;
            }}
            class="max-h-[224px] overflow-y-auto p-3"
          >
            <div
              id={EMOJI_RESULTS_GRID_ID}
              class="space-y-1"
              role="grid"
              aria-label="Emoji results"
            >
              <Show
                when={filtered().length > 0}
                fallback={
                  <div role="row">
                    <div role="gridcell" class="px-2 py-6 text-center text-sm text-gray-400">
                      No emojis found.
                    </div>
                  </div>
                }
              >
                <For each={rows()}>
                  {(row, rowIndex) => (
                    <div class="flex gap-1" role="row">
                      <For each={row}>
                        {(entry, cellIndex) => {
                          const index = () => rowIndex() * EMOJI_GRID_COLUMNS + cellIndex();
                          const isActive = () => activeIndex() === index();
                          const label = () => emojiLabel(entry);

                          return (
                            <div
                              id={emojiGridcellId(entry)}
                              role="gridcell"
                              aria-selected={isActive() ? "true" : "false"}
                              aria-label={label()}
                              onMouseEnter={() => setActiveIndex(index())}
                            >
                              <button
                                type="button"
                                tabIndex={-1}
                                class={`flex h-8 w-8 cursor-pointer items-center justify-center rounded-md text-xl leading-none focus:outline-none ${
                                  isActive()
                                    ? "bg-blue-600 text-white shadow-inner ring-2 ring-blue-300"
                                    : "text-gray-100 hover:bg-gray-700 focus:bg-gray-700 focus:ring-2 focus:ring-blue-400"
                                }`}
                                aria-label={label()}
                                title={entry.shortcodes.join(" ")}
                                onMouseDown={(event) => event.preventDefault()}
                                onClick={() => selectEntry(entry)}
                              >
                                <span aria-hidden="true">{entry.emoji}</span>
                              </button>
                            </div>
                          );
                        }}
                      </For>
                    </div>
                  )}
                </For>
              </Show>
            </div>
          </div>
          <Show when={activeEntry()} keyed>
            {(entry) => (
              <div
                class="flex-shrink-0 border-t border-gray-700 bg-gray-900 px-3 py-2"
                role="group"
                aria-label="Emoji shortcodes"
              >
                <div class="flex items-start gap-3">
                  <span class="text-2xl leading-none" aria-hidden="true">
                    {entry.emoji}
                  </span>
                  <div class="flex flex-1 flex-wrap gap-1 text-xs text-gray-200">
                    <For each={entry.shortcodes}>
                      {(shortcode) => (
                        <code class="rounded bg-gray-800 px-1.5 py-0.5 text-blue-100">
                          {shortcode}
                        </code>
                      )}
                    </For>
                  </div>
                </div>
              </div>
            )}
          </Show>
        </div>
      </Portal>
    </Show>
  );
}
