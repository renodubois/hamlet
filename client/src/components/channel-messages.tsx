import { Component, For, Show, createSignal, onCleanup, onMount, type JSX } from "solid-js";
import {
  addMessageReaction,
  deleteMessage,
  editMessage,
  messageDisplayName,
  resolveServerUrl,
  removeMessageReaction,
  setMessageEmbedsSuppressed,
  type CustomEmoji,
  type Message,
  type ReactionRequest,
  type ReactionSummary,
} from "../api";
import { useOptionalCustomEmojis } from "../contexts/custom-emojis";
import { CONSERVATIVE_EMOJIS } from "../emoji/emoji-data";
import { customEmojisToEntries, parseCustomEmojiMarkers } from "../emoji/custom-emojis";
import { applyOptimisticReaction, reactionSummariesEqual } from "../reactions/reaction-summaries";
import { linkifyText } from "../linkify";
import AttachmentGrid from "./attachment-grid";
import Avatar from "./avatar";
import EmojiPicker from "./emoji-picker";
import { DeleteIcon, EditIcon, EmojiIcon } from "./icons";
import MessageEmbed from "./message-embed";
import MessageInput from "./message-input";
import MessageReferencePreview, { messageReferencePreviewText } from "./message-reference-preview";
import Modal from "./modal";
import ReactionRow from "./reaction-row";

interface ContextMenuState {
  messageId: number;
  x: number;
  y: number;
}

interface ReactionPickerState {
  messageId: number;
  anchor: HTMLElement;
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
        src={resolveServerUrl(emoji.image_url)}
        alt={label}
        title={emoji.deleted_at === null ? label : `${label} (deleted)`}
        class="inline-block h-6 w-6 align-text-bottom object-contain"
      />
    );
  });
}

function formatThreadTimestamp(timestampMicros: number): string {
  const date = new Date(Math.trunc(timestampMicros / 1000));
  if (Number.isNaN(date.getTime())) return "unknown time";
  return `${date.toISOString().slice(0, 16).replace("T", " ")} UTC`;
}

function formatThreadTimestampTitle(timestampMicros: number): string {
  const date = new Date(Math.trunc(timestampMicros / 1000));
  if (Number.isNaN(date.getTime())) return "unknown time";
  return `${date.toISOString().slice(0, 19).replace("T", " ")} UTC`;
}

function replyCountLabel(count: number): string {
  return count === 1 ? "1 reply" : `${count} replies`;
}

function referencedMessageLabel(message: Message): string {
  return `message by ${messageDisplayName(message)}: ${messageReferencePreviewText(message)}`;
}

