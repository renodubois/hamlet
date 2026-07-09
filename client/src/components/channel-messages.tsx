import {
  Component,
  List,
  If,
  useComputedValue,
  useSignalState,
  registerCleanup,
  useMountEffect,
} from "../hooks/react-state";
import {
  addMessageReaction,
  deleteMessage,
  editMessage,
  messageDisplayName,
  removeMessageReaction,
  setMessageEmbedsSuppressed,
  type Message,
  type PublicUser,
  type ReactionRequest,
  type ReactionSummary,
  type SearchUsersOptions,
} from "../api";
import { useOptionalCustomEmojis } from "../contexts/custom-emojis";
import { CONSERVATIVE_EMOJIS } from "../emoji/emoji-data";
import { customEmojisToEntries, parseCustomEmojiMarkers } from "../emoji/custom-emojis";
import { messageMentionsCurrentUser } from "../mentions/mentions";
import { applyOptimisticReaction, reactionSummariesEqual } from "../reactions/reaction-summaries";
import AttachmentGrid from "./attachment-grid";
import Avatar from "./avatar";
import EmojiPicker from "./emoji-picker";
import { DeleteIcon, EditIcon, EmojiIcon } from "./icons";
import MessageEmbed from "./message-embed";
import MessageInput from "./message-input";
import MessageReferencePreview, { messageReferencePreviewText } from "./message-reference-preview";
import MessageText from "./message-text";
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

function messageRowClass(authoredByCurrentUser: boolean, mentionedCurrentUser: boolean): string {
  const borderClass = authoredByCurrentUser
    ? "border-blue-400"
    : mentionedCurrentUser
      ? "border-yellow-300"
      : "border-transparent";
  const stateClass = mentionedCurrentUser
    ? "bg-yellow-50 ring-1 ring-inset ring-yellow-300 hover:bg-yellow-100/80 focus-within:bg-yellow-100/80"
    : authoredByCurrentUser
      ? "bg-blue-50/50 hover:bg-blue-50 focus-within:bg-blue-50"
      : "hover:bg-gray-50 focus-within:bg-gray-50";

  return `group relative flex items-start gap-3 px-2 py-1 -mx-2 rounded-md border-l-4 transition-colors ${borderClass} ${stateClass}`;
}

export function channelMessageElementId(messageId: number): string {
  return `channel-message-${messageId}`;
}

