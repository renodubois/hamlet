import { useParams } from "@solidjs/router";
import { createEffect, createResource, createSignal, onCleanup, onMount } from "solid-js";
import ChannelMessages from "../components/channel_messages";
import EmojiPicker from "../components/emoji_picker";
import { EmojiIcon } from "../components/icons";
import TypingIndicator from "../components/typing_indicator";
import { listMessages, sendMessage, sendTyping, type Message, type User } from "../api";
import { useChannels } from "../contexts/channels";
import { useEvents } from "../contexts/events";
import { useAuth } from "../contexts/auth";
import { TYPING_PING_INTERVAL_MS } from "../constants";

export function syncMessagesForCurrentUser(
  messages: Message[] | undefined,
  user: User | null | undefined,
): Message[] | undefined {
  if (!messages || !user) return messages;
  return messages.map((message) =>
    message.user_id === user.id
      ? {
          ...message,
          username: user.username,
          display_name: user.display_name,
          avatar_url: user.avatar_url,
        }
      : message,
  );
}

export default function ChannelView() {
  const params = useParams<{ id: string }>();
  const { channels } = useChannels();
  const events = useEvents();
  const { user } = useAuth();
  const channel = () => channels()?.find((c) => String(c.id) === params.id);
  const [message, setMessage] = createSignal("");
  const [emojiPickerOpen, setEmojiPickerOpen] = createSignal(false);
  const [messages, { mutate }] = createResource(() => params.id, listMessages);
  let messageInputRef: HTMLInputElement | undefined;
  let emojiButtonRef: HTMLButtonElement | undefined;
  let lastTypingSentAt = 0;

  createEffect(() => {
    mutate((prev) => syncMessagesForCurrentUser(prev, user()));
  });

  const handleInput = (value: string) => {
    setMessage(value);
    if (value.length === 0) return;
    const now = Date.now();
    if (now - lastTypingSentAt < TYPING_PING_INTERVAL_MS) return;
    lastTypingSentAt = now;
    void sendTyping(params.id);
  };

  const handleEmojiSelect = (emoji: string) => {
    handleInput(`${message()}${emoji}`);
    queueMicrotask(() => messageInputRef?.focus());
  };

  onMount(() => {
    const unsubCreated = events.onMessage((m) => {
      if (String(m.channel_id) !== params.id) return;
      mutate((prev) => [...(prev ?? []), m]);
    });
    const unsubUpdated = events.onMessageUpdated((m) => {
      if (String(m.channel_id) !== params.id) return;
      mutate((prev) => prev?.map((existing) => (existing.id === m.id ? m : existing)));
    });
    const unsubDeleted = events.onMessageDeleted((d) => {
      if (String(d.channel_id) !== params.id) return;
      mutate((prev) => prev?.filter((existing) => existing.id !== d.id));
    });
    const unsubEmbeds = events.onMessageEmbedsUpdated((e) => {
      if (String(e.channel_id) !== params.id) return;
      mutate((prev) =>
        prev?.map((existing) =>
          existing.id === e.id
            ? { ...existing, suppress_embeds: e.suppress_embeds, embeds: e.embeds }
            : existing,
        ),
      );
    });
    onCleanup(() => {
      unsubCreated();
      unsubUpdated();
      unsubDeleted();
      unsubEmbeds();
    });
  });

  // TODO(reno): Can the router ensure this parameter exists?
  if (!params.id) {
    return <div>Error: channel required</div>;
  }

  return (
    <div class="flex flex-col h-full">
      <section class="bg-gray-100 text-gray-700 p-4 flex-shrink-0">
        <h1 class="text-2xl font-bold"># {channel()?.name ?? params.id}</h1>
      </section>

      <div class="flex-1 overflow-y-auto">
        <ChannelMessages messages={messages} currentUserId={user()?.id ?? null} />
      </div>

      <section class="flex-shrink-0 p-4 border-t border-gray-200">
        <TypingIndicator
          channelId={Number(params.id)}
          currentUserId={user()?.id ?? null}
          events={events}
        />
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void sendMessage(params.id, message());
            setMessage("");
            setEmojiPickerOpen(false);
            lastTypingSentAt = 0;
          }}
        >
          <div class="flex items-center gap-2">
            <input
              ref={(el) => {
                messageInputRef = el;
              }}
              class="bg-gray-100 rounded-md p-4 w-full"
              value={message()}
              onInput={(e) => handleInput(e.currentTarget.value)}
              placeholder="Send a new message..."
            />
            <button
              ref={(el) => {
                emojiButtonRef = el;
              }}
              type="button"
              class="rounded-md bg-gray-100 p-4 text-gray-700 hover:bg-gray-200 focus:outline-none focus:ring-2 focus:ring-blue-400"
              aria-label="Open emoji picker"
              aria-haspopup="dialog"
              aria-expanded={emojiPickerOpen()}
              title="Emoji"
              onClick={() => setEmojiPickerOpen((open) => !open)}
            >
              <EmojiIcon size={20} aria-hidden="true" />
            </button>
            <button class="bg-blue-100 p-4 rounded-md" type="submit">
              Send
            </button>
            <EmojiPicker
              open={emojiPickerOpen()}
              anchor={() => emojiButtonRef}
              onSelect={handleEmojiSelect}
              onClose={() => setEmojiPickerOpen(false)}
            />
          </div>
        </form>
      </section>
    </div>
  );
}
