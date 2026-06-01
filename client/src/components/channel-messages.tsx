import { Component, For, Show, createSignal, onCleanup, onMount, type JSX } from "solid-js";
import {
  deleteMessage,
  editMessage,
  getServerUrl,
  messageDisplayName,
  setMessageEmbedsSuppressed,
  type CustomEmoji,
  type Message,
} from "../api";
import { useOptionalCustomEmojis } from "../contexts/custom-emojis";
import { parseCustomEmojiMarkers } from "../emoji/custom-emojis";
import { linkifyText } from "../linkify";
import Avatar from "./avatar";
import { DeleteIcon, EditIcon } from "./icons";
import MessageEmbed from "./message-embed";
import MessageInput from "./message-input";
import Modal from "./modal";

interface ContextMenuState {
  messageId: number;
  x: number;
  y: number;
}

function resolveImageUrl(url: string): string {
  if (url.startsWith("http://") || url.startsWith("https://")) return url;
  return `${getServerUrl()}${url}`;
}

function renderTextWithCustomEmojis(
  text: string,
  byId: (id: number) => CustomEmoji | null,
): JSX.Element[] {
  return parseCustomEmojiMarkers(text).map((token) => {
    if (token.type === "text") return token.value;

    const emoji = byId(token.id);
    if (!emoji) {
      return (
        <span title={`Custom emoji ${token.marker} is unavailable`}>:{token.storedName}:</span>
      );
    }

    const label = `:${emoji.name}:`;
    return (
      <img
        src={resolveImageUrl(emoji.image_url)}
        alt={label}
        title={emoji.deleted_at === null ? label : `${label} (deleted)`}
        class="inline-block h-6 w-6 align-text-bottom object-contain"
      />
    );
  });
}