const ChannelMessages: Component<{
  messages: readonly Message[];
  loading: boolean;
  error: unknown;
  currentUserId: number | null;
  onOpenThread?: (message: Message, options?: { focusComposer?: boolean }) => void;
  onReplyToMessage?: (message: Message) => void;
  onMessageUpdated?: (message: Message) => void;
  onReactionsChange?: (messageId: number, reactions: ReactionSummary[]) => void;
  mentionUsers?: readonly PublicUser[];
  onMentionUsers?: (users: readonly PublicUser[]) => void;
  searchMentionUsers?: (options: SearchUsersOptions) => Promise<PublicUser[]>;
  mentionSearchLimit?: number;
}> = (props) => {
  const customEmojis = useOptionalCustomEmojis();
  const activeCustomEmojis = () => customEmojis?.activeEmojis?.() ?? [];
  const reactionEmojiEntries = () => [
    ...CONSERVATIVE_EMOJIS,
    ...customEmojisToEntries(activeCustomEmojis()),
  ];
  const visibleMessageIds = useComputedValue(
    () => new Set(props.messages.map((message) => message.id)),
  );
  const customEmojiById = (id: number) => customEmojis?.byId(id) ?? null;
  const [contextMenu, setContextMenu] = useSignalState<ContextMenuState | null>(null);
  const [editingId, setEditingId] = useSignalState<number | null>(null);
  const [draft, setDraft] = useSignalState("");
  const [pendingDeleteId, setPendingDeleteId] = useSignalState<number | null>(null);
  const [reactionPicker, setReactionPicker] = useSignalState<ReactionPickerState | null>(null);

  const closeMenu = () => setContextMenu(null);
  const closeReactionPicker = () => setReactionPicker(null);

  useMountEffect(() => {
    const onDocClick = () => closeMenu();
    const onKey = (e: any) => {
      if (e.key === "Escape") {
        closeMenu();
        closeReactionPicker();
      }
    };
    document.addEventListener("click", onDocClick);
    document.addEventListener("keydown", onKey);
    registerCleanup(() => {
      document.removeEventListener("click", onDocClick);
      document.removeEventListener("keydown", onKey);
    });
  });

  const startEditing = (msg: Message) => {
    props.onMentionUsers?.(msg.mentions ?? []);
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
      const updated = await editMessage(msg.id, next);
      props.onMentionUsers?.(updated?.mentions ?? []);
      props.onMessageUpdated?.(updated);
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

  const isMentionedCurrentUser = (msg: Message) =>
    messageMentionsCurrentUser(msg, props.currentUserId);

  const canOpenThread = (msg: Message) =>
    !isDeletedMessage(msg) && msg.parent_id == null && props.onOpenThread !== undefined;

  const canInlineReply = (msg: Message) =>
    !isDeletedMessage(msg) && msg.parent_id == null && props.onReplyToMessage !== undefined;

  const canReact = (msg: Message) => !isDeletedMessage(msg) && props.currentUserId !== null;

  const hasAnyAction = (msg: Message) =>
    canReact(msg) || isOwnMessage(msg) || canOpenThread(msg) || canInlineReply(msg);

  const referencedTargetId = (msg: Message) => msg.reply_to_message_id ?? msg.reply_to?.id ?? null;

  const canJumpToReferencedMessage = (targetId: number | null | undefined) =>
    targetId != null && visibleMessageIds().has(targetId);

  const jumpToReferencedMessage = (targetId: number | null | undefined) => {
    if (targetId == null) return;
    document.getElementById(channelMessageElementId(targetId))?.scrollIntoView({
      block: "center",
      inline: "nearest",
    });
  };

  const handleContextMenu = (e: any, msg: Message) => {
    if (!isOwnMessage(msg)) return;
    e.preventDefault();
    setContextMenu({ messageId: msg.id, x: e.clientX, y: e.clientY });
  };

  return (
    <section className="bg-white text-gray-900 p-8 min-h-full flex flex-col justify-end">
      <If when={props.loading && props.messages.length === 0}>
        <p>Loading...</p>
      </If>
      <If when={props.error}>
        <span>Error getting messages: {String(props.error)}</span>
      </If>
      <List each={props.messages}>
        {(message) => (
          <div
            id={channelMessageElementId(message.id)}
            data-message-id={String(message.id)}
            data-authored-by-current-user={isOwnMessage(message) ? "true" : undefined}
            data-mentioned-current-user={isMentionedCurrentUser(message) ? "true" : undefined}
            className={messageRowClass(isOwnMessage(message), isMentionedCurrentUser(message))}
            onContextMenu={(e) => handleContextMenu(e, message)}
          >
            <Avatar
              url={isDeletedMessage(message) ? null : message.avatar_url}
              username={isDeletedMessage(message) ? "Deleted message" : messageDisplayName(message)}
              size={32}
            />
            <div className="min-w-0 flex-1">
              <If
                when={!isDeletedMessage(message)}
                fallback={
                  <p className="italic text-gray-500" aria-label="Original message deleted">
                    Original message deleted
                  </p>
                }
              >
                <div className="font-bold">{messageDisplayName(message)}</div>
                <If when={message.reply_to ?? message.reply_to_message_id ?? null}>
                  <MessageReferencePreview
                    reference={message.reply_to ?? null}
                    targetId={referencedTargetId(message)}
                    onActivate={
                      canJumpToReferencedMessage(referencedTargetId(message))
                        ? () => jumpToReferencedMessage(referencedTargetId(message))
                        : undefined
                    }
                  />
                </If>
                <If
                  when={editingId() === message.id}
                  fallback={
                    <MessageText
                      text={message.text}
                      mentions={message.mentions ?? []}
                      currentUserId={props.currentUserId}
                    />
                  }
                >
                  <form
                    className="flex gap-2 items-center"
                    onSubmit={(e) => {
                      e.preventDefault();
                      void saveEdit(message);
                    }}
                  >
                    <MessageInput
                      value={draft()}
                      onChange={setDraft}
                      ariaLabel="Edit message"
                      mentionUsers={props.mentionUsers}
                      onMentionUsers={props.onMentionUsers}
                      searchMentionUsers={props.searchMentionUsers}
                      mentionSearchLimit={props.mentionSearchLimit}
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
                      className="flex min-w-0 flex-1 items-center gap-2"
                      inputClass="bg-gray-100 rounded-md px-2 py-1 w-full"
                      emojiButtonClass="cursor-pointer rounded-md bg-gray-100 px-2 py-1 text-gray-700 hover:bg-gray-200 focus:outline-none focus:ring-2 focus:ring-blue-400"
                      emojiButtonLabel="Open emoji picker for edit"
                    />
                    <button type="submit" className="text-sm text-blue-600">
                      Save
                    </button>
                    <button type="button" className="text-sm text-gray-500" onClick={cancelEditing}>
                      Cancel
                    </button>
                  </form>
                </If>
              </If>
              <If when={!isDeletedMessage(message) && message.attachments.length > 0}>
                <AttachmentGrid
                  attachments={message.attachments}
                  authorName={messageDisplayName(message)}
                />
              </If>
              <If
                when={
                  !isDeletedMessage(message) &&
                  !message.suppress_embeds &&
                  message.embeds.length > 0
                }
              >
                <div className="flex flex-col gap-1">
                  <List each={message.embeds}>
                    {(embed) => (
                      <MessageEmbed
                        embed={embed}
                        onRemove={
                          isOwnMessage(message) ? () => void suppressEmbeds(message) : undefined
                        }
                      />
                    )}
                  </List>
                </div>
              </If>
              <If when={!isDeletedMessage(message)}>
                <ReactionRow
                  reactions={message.reactions ?? []}
                  onToggle={(reaction) => toggleReaction(message, reaction)}
                />
              </If>
              <If when={message.thread_summary?.reply_count ? message.thread_summary : null}>
                {(summary) => {
                  const countText = () => replyCountLabel(summary().reply_count);
                  const lastReplyText = () =>
                    formatThreadTimestamp(summary().last_reply_created_at);
                  return (
                    <button
                      type="button"
                      className="mt-1 text-sm font-medium text-blue-700 hover:underline focus:outline-none focus:ring-2 focus:ring-blue-400 rounded"
                      title={`Last reply ${formatThreadTimestampTitle(summary().last_reply_created_at)}`}
                      aria-label={`Open thread with ${countText()}, last reply ${lastReplyText()}`}
                      onClick={() => props.onOpenThread?.(message, { focusComposer: false })}
                    >
                      {countText()} · Last reply {lastReplyText()}
                    </button>
                  );
                }}
              </If>
            </div>
            <If when={hasAnyAction(message) && editingId() !== message.id}>
              <div
                role="toolbar"
                aria-label={`Message actions for ${referencedMessageLabel(message)}`}
                className="absolute -top-3 right-2 flex gap-1 rounded-md border border-gray-200 bg-white shadow-sm opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity"
              >
                <If when={canReact(message)}>
                  <button
                    type="button"
                    aria-label={`Add reaction to message by ${messageDisplayName(message)}`}
                    title="Add reaction"
                    className="p-1.5 rounded-md hover:bg-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-400"
                    onClick={(event) => {
                      event.stopPropagation();
                      setReactionPicker({ messageId: message.id, anchor: event.currentTarget });
                    }}
                  >
                    <EmojiIcon size={14} />
                  </button>
                </If>
                <If when={canInlineReply(message)}>
                  <button
                    type="button"
                    aria-label={`Reply inline to ${referencedMessageLabel(message)}`}
                    title="Reply inline"
                    className="rounded-md bg-blue-50 px-2 py-1 text-xs font-medium text-blue-700 hover:bg-blue-100 focus:outline-none focus:ring-2 focus:ring-blue-400"
                    onClick={() => props.onReplyToMessage?.(message)}
                  >
                    Reply
                  </button>
                </If>
                <If when={canOpenThread(message)}>
                  <button
                    type="button"
                    aria-label={`Reply in thread to ${referencedMessageLabel(message)}`}
                    title="Reply in thread"
                    className="rounded-md bg-purple-50 px-2 py-1 text-xs font-medium text-purple-700 hover:bg-purple-100 focus:outline-none focus:ring-2 focus:ring-purple-400"
                    onClick={() => props.onOpenThread?.(message, { focusComposer: true })}
                  >
                    Thread
                  </button>
                </If>
                <If when={isOwnMessage(message)}>
                  <button
                    type="button"
                    aria-label="Edit"
                    title="Edit"
                    className="p-1.5 rounded-md hover:bg-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-400"
                    onClick={() => startEditing(message)}
                  >
                    <EditIcon size={14} />
                  </button>
                  <button
                    type="button"
                    aria-label="Delete"
                    title="Delete"
                    className="p-1.5 rounded-md text-red-600 hover:bg-red-50 focus:outline-none focus:ring-2 focus:ring-red-400"
                    onClick={() => requestDelete(message.id)}
                  >
                    <DeleteIcon size={14} />
                  </button>
                </If>
              </div>
            </If>
          </div>
        )}
      </List>
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
      <If when={contextMenu()}>
        {(menu) => (
          <ul
            role="menu"
            className="fixed z-50 bg-white border border-gray-200 rounded-md shadow-md py-1"
            style={{ top: `${menu().y}px`, left: `${menu().x}px` }}
            onClick={(e) => e.stopPropagation()}
          >
            <li>
              <button
                role="menuitem"
                type="button"
                className="w-full text-left px-4 py-1 hover:bg-gray-100"
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
                className="w-full text-left px-4 py-1 text-red-600 hover:bg-red-50"
                onClick={() => requestDelete(menu().messageId)}
              >
                Delete message
              </button>
            </li>
          </ul>
        )}
      </If>
      <Modal open={pendingDeleteId() !== null} onClose={cancelDelete} title="Delete message?">
        <p className="text-sm text-gray-200 mb-4">
          This will permanently delete the message. This cannot be undone.
        </p>
        <div className="flex gap-2 justify-end">
          <button
            type="button"
            className="text-gray-300 hover:text-gray-100 text-sm px-3 py-2"
            onClick={cancelDelete}
          >
            Cancel
          </button>
          <button
            type="button"
            className="bg-red-600 hover:bg-red-700 text-white rounded-md px-4 py-2 text-sm font-medium transition-colors"
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
