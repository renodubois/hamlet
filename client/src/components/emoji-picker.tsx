import {
  useEffect,
  useId,
  useRef,
  type ChangeEvent,
  type CSSProperties,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";
import { createPortal, flushSync } from "react-dom";
import { useComputedValue, useSignalState } from "../hooks/react-state";
import { getServerUrl } from "../api";
import { CONSERVATIVE_EMOJIS, type EmojiEntry } from "../emoji/emoji-data";
import { searchEmojis } from "../emoji/emoji-search";
import { Input } from "./ui/input";

const PICKER_WIDTH = 320;
const PICKER_MAX_HEIGHT = 360;
const VIEWPORT_MARGIN = 8;
const ANCHOR_GAP = 8;
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

function emojiEntryKey(entry: EmojiEntry): string {
  return entry.kind === "custom" ? `custom:${entry.id}` : `native:${entry.emoji}`;
}

function emojiGridcellId(gridId: string, entry: EmojiEntry): string {
  return `${gridId}-${emojiEntryKey(entry)}`;
}

function shortcodeKey(shortcodes: readonly string[], index: number): string {
  const shortcode = shortcodes[index];
  const duplicateNumber = shortcodes.slice(0, index).filter((value) => value === shortcode).length;
  return duplicateNumber === 0 ? shortcode : `${shortcode}:${duplicateNumber + 1}`;
}

function isArrowKey(key: string): boolean {
  return key === "ArrowLeft" || key === "ArrowRight" || key === "ArrowUp" || key === "ArrowDown";
}

function isModifiedKeyboardEvent(event: ReactKeyboardEvent<HTMLElement>): boolean {
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
  const resultsGridId = useId();
  const panelRef = useRef<HTMLDivElement | null>(null);
  const searchRef = useRef<HTMLInputElement | null>(null);
  const gridScrollRef = useRef<HTMLDivElement | null>(null);
  const restoreFocusOnClose = useRef(true);
  const [query, setQuery] = useSignalState("");
  const [activeIndex, setActiveIndex] = useSignalState(-1);
  const [panelStyle, setPanelStyle] = useSignalState<CSSProperties>({
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
    return entry ? emojiGridcellId(resultsGridId, entry) : undefined;
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

    document
      .getElementById(emojiGridcellId(resultsGridId, entry))
      ?.scrollIntoView?.({ block: "nearest" });
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

  const handlePanelKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    const activeElement = document.activeElement;
    if (!activeElement || !panelRef.current?.contains(activeElement)) return;

    if (event.key === "Tab") {
      restoreFocusOnClose.current = false;
      props.onClose();
      return;
    }

    const isComposing = event.nativeEvent.isComposing;
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
    setActiveIndex(searchEmojis("", emojis()).length > 0 ? 0 : -1);
    const previouslyFocused = document.activeElement as HTMLElement | null;
    queueMicrotask(() => {
      if (gridScrollRef.current) gridScrollRef.current.scrollTop = 0;
      updatePosition();
      searchRef.current?.focus();
    });

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      props.onClose();
    };
    const handleMouseDown = (event: MouseEvent) => {
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

  if (!props.open) return null;

  const active = activeEntry();

  return createPortal(
    <div
      ref={(el) => {
        panelRef.current = el;
      }}
      role="dialog"
      aria-label="Emoji picker"
      className="fixed z-50 flex max-h-[360px] flex-col overflow-hidden rounded-lg border border-border bg-popover text-popover-foreground shadow-md"
      style={panelStyle()}
      onKeyDown={handlePanelKeyDown}
    >
      <div className="flex-shrink-0 border-b border-border p-3">
        <Input
          ref={(el: HTMLInputElement | null) => {
            searchRef.current = el;
          }}
          role="combobox"
          aria-label="Search and select emoji"
          aria-expanded="true"
          aria-haspopup="grid"
          aria-controls={resultsGridId}
          aria-activedescendant={activeGridcellId()}
          placeholder="Search emojis"
          spellCheck="false"
          value={query()}
          onChange={(event: ChangeEvent<HTMLInputElement>) => setQuery(event.currentTarget.value)}
        />
      </div>
      <div
        ref={(el) => {
          gridScrollRef.current = el;
        }}
        className="max-h-[224px] overflow-y-auto p-3"
      >
        <div id={resultsGridId} className="space-y-1" role="grid" aria-label="Emoji results">
          {filtered().length > 0 ? (
            rows().map((row, rowIndex) => (
              <div key={row.map(emojiEntryKey).join("|")} className="flex gap-1" role="row">
                {row.map((entry, cellIndex) => {
                  const index = rowIndex * EMOJI_GRID_COLUMNS + cellIndex;
                  const isActive = activeIndex() === index;
                  const label = emojiLabel(entry);
                  const imageUrl = isCustomEmoji(entry) ? entry.imageUrl : undefined;

                  return (
                    <div
                      key={emojiEntryKey(entry)}
                      id={emojiGridcellId(resultsGridId, entry)}
                      role="gridcell"
                      aria-selected={isActive ? "true" : "false"}
                      aria-label={label}
                      onMouseEnter={() => setActiveIndex(index)}
                    >
                      <button
                        type="button"
                        tabIndex={-1}
                        className={`relative flex h-8 w-8 cursor-pointer items-center justify-center rounded-md text-xl leading-none transition-colors focus:outline-none ${
                          isActive
                            ? "bg-accent text-accent-foreground shadow-inner ring-2 ring-ring"
                            : "hover:bg-accent focus:bg-accent focus-visible:ring-2 focus-visible:ring-ring"
                        }`}
                        aria-label={label}
                        title={entry.shortcodes.join(" ")}
                        onMouseDown={(event) => event.preventDefault()}
                        onClick={() => selectEntry(entry)}
                      >
                        {imageUrl ? (
                          <img
                            src={resolveImageUrl(imageUrl)}
                            alt=""
                            className="h-6 w-6 object-contain"
                            aria-hidden="true"
                          />
                        ) : (
                          <span aria-hidden="true">{entry.emoji}</span>
                        )}
                        {isCustomEmoji(entry) && entry.animated ? (
                          <>
                            <span
                              className="absolute bottom-0 right-0 rounded bg-purple-700 px-0.5 text-[8px] font-bold uppercase leading-3 text-white"
                              aria-hidden="true"
                              title="Animated custom emoji"
                            >
                              A
                            </span>
                            <span className="sr-only">Animated custom emoji</span>
                          </>
                        ) : null}
                      </button>
                    </div>
                  );
                })}
              </div>
            ))
          ) : (
            <div role="row">
              <div role="gridcell" className="px-2 py-6 text-center text-sm text-muted-foreground">
                No emojis found.
              </div>
            </div>
          )}
        </div>
      </div>
      {active ? (
        <div
          className="flex-shrink-0 border-t border-border bg-muted px-3 py-2"
          role="group"
          aria-label="Emoji shortcodes"
        >
          <div className="flex items-start gap-3">
            {isCustomEmoji(active) && active.imageUrl ? (
              <img
                src={resolveImageUrl(active.imageUrl)}
                alt=""
                className="h-8 w-8 object-contain"
                aria-hidden="true"
              />
            ) : (
              <span className="text-2xl leading-none" aria-hidden="true">
                {active.emoji}
              </span>
            )}
            <div className="flex flex-1 flex-wrap gap-1 text-xs text-muted-foreground">
              {isCustomEmoji(active) && active.animated ? (
                <span className="rounded bg-purple-700 px-1.5 py-0.5 text-white">animated</span>
              ) : null}
              {active.shortcodes.map((shortcode, index) => (
                <code
                  key={shortcodeKey(active.shortcodes, index)}
                  className="rounded bg-primary/10 px-1.5 py-0.5 text-primary"
                >
                  {shortcode}
                </code>
              ))}
            </div>
          </div>
        </div>
      ) : null}
    </div>,
    document.body,
  );
}