const ChannelMessages: Component<{
  messages: readonly Message[];
  loading: boolean;
  error: unknown;
  currentUserId: number | null;
}> = (props) => {
  const customEmojis = useOptionalCustomEmojis();
  const customEmojiById = (id: number) => customEmojis?.byId(id) ?? null;
  const [contextMenu, setContextMenu] = createSignal<ContextMenuState | null>(null);
  const [editingId, setEditingId] = createSignal<number | null>(null);
  const [draft, setDraft] = createSignal("");
  const [pendingDeleteId, setPendingDeleteId] = createSignal<number | null>(null);

  const closeMenu = () => setContextMenu(null);

  onMount(() => {
    const onDocClick = () => closeMenu();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeMenu();
    };
    document.addEventListener("click", onDocClick);
    document.addEventListener("keydown", onKey);
    onCleanup(() => {
      document.removeEventListener("click", onDocClick);
      document.removeEventListener("keydown", onKey);
    });
  });

  const startEditing = (msg: Message) => {
    setDraft(msg.text);
    setEditingId(msg.id);
    closeMenu();
  };

  const cancelEditing = () => {
    setEditingId(null);
    setDraft("");
  };

  const requestDelete = (id: number) => {
    setPendingDeleteId(id);
    closeMenu();
  };

  const cancelDelete = () => {
    setPendingDeleteId(null);
  };

  const confirmDelete = async () => {
    const id = pendingDeleteId();
    if (id === null) return;
    try {
      await deleteMessage(id);
    } catch (e) {
      console.error("failed to delete message", e);
    }
    setPendingDeleteId(null);
    if (editingId() === id) cancelEditing();
  };

  const suppressEmbeds = async (msg: Message) => {
    try {
      await setMessageEmbedsSuppressed(msg.id, true);
    } catch (e) {
      console.error("failed to suppress embeds", e);
    }
  };

  const saveEdit = async (msg: Message) => {
    const next = draft();
    if (next.length === 0) {
      requestDelete(msg.id);
      return;
    }
    if (next === msg.text) {
      cancelEditing();
      return;
    }
    try {
      await editMessage(msg.id, next);
    } catch (e) {
      console.error("failed to edit message", e);
    }
    cancelEditing();
  };

  const isOwnMessage = (msg: Message) =>
    props.currentUserId !== null && msg.user_id === props.currentUserId;

  // Whether any hover-toolbar action applies to this message. Today only
  // owner-gated actions (Edit, Delete) exist; non-owner actions (react,
  // reply, quote, …) will OR into this check when they land.
  const hasAnyAction = (msg: Message) => isOwnMessage(msg);

  const handleContextMenu = (e: MouseEvent, msg: Message) => {
    if (!isOwnMessage(msg)) return;
    e.preventDefault();
    setContextMenu({ messageId: msg.id, x: e.clientX, y: e.clientY });
  };

  return (
    <section class="bg-white text-gray-900 p-8 min-h-full flex flex-col justify-end">
      <Show when={props.loading && props.messages.length === 0}>
        <p>Loading...</p>
      </Show>
      <Show when={props.error}>
        <span>Error getting messages: {String(props.error)}</span>
      </Show>
      <For each={props.messages}>
        {(message) => (
          <div
            class="group relative flex items-start gap-3 px-2 py-1 -mx-2 rounded-md hover:bg-gray-50 focus-within:bg-gray-50"
            onContextMenu={(e) => handleContextMenu(e, message)}
          >
            <Avatar url={message.avatar_url} username={messageDisplayName(message)} size={32} />
            <div class="min-w-0 flex-1">
              <div class="font-bold">{messageDisplayName(message)}</div>
              <Show
                when={editingId() === message.id}
                fallback={
                  <div class="whitespace-pre-wrap break-words">
                    {linkifyText(message.text).map((tok) =>
                      tok.type === "link" ? (
                        <a
                          href={tok.url}
                          target="_blank"
                          rel="noopener noreferrer"
                          class="text-blue-700 hover:underline break-all"
                        >
                          {tok.url}
                        </a>
                      ) : (
                        renderTextWithCustomEmojis(tok.value, customEmojiById)
                      ),
                    )}
                  </div>
                }
              >
                <form
                  class="flex gap-2 items-center"
                  onSubmit={(e) => {
                    e.preventDefault();
                    void saveEdit(message);
                  }}
                >
                  <MessageInput
                    value={draft()}
                    onChange={setDraft}
                    ariaLabel="Edit message"
                    inputRef={(el) =>
                      queueMicrotask(() => {
                        if (el.isConnected) el.focus();
                      })
                    }
                    onKeyDown={(e) => {
                      if (e.key === "Escape") {
                        e.preventDefault();
                        cancelEditing();
                      }
                    }}
                    class="flex min-w-0 flex-1 items-center gap-2"
                    inputClass="bg-gray-100 rounded-md px-2 py-1 w-full"
                    emojiButtonClass="cursor-pointer rounded-md bg-gray-100 px-2 py-1 text-gray-700 hover:bg-gray-200 focus:outline-none focus:ring-2 focus:ring-blue-400"
                    emojiButtonLabel="Open emoji picker for edit"
                  />
                  <button type="submit" class="text-sm text-blue-600">
                    Save
                  </button>
                  <button type="button" class="text-sm text-gray-500" onClick={cancelEditing}>
                    Cancel
                  </button>
                </form>
              </Show>
              <Show when={!message.suppress_embeds && message.embeds.length > 0}>
                <div class="flex flex-col gap-1">
                  <For each={message.embeds}>
                    {(embed) => (
                      <MessageEmbed
                        embed={embed}
                        onRemove={
                          isOwnMessage(message) ? () => void suppressEmbeds(message) : undefined
                        }
                      />
                    )}
                  </For>
                </div>
              </Show>
            </div>
            <Show when={hasAnyAction(message) && editingId() !== message.id}>
              <div
                role="toolbar"
                aria-label="Message actions"
                class="absolute -top-3 right-2 flex gap-1 rounded-md border border-gray-200 bg-white shadow-sm opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity"
              >
                <Show when={isOwnMessage(message)}>
                  <button
                    type="button"
                    aria-label="Edit"
                    title="Edit"
                    class="p-1.5 rounded-md hover:bg-gray-100"
                    onClick={() => startEditing(message)}
                  >
                    <EditIcon size={14} />
                  </button>
                  <button
                    type="button"
                    aria-label="Delete"
                    title="Delete"
                    class="p-1.5 rounded-md text-red-600 hover:bg-red-50"
                    onClick={() => requestDelete(message.id)}
                  >
                    <DeleteIcon size={14} />
                  </button>
                </Show>
              </div>
            </Show>
          </div>
        )}
      </For>
      <Show when={contextMenu()}>
        {(menu) => (
          <ul
            role="menu"
            class="fixed z-50 bg-white border border-gray-200 rounded-md shadow-md py-1"
            style={{ top: `${menu().y}px`, left: `${menu().x}px` }}
            onClick={(e) => e.stopPropagation()}
          >
            <li>
              <button
                role="menuitem"
                type="button"
                class="w-full text-left px-4 py-1 hover:bg-gray-100"
                onClick={() => {
                  const id = menu().messageId;
                  const msg = props.messages.find((m) => m.id === id);
                  if (msg) startEditing(msg);
                }}
              >
                Edit message
              </button>
            </li>
            <li>
              <button
                role="menuitem"
                type="button"
                class="w-full text-left px-4 py-1 text-red-600 hover:bg-red-50"
                onClick={() => requestDelete(menu().messageId)}
              >
                Delete message
              </button>
            </li>
          </ul>
        )}
      </Show>
      <Modal open={pendingDeleteId() !== null} onClose={cancelDelete} title="Delete message?">
        <p class="text-sm text-gray-200 mb-4">
          This will permanently delete the message. This cannot be undone.
        </p>
        <div class="flex gap-2 justify-end">
          <button
            type="button"
            class="text-gray-300 hover:text-gray-100 text-sm px-3 py-2"
            onClick={cancelDelete}
          >
            Cancel
          </button>
          <button
            type="button"
            class="bg-red-600 hover:bg-red-700 text-white rounded-md px-4 py-2 text-sm font-medium transition-colors"
            onClick={() => void confirmDelete()}
          >
            Delete
          </button>
        </div>
      </Modal>
    </section>
  );
};

export default ChannelMessages;
