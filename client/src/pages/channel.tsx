import { useParams } from "@solidjs/router";
import { createEffect, createResource, createSignal, onCleanup, onMount } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import ChannelMessages from "../components/channel_messages";
import EmojiPicker from "../components/emoji_picker";
import { EmojiIcon } from "../components/icons";
import TypingIndicator from "../components/typing_indicator";
import { listMessages, sendMessage, sendTyping, type Message } from "../api";
import { useChannels } from "../contexts/channels";
import { useEvents } from "../contexts/events";
import { useAuth } from "../contexts/auth";
import { TYPING_PING_INTERVAL_MS } from "../constants";

export default function ChannelView() {
  const params = useParams<{ id: string }>();
  const { channels } = useChannels();
  const events = useEvents();
  const { user } = useAuth();
  const channel = () => channels()?.find((c) => String(c.id) === params.id);
  const [message, setMessage] = createSignal("");
  const [emojiPickerOpen, setEmojiPickerOpen] = createSignal(false);
  // The Resource owns loading/error state for the initial fetch; the Store
  // owns the reactive list that SSE events mutate granularly. createStore
  // means an embed update on one row only re-renders that row.
  const [resource] = createResource(() => params.id, listMessages);
  const [messages, setMessages] = createStore<Message[]>([]);
  let messageInputRef: HTMLInputElement | undefined;
  let emojiButtonRef: HTMLButtonElement | undefined;
  let lastTypingSentAt = 0;

  // Reconcile the fetched array into the store on initial load and on every
  // channel switch. The "id" key tells reconcile to diff items rather than
  // replace the whole array, so already-rendered rows survive when the same
  // message appears in both the old and new fetches.
  createEffect(() => {
    const data = resource();
    setMessages(reconcile(data ?? [], { key: "id" }));
  });

  // When the current user's profile changes (display_name, avatar), patch
  // every message of theirs in place so the rendered list reflects the new
  // values without a refetch.
  createEffect(() => {
    const u = user();
    if (!u) return;
    setMessages(
      (m) => m.user_id === u.id,
      (m) => ({
        ...m,
        username: u.username,
        display_name: u.display_name,
        avatar_url: u.avatar_url,
      }),
    );
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
      setMessages(messages.length, m);
    });
    const unsubUpdated = events.onMessageUpdated((m) => {
      if (String(m.channel_id) !== params.id) return;
      setMessages((existing) => existing.id === m.id, m);
    });
    const unsubDeleted = events.onMessageDeleted((d) => {
      if (String(d.channel_id) !== params.id) return;
      setMessages((arr) => arr.filter((existing) => existing.id !== d.id));
    });
    const unsubEmbeds = events.onMessageEmbedsUpdated((e) => {
      if (String(e.channel_id) !== params.id) return;
      setMessages((existing) => existing.id === e.id, {
        suppress_embeds: e.suppress_embeds,
        embeds: e.embeds,
      });
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
        <ChannelMessages
          messages={messages}
          loading={resource.loading}
          error={resource.error}
          currentUserId={user()?.id ?? null}
        />
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
