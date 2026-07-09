import { useEffect, useRef } from "react";
import { flushSync } from "react-dom";
import {
  useComputedValue,
  useSignalState,
  List,
  If,
  ignoreReactiveTracking,
  PortalRoot,
  type JSX,
} from "../hooks/react-state";
import { getServerUrl } from "../api";
import { CONSERVATIVE_EMOJIS, type EmojiEntry } from "../emoji/emoji-data";
import { searchEmojis } from "../emoji/emoji-search";

const PICKER_WIDTH = 320;
const PICKER_MAX_HEIGHT = 360;
const VIEWPORT_MARGIN = 8;
const ANCHOR_GAP = 8;
const EMOJI_RESULTS_GRID_ID = "emoji-results-grid";

export const EMOJI_GRID_COLUMNS = 8;

function pickerPosition(
  anchor: HTMLElement | null | undefined,
  panel: HTMLElement | null | undefined,
) {
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
  return `${entry.animated ? "Animated emoji" : "Emoji"} ${shortcodes}`;
}

function resolveImageUrl(url: string): string {
  if (url.startsWith("http://") || url.startsWith("https://")) return url;
  return `${getServerUrl()}${url}`;
}

function isCustomEmoji(entry: EmojiEntry): boolean {
  return entry.kind === "custom" && !!entry.imageUrl;
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

function isModifiedKeyboardEvent(event: any): boolean {
  return event.shiftKey || event.ctrlKey || event.metaKey || event.altKey;
}

function clampIndex(index: number, maxIndex: number): number {
  return Math.max(0, Math.min(index, maxIndex));
}

export default function EmojiPicker(props: {
  open: boolean;
  anchor: () => HTMLElement | null | undefined;
  emojis?: readonly EmojiEntry[];
  onSelect: (emoji: string) => void;
  onClose: () => void;
}) {
  const panelRef = useRef<HTMLDivElement | null>(null);
  const searchRef = useRef<HTMLInputElement | null>(null);
  const gridScrollRef = useRef<HTMLDivElement | null>(null);
  const restoreFocusOnClose = useRef(true);
  const [query, setQuery] = useSignalState("");
  const [activeIndex, setActiveIndex] = useSignalState(-1);
  const [panelStyle, setPanelStyle] = useSignalState<JSX.CSSProperties>({
    left: `${VIEWPORT_MARGIN}px`,
    top: `${VIEWPORT_MARGIN}px`,
    width: `${PICKER_WIDTH}px`,
  });

  const emojis = () => props.emojis ?? CONSERVATIVE_EMOJIS;
  const filtered = useComputedValue(() => searchEmojis(query(), emojis()));
  const rows = useComputedValue(() => chunkEmojis(filtered()));
  const activeEntry = useComputedValue(() => {
    const entry = filtered()[activeIndex()];
    return entry;
  });
  const activeGridcellId = useComputedValue(() => {
    const entry = activeEntry();
    return entry ? emojiGridcellId(entry) : undefined;
  });

  const updatePosition = () => {
    const position = pickerPosition(props.anchor(), panelRef.current);
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
    flushSync(() => setActiveIndex(nextIndex));
    queueMicrotask(() => scrollActiveEmojiIntoView(nextIndex));
  };

  const selectEntry = (entry: EmojiEntry) => {
    props.onSelect(entry.emoji);
    props.onClose();
  };

  const handlePanelKeyDown = (event: any) => {
    const activeElement = document.activeElement;
    if (!activeElement || !panelRef.current?.contains(activeElement)) return;

    if (event.key === "Tab") {
      restoreFocusOnClose.current = false;
      props.onClose();
      return;
    }

    const isComposing = Boolean(event.isComposing || event.nativeEvent?.isComposing);
    if (isComposing) return;

    if (event.key === "Enter") {
      event.preventDefault();
      const entry = activeEntry();
      if (!entry) return;

      event.stopPropagation();
      selectEntry(entry);
      return;
    }

    if (!isArrowKey(event.key)) return;
    if (isComposing || isModifiedKeyboardEvent(event)) return;
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

  useEffect(() => {
    if (!props.open) return;

    restoreFocusOnClose.current = true;
    setQuery("");
    setActiveIndex(ignoreReactiveTracking(() => (searchEmojis("", emojis()).length > 0 ? 0 : -1)));
    const previouslyFocused = document.activeElement as HTMLElement | null;
    queueMicrotask(() => {
      if (gridScrollRef.current) gridScrollRef.current.scrollTop = 0;
      updatePosition();
      searchRef.current?.focus();
    });

    const handleKeyDown = (event: any) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      props.onClose();
    };
    const handleMouseDown = (event: any) => {
      const target = event.target as Node | null;
      if (!target) return;
      if (panelRef.current?.contains(target)) return;
      if (props.anchor()?.contains(target)) return;
      props.onClose();
    };

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    document.addEventListener("mousedown", handleMouseDown);

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
      document.removeEventListener("mousedown", handleMouseDown);
      if (restoreFocusOnClose.current && panelRef.current?.contains(document.activeElement)) {
        previouslyFocused?.focus?.();
      }
    };
  }, [props.open]);

  useEffect(() => {
    const resultCount = filtered().length;
    const nextActiveIndex = resultCount > 0 ? 0 : -1;
    if (activeIndex() !== nextActiveIndex) setActiveIndex(nextActiveIndex);

    if (props.open) {
      queueMicrotask(updatePosition);
    }
  }, [query(), props.emojis, props.open]);

  return (
    <If when={props.open}>
      <PortalRoot>
        <div
          ref={(el) => {
            panelRef.current = el;
          }}
          role="dialog"
          aria-label="Emoji picker"
          className="fixed z-50 flex max-h-[360px] flex-col overflow-hidden rounded-lg border border-gray-700 bg-gray-800 text-gray-100 shadow-xl"
          style={panelStyle()}
          onKeyDown={handlePanelKeyDown}
        >
          <div className="flex-shrink-0 border-b border-gray-700 p-3">
            <input
              ref={(el) => {
                searchRef.current = el;
              }}
              role="combobox"
              aria-label="Search and select emoji"
              aria-expanded="true"
              aria-haspopup="grid"
              aria-controls={EMOJI_RESULTS_GRID_ID}
              aria-activedescendant={activeGridcellId()}
              className="w-full rounded-md border border-gray-700 bg-gray-900 px-3 py-2 text-sm text-gray-100 placeholder:text-gray-500 outline-none focus:border-blue-400 focus:ring-2 focus:ring-blue-400/40"
              placeholder="Search emojis"
              spellCheck="false"
              value={query()}
              onInput={(event) => setQuery(event.currentTarget.value)}
            />
          </div>
          <div
            ref={(el) => {
              gridScrollRef.current = el;
            }}
            className="max-h-[224px] overflow-y-auto p-3"
          >
            <div
              id={EMOJI_RESULTS_GRID_ID}
              className="space-y-1"
              role="grid"
              aria-label="Emoji results"
            >
              <If
                when={filtered().length > 0}
                fallback={
                  <div role="row">
                    <div role="gridcell" className="px-2 py-6 text-center text-sm text-gray-400">
                      No emojis found.
                    </div>
                  </div>
                }
              >
                <List each={rows()}>
                  {(row, rowIndex) => (
                    <div className="flex gap-1" role="row">
                      <List each={row}>
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
                                className={`relative flex h-8 w-8 cursor-pointer items-center justify-center rounded-md text-xl leading-none focus:outline-none ${
                                  isActive()
                                    ? "bg-blue-600 text-white shadow-inner ring-2 ring-blue-300"
                                    : "text-gray-100 hover:bg-gray-700 focus:bg-gray-700 focus:ring-2 focus:ring-blue-400"
                                }`}
                                aria-label={label()}
                                title={entry.shortcodes.join(" ")}
                                onMouseDown={(event) => event.preventDefault()}
                                onClick={() => selectEntry(entry)}
                              >
                                <If
                                  when={isCustomEmoji(entry) && entry.imageUrl}
                                  fallback={<span aria-hidden="true">{entry.emoji}</span>}
                                >
                                  {(imageUrl) => (
                                    <img
                                      src={resolveImageUrl(imageUrl())}
                                      alt=""
                                      className="h-6 w-6 object-contain"
                                      aria-hidden="true"
                                    />
                                  )}
                                </If>
                                <If when={isCustomEmoji(entry) && entry.animated}>
                                  <span
                                    className="absolute bottom-0 right-0 rounded bg-purple-700 px-0.5 text-[8px] font-bold uppercase leading-3 text-white"
                                    aria-hidden="true"
                                    title="Animated custom emoji"
                                  >
                                    A
                                  </span>
                                  <span className="sr-only">Animated custom emoji</span>
                                </If>
                              </button>
                            </div>
                          );
                        }}
                      </List>
                    </div>
                  )}
                </List>
              </If>
            </div>
          </div>
          <If when={activeEntry()} keyed>
            {(entry) => (
              <div
                className="flex-shrink-0 border-t border-gray-700 bg-gray-900 px-3 py-2"
                role="group"
                aria-label="Emoji shortcodes"
              >
                <div className="flex items-start gap-3">
                  <If
                    when={isCustomEmoji(entry) && entry.imageUrl}
                    fallback={
                      <span className="text-2xl leading-none" aria-hidden="true">
                        {entry.emoji}
                      </span>
                    }
                  >
                    {(imageUrl) => (
                      <img
                        src={resolveImageUrl(imageUrl())}
                        alt=""
                        className="h-8 w-8 object-contain"
                        aria-hidden="true"
                      />
                    )}
                  </If>
                  <div className="flex flex-1 flex-wrap gap-1 text-xs text-gray-200">
                    <If when={isCustomEmoji(entry) && entry.animated}>
                      <span className="rounded bg-purple-700 px-1.5 py-0.5 text-white">
                        animated
                      </span>
                    </If>
                    <List each={entry.shortcodes}>
                      {(shortcode) => (
                        <code className="rounded bg-gray-800 px-1.5 py-0.5 text-blue-100">
                          {shortcode}
                        </code>
                      )}
                    </List>
                  </div>
                </div>
              </div>
            )}
          </If>
        </div>
      </PortalRoot>
    </If>
  );
}
