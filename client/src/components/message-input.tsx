import { createEffect, createSignal, untrack, type JSX } from "solid-js";
import { replaceCompletedEmojiShortcodeBeforeCaret } from "../emoji/emoji-shortcodes";
import EmojiPicker from "./emoji-picker";
import { EmojiIcon } from "./icons";

interface SelectionRange {
  start: number;
  end: number;
}

export interface MessageInputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  ariaLabel?: string;
  class?: string;
  inputClass?: string;
  emojiButtonClass?: string;
  emojiButtonLabel?: string;
  inputRef?: (element: HTMLInputElement) => void;
  onKeyDown?: JSX.EventHandler<HTMLInputElement, KeyboardEvent>;
}

const DEFAULT_ROOT_CLASS = "flex min-w-0 flex-1 items-center gap-2";
const DEFAULT_INPUT_CLASS = "bg-gray-100 rounded-md p-4 w-full";
const DEFAULT_EMOJI_BUTTON_CLASS =
  "cursor-pointer rounded-md bg-gray-100 p-4 text-gray-700 hover:bg-gray-200 focus:outline-none focus:ring-2 focus:ring-blue-400";

function clampIndex(index: number, value: string): number {
  return Math.min(Math.max(index, 0), value.length);
}

function normalizeSelection(selection: SelectionRange, value: string): SelectionRange {
  const start = clampIndex(selection.start, value);
  const end = clampIndex(selection.end, value);

  return start <= end ? { start, end } : { start: end, end: start };
}

export default function MessageInput(props: MessageInputProps) {
  const [emojiPickerOpen, setEmojiPickerOpen] = createSignal(false);
  const [selection, setSelection] = createSignal<SelectionRange>({
    start: props.value.length,
    end: props.value.length,
  });
  let inputRef: HTMLInputElement | undefined;
  let emojiButtonRef: HTMLButtonElement | undefined;
  let previousValue = props.value;

  const readInputSelection = (): SelectionRange => {
    if (!inputRef) return normalizeSelection(selection(), props.value);

    const start = inputRef.selectionStart ?? props.value.length;
    const end = inputRef.selectionEnd ?? start;
    return normalizeSelection({ start, end }, props.value);
  };

  const rememberSelection = () => {
    setSelection(readInputSelection());
  };

  const setInputSelection = (nextSelection: SelectionRange, focusInput = false) => {
    const normalized = normalizeSelection(nextSelection, props.value);
    setSelection(normalized);

    queueMicrotask(() => {
      if (!inputRef) return;
      if (focusInput) inputRef.focus();
      inputRef.setSelectionRange(normalized.start, normalized.end);
    });
  };

  const updateValue = (
    value: string,
    nextSelection: SelectionRange,
    options: { focusInput?: boolean; restoreSelection?: boolean } = {},
  ) => {
    props.onChange(value);
    const normalized = normalizeSelection(nextSelection, value);
    setSelection(normalized);

    if (!options.focusInput && !options.restoreSelection) return;

    queueMicrotask(() => {
      if (!inputRef) return;
      if (options.focusInput) inputRef.focus();
      inputRef.setSelectionRange(normalized.start, normalized.end);
    });
  };

  const handleInput = (event: InputEvent & { currentTarget: HTMLInputElement }) => {
    const rawValue = event.currentTarget.value;
    const caretIndex = event.currentTarget.selectionStart ?? rawValue.length;
    const next = replaceCompletedEmojiShortcodeBeforeCaret(rawValue, caretIndex);
    const selectionEnd = next.replaced
      ? next.caretIndex
      : (event.currentTarget.selectionEnd ?? caretIndex);

    updateValue(
      next.value,
      { start: next.caretIndex, end: selectionEnd },
      { restoreSelection: next.replaced },
    );
  };

  const handleEmojiSelect = (emoji: string) => {
    const currentSelection = normalizeSelection(selection(), props.value);
    const nextValue = `${props.value.slice(0, currentSelection.start)}${emoji}${props.value.slice(
      currentSelection.end,
    )}`;
    const caretIndex = currentSelection.start + emoji.length;

    updateValue(nextValue, { start: caretIndex, end: caretIndex }, { focusInput: true });
  };

  createEffect(() => {
    const value = props.value;
    const currentSelection = untrack(selection);
    const normalizedSelection = normalizeSelection(currentSelection, value);

    if (
      currentSelection.start !== normalizedSelection.start ||
      currentSelection.end !== normalizedSelection.end
    ) {
      setSelection(normalizedSelection);
    }

    if (previousValue.length > 0 && value.length === 0) {
      setEmojiPickerOpen(false);
      setInputSelection({ start: 0, end: 0 });
    }

    previousValue = value;
  });

  return (
    <div class={props.class ?? DEFAULT_ROOT_CLASS}>
      <input
        ref={(el) => {
          inputRef = el;
          props.inputRef?.(el);
        }}
        class={props.inputClass ?? DEFAULT_INPUT_CLASS}
        aria-label={props.ariaLabel ?? "Message input"}
        autocorrect="off"
        value={props.value}
        onInput={handleInput}
        onKeyDown={props.onKeyDown}
        onSelect={rememberSelection}
        onKeyUp={rememberSelection}
        onMouseUp={rememberSelection}
        onClick={rememberSelection}
        onFocus={rememberSelection}
        onBlur={rememberSelection}
        placeholder={props.placeholder}
      />
      <button
        ref={(el) => {
          emojiButtonRef = el;
        }}
        type="button"
        class={props.emojiButtonClass ?? DEFAULT_EMOJI_BUTTON_CLASS}
        aria-label={props.emojiButtonLabel ?? "Open emoji picker"}
        aria-haspopup="dialog"
        aria-expanded={emojiPickerOpen()}
        title="Emoji"
        onMouseDown={(event) => {
          event.preventDefault();
          rememberSelection();
        }}
        onClick={() => {
          rememberSelection();
          setEmojiPickerOpen((open) => !open);
        }}
      >
        <EmojiIcon size={20} aria-hidden="true" />
      </button>
      <EmojiPicker
        open={emojiPickerOpen()}
        anchor={() => emojiButtonRef}
        onSelect={handleEmojiSelect}
        onClose={() => setEmojiPickerOpen(false)}
      />
    </div>
  );
}