const ChannelMessages: Component<{
  messages: readonly Message[];
  loading: boolean;
  error: unknown;
  currentUserId: number | null;
  onOpenThread?: (message: Message, options?: { focusComposer?: boolean }) => void;
  onReplyToMessage?: (message: Message) => void;
  onReactionsChange?: (messageId: number, reactions: ReactionSummary[]) => void;
}> = (props) => {
  const customEmojis = useOptionalCustomEmojis();
  const activeCustomEmojis = () => customEmojis?.activeEmojis?.() ?? [];
  const reactionEmojiEntries = () => [
    ...CONSERVATIVE_EMOJIS,
    ...customEmojisToEntries(activeCustomEmojis()),
  ];
  const customEmojiById = (id: number) => customEmojis?.byId(id) ?? null;
  const [contextMenu, setContextMenu] = createSignal<ContextMenuState | null>(null);
  const [editingId, setEditingId] = createSignal<number | null>(null);
  const [draft, setDraft] = createSignal("");
  const [pendingDeleteId, setPendingDeleteId] = createSignal<number | null>(null);
  const [reactionPicker, setReactionPicker] = createSignal<ReactionPickerState | null>(null);

  const closeMenu = () => setContextMenu(null);
  const closeReactionPicker = () => setReactionPicker(null);

  onMount(() => {
    const onDocClick = () => closeMenu();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        closeMenu();
        closeReactionPicker();
      }
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
    if (next.length === 0 && msg.attachments.length === 0) {
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

  const mutateReaction = async (
    msg: Message,
    reaction: ReactionRequest,
    mutation: "add" | "remove",
  ) => {
    const previous = msg.reactions ?? [];
    const optimistic = applyOptimisticReaction(previous, reaction, mutation);
    const currentReactions = () =>
      props.messages.find((message) => message.id === msg.id)?.reactions ?? [];
    const canApplyHttpResult = () => {
      const current = currentReactions();
      return (
        reactionSummariesEqual(current, optimistic) || reactionSummariesEqual(current, previous)
      );
    };
    props.onReactionsChange?.(msg.id, optimistic);

    try {
      const canonical =
        mutation === "add"
          ? await addMessageReaction(msg.id, reaction)
          : await removeMessageReaction(msg.id, reaction);
      if (canApplyHttpResult()) {
        props.onReactionsChange?.(msg.id, canonical);
      }
    } catch (e) {
      if (canApplyHttpResult()) {
        props.onReactionsChange?.(msg.id, previous);
      }
      console.error("failed to update reaction", e);
    }
  };

  const addReactionFromPicker = (msg: Message, emoji: string) => {
    closeReactionPicker();
    const customToken = parseCustomEmojiMarkers(emoji)[0];
    if (customToken?.type === "custom-emoji" && customToken.marker === emoji) {
      const customEmoji = customEmojiById(customToken.id);
      if (!customEmoji || customEmoji.deleted_at !== null) return;
      void mutateReaction(
        msg,
        {
          kind: "custom",
          emoji_id: customEmoji.id,
          name: customEmoji.name,
          image_url: customEmoji.image_url,
          animated: customEmoji.animated,
        },
        "add",
      );
      return;
    }

    void mutateReaction(msg, { kind: "native", emoji }, "add");
  };

  const toggleReaction = (msg: Message, reaction: ReactionSummary) => {
    const request: ReactionRequest =
      reaction.kind === "native"
        ? { kind: "native", emoji: reaction.emoji }
        : {
            kind: "custom",
            emoji_id: reaction.emoji_id,
            name: reaction.name,
            image_url: reaction.image_url,
            animated: reaction.animated,
          };
    void mutateReaction(msg, request, reaction.me_reacted ? "remove" : "add");
  };

  const isDeletedMessage = (msg: Message) => msg.deleted_at != null;

  const isOwnMessage = (msg: Message) =>
    !isDeletedMessage(msg) && props.currentUserId !== null && msg.user_id === props.currentUserId;

  const canOpenThread = (msg: Message) =>
    !isDeletedMessage(msg) && msg.parent_id == null && props.onOpenThread !== undefined;

  const canInlineReply = (msg: Message) =>
    !isDeletedMessage(msg) && msg.parent_id == null && props.onReplyToMessage !== undefined;

  const canReact = (msg: Message) => !isDeletedMessage(msg) && props.currentUserId !== null;

  const hasAnyAction = (msg: Message) =>
    canReact(msg) || isOwnMessage(msg) || canOpenThread(msg) || canInlineReply(msg);

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
            <Avatar
              url={isDeletedMessage(message) ? null : message.avatar_url}
              username={isDeletedMessage(message) ? "Deleted message" : messageDisplayName(message)}
              size={32}
            />
            <div class="min-w-0 flex-1">
              <Show
                when={!isDeletedMessage(message)}
                fallback={
                  <p class="italic text-gray-500" aria-label="Original message deleted">
                    Original message deleted
                  </p>
                }
              >
                <div class="font-bold">{messageDisplayName(message)}</div>
                <Show when={message.reply_to ?? message.reply_to_message_id ?? null}>
                  <MessageReferencePreview
                    reference={message.reply_to ?? null}
                    targetId={message.reply_to_message_id ?? message.reply_to?.id}
                  />
                </Show>
                <Show
                  when={editingId() === message.id}
                  fallback={
                    <div class="whitespace-pre-wrap break-words [overflow-wrap:anywhere]">
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
              </Show>
              <Show when={!isDeletedMessage(message) && message.attachments.length > 0}>
                <AttachmentGrid
                  attachments={message.attachments}
                  authorName={messageDisplayName(message)}
                />
              </Show>
              <Show
                when={
                  !isDeletedMessage(message) &&
                  !message.suppress_embeds &&
                  message.embeds.length > 0
                }
              >
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
              <Show when={!isDeletedMessage(message)}>
                <ReactionRow
                  reactions={message.reactions ?? []}
                  onToggle={(reaction) => toggleReaction(message, reaction)}
                />
              </Show>
              <Show when={message.thread_summary?.reply_count ? message.thread_summary : null}>
                {(summary) => {
                  const countText = () => replyCountLabel(summary().reply_count);
                  const lastReplyText = () =>
                    formatThreadTimestamp(summary().last_reply_created_at);
                  return (
                    <button
                      type="button"
                      class="mt-1 text-sm font-medium text-blue-700 hover:underline focus:outline-none focus:ring-2 focus:ring-blue-400 rounded"
                      title={`Last reply ${formatThreadTimestampTitle(summary().last_reply_created_at)}`}
                      aria-label={`Open thread with ${countText()}, last reply ${lastReplyText()}`}
                      onClick={() => props.onOpenThread?.(message, { focusComposer: false })}
                    >
                      {countText()} · Last reply {lastReplyText()}
                    </button>
                  );
                }}
              </Show>
            </div>
            <Show when={hasAnyAction(message) && editingId() !== message.id}>
              <div
                role="toolbar"
                aria-label={`Message actions for ${referencedMessageLabel(message)}`}
                class="absolute -top-3 right-2 flex gap-1 rounded-md border border-gray-200 bg-white shadow-sm opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity"
              >
                <Show when={canReact(message)}>
                  <button
                    type="button"
                    aria-label={`Add reaction to message by ${messageDisplayName(message)}`}
                    title="Add reaction"
                    class="p-1.5 rounded-md hover:bg-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-400"
                    onClick={(event) => {
                      event.stopPropagation();
                      setReactionPicker({ messageId: message.id, anchor: event.currentTarget });
                    }}
                  >
                    <EmojiIcon size={14} />
                  </button>
                </Show>
                <Show when={canInlineReply(message)}>
                  <button
                    type="button"
                    aria-label={`Reply inline to ${referencedMessageLabel(message)}`}
                    title="Reply inline"
                    class="rounded-md bg-blue-50 px-2 py-1 text-xs font-medium text-blue-700 hover:bg-blue-100 focus:outline-none focus:ring-2 focus:ring-blue-400"
                    onClick={() => props.onReplyToMessage?.(message)}
                  >
                    Reply
                  </button>
                </Show>
                <Show when={canOpenThread(message)}>
                  <button
                    type="button"
                    aria-label={`Reply in thread to ${referencedMessageLabel(message)}`}
                    title="Reply in thread"
                    class="rounded-md bg-purple-50 px-2 py-1 text-xs font-medium text-purple-700 hover:bg-purple-100 focus:outline-none focus:ring-2 focus:ring-purple-400"
                    onClick={() => props.onOpenThread?.(message, { focusComposer: true })}
                  >
                    Thread
                  </button>
                </Show>
                <Show when={isOwnMessage(message)}>
                  <button
                    type="button"
                    aria-label="Edit"
                    title="Edit"
                    class="p-1.5 rounded-md hover:bg-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-400"
                    onClick={() => startEditing(message)}
                  >
                    <EditIcon size={14} />
                  </button>
                  <button
                    type="button"
                    aria-label="Delete"
                    title="Delete"
                    class="p-1.5 rounded-md text-red-600 hover:bg-red-50 focus:outline-none focus:ring-2 focus:ring-red-400"
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
      <EmojiPicker
        open={reactionPicker() !== null}
        anchor={() => reactionPicker()?.anchor}
        emojis={reactionEmojiEntries()}
        onSelect={(emoji) => {
          const state = reactionPicker();
          const msg = state
            ? props.messages.find((message) => message.id === state.messageId)
            : null;
          if (msg) addReactionFromPicker(msg, emoji);
        }}
        onClose={closeReactionPicker}
      />
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
