import { Show, createSignal, onCleanup, onMount } from "solid-js";
import type { UserTyping } from "../api";
import type { EventsContextValue } from "../events_context";

// How long after a user's last typing ping the indicator keeps showing them.
// Must be longer than TYPING_PING_INTERVAL_MS in channel.tsx or the indicator
// will flicker between pings.
export const TYPING_EXPIRY_MS = 3500;

// How often we prune expired entries.
const TYPING_SWEEP_MS = 500;

// Above this count, we collapse to "Several people are typing..." instead of
// listing names.
export const TYPING_LIST_LIMIT = 3;

export function formatTypingMessage(usernames: string[]): string | null {
  if (usernames.length === 0) return null;
  if (usernames.length > TYPING_LIST_LIMIT) return "Several people are typing…";
  if (usernames.length === 1) return `${usernames[0]} is typing…`;
  if (usernames.length === 2) return `${usernames[0]} and ${usernames[1]} are typing…`;
  return `${usernames[0]}, ${usernames[1]}, and ${usernames[2]} are typing…`;
}

interface Props {
  channelId: number;
  currentUserId: number | null;
  events: EventsContextValue;
  now?: () => number;
}

interface TypingEntry {
  username: string;
  lastSeen: number;
}

export default function TypingIndicator(props: Props) {
  const now = () => (props.now ?? Date.now)();
  const [entries, setEntries] = createSignal<Record<number, TypingEntry>>({});

  const prune = () => {
    const cutoff = now() - TYPING_EXPIRY_MS;
    setEntries((prev) => {
      let changed = false;
      const next: Record<number, TypingEntry> = {};
      for (const [key, entry] of Object.entries(prev)) {
        if (entry.lastSeen >= cutoff) {
          next[Number(key)] = entry;
        } else {
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  };

  onMount(() => {
    const unsub = props.events.onUserTyping((t: UserTyping) => {
      if (t.channel_id !== props.channelId) return;
      if (props.currentUserId !== null && t.user_id === props.currentUserId) return;
      setEntries((prev) => ({
        ...prev,
        [t.user_id]: { username: t.username, lastSeen: now() },
      }));
    });
    const timer = window.setInterval(prune, TYPING_SWEEP_MS);
    onCleanup(() => {
      unsub();
      window.clearInterval(timer);
    });
  });

  const usernames = () =>
    Object.values(entries())
      .map((e) => e.username)
      .sort((a, b) => a.localeCompare(b));

  const message = () => formatTypingMessage(usernames());

  return (
    <Show when={message()} keyed>
      {(text) => (
        <div
          class="flex items-center gap-2 text-xs text-gray-500 px-1 pb-1 h-5"
          aria-live="polite"
          data-testid="typing-indicator"
        >
          <span class="inline-flex items-center gap-0.5" aria-hidden="true">
            <span
              class="inline-block w-1 h-1 rounded-full bg-gray-500 animate-bounce"
              style={{ "animation-delay": "0ms" }}
            />
            <span
              class="inline-block w-1 h-1 rounded-full bg-gray-500 animate-bounce"
              style={{ "animation-delay": "150ms" }}
            />
            <span
              class="inline-block w-1 h-1 rounded-full bg-gray-500 animate-bounce"
              style={{ "animation-delay": "300ms" }}
            />
          </span>
          <span>{text}</span>
        </div>
      )}
    </Show>
  );
}
