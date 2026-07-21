import { useLayoutEffect, useState } from "react";

import type { Message, UserTyping } from "../api";
import type { EventsContextValue } from "../contexts/events";
import { TYPING_EXPIRY_MS } from "../constants";

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

export default function TypingIndicator({ channelId, currentUserId, events, now }: Props) {
  const [entries, setEntries] = useState<Record<number, TypingEntry>>({});

  useLayoutEffect(() => {
    setEntries({});
    const currentTime = now ?? Date.now;

    const clearUser = (userId: number) => {
      setEntries((previous) => {
        if (!(userId in previous)) return previous;
        const next = { ...previous };
        delete next[userId];
        return next;
      });
    };

    const unsubscribeTyping = events.onUserTyping((typing: UserTyping) => {
      if (typing.channel_id !== channelId || typing.user_id === currentUserId) return;
      setEntries((previous) => ({
        ...previous,
        [typing.user_id]: { username: typing.username, lastSeen: currentTime() },
      }));
    });
    const unsubscribeMessages = events.onMessage((message: Message) => {
      if (message.channel_id !== channelId) return;
      clearUser(message.user_id);
    });
    const timer = window.setInterval(() => {
      const cutoff = currentTime() - TYPING_EXPIRY_MS;
      setEntries((previous) => {
        let changed = false;
        const next: Record<number, TypingEntry> = {};
        for (const [key, entry] of Object.entries(previous)) {
          if (entry.lastSeen >= cutoff) {
            next[Number(key)] = entry;
          } else {
            changed = true;
          }
        }
        return changed ? next : previous;
      });
    }, TYPING_SWEEP_MS);

    return () => {
      unsubscribeTyping();
      unsubscribeMessages();
      window.clearInterval(timer);
    };
  }, [channelId, currentUserId, events, now]);

  const usernames = Object.values(entries)
    .map((entry) => entry.username)
    .sort((a, b) => a.localeCompare(b));
  const message = formatTypingMessage(usernames);

  return message ? (
    <div
      className="flex items-center gap-2 text-xs text-muted-foreground px-1 pb-1 h-5"
      aria-live="polite"
      data-testid="typing-indicator"
    >
      <span className="inline-flex items-center gap-0.5" aria-hidden="true">
        <span
          className="inline-block w-1 h-1 rounded-full bg-muted-foreground animate-bounce"
          style={{ animationDelay: "0ms" }}
        />
        <span
          className="inline-block w-1 h-1 rounded-full bg-muted-foreground animate-bounce"
          style={{ animationDelay: "150ms" }}
        />
        <span
          className="inline-block w-1 h-1 rounded-full bg-muted-foreground animate-bounce"
          style={{ animationDelay: "300ms" }}
        />
      </span>
      <span>{message}</span>
    </div>
  ) : null;
}
