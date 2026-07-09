import { useRef } from "react";
import { flushSync } from "react-dom";

import { If, useSignalState, useMountEffect } from "../hooks/react-state";
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

export default function TypingIndicator(props: Props) {
  const now = () => (props.now ?? Date.now)();
  const [entries, setEntries] = useSignalState<Record<number, TypingEntry>>({});
  const propsRef = useRef(props);
  propsRef.current = props;

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

  const clearUser = (userId: number) => {
    setEntries((prev) => {
      if (!(userId in prev)) return prev;
      const next = { ...prev };
      delete next[userId];
      return next;
    });
  };

  useMountEffect(() => {
    const unsubscribers = [
      props.events.onUserTyping((t: UserTyping) => {
        if (t.channel_id !== propsRef.current.channelId) return;
        if (
          propsRef.current.currentUserId !== null &&
          t.user_id === propsRef.current.currentUserId
        ) {
          return;
        }
        flushSync(() => {
          setEntries((prev) => ({
            ...prev,
            [t.user_id]: { username: t.username, lastSeen: now() },
          }));
        });
      }),
      props.events.onMessage((m: Message) => {
        if (m.channel_id !== propsRef.current.channelId) return;
        flushSync(() => clearUser(m.user_id));
      }),
    ];
    const timer = window.setInterval(prune, TYPING_SWEEP_MS);
    return () => {
      unsubscribers.forEach((unsubscribe) => unsubscribe());
      window.clearInterval(timer);
    };
  });

  const usernames = () =>
    Object.values(entries())
      .map((e) => e.username)
      .sort((a, b) => a.localeCompare(b));

  const message = () => formatTypingMessage(usernames());

  return (
    <If when={message()} keyed>
      {(text) => (
        <div
          className="flex items-center gap-2 text-xs text-gray-500 px-1 pb-1 h-5"
          aria-live="polite"
          data-testid="typing-indicator"
        >
          <span className="inline-flex items-center gap-0.5" aria-hidden="true">
            <span
              className="inline-block w-1 h-1 rounded-full bg-gray-500 animate-bounce"
              style={{ animationDelay: "0ms" }}
            />
            <span
              className="inline-block w-1 h-1 rounded-full bg-gray-500 animate-bounce"
              style={{ animationDelay: "150ms" }}
            />
            <span
              className="inline-block w-1 h-1 rounded-full bg-gray-500 animate-bounce"
              style={{ animationDelay: "300ms" }}
            />
          </span>
          <span>{text}</span>
        </div>
      )}
    </If>
  );
}
