import { useEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
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
import { Button } from "./ui/button";
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
    ? "border-primary"
    : mentionedCurrentUser
      ? "border-primary/50"
      : "border-transparent";
  const stateClass = mentionedCurrentUser
    ? "bg-primary/10 ring-1 ring-inset ring-primary/20 hover:bg-primary/15 focus-within:bg-primary/15"
    : authoredByCurrentUser
      ? "bg-primary/5 hover:bg-primary/10 focus-within:bg-primary/10"
      : "hover:bg-accent focus-within:bg-accent";
  return `group relative flex items-start gap-3 px-2 py-1 -mx-2 rounded-md border-l-4 transition-colors ${borderClass} ${stateClass}`;
}
export function channelMessageElementId(messageId: number): string {
  return `channel-message-${messageId}`;
}
interface ChannelMessagesProps {
  channelId?: number;
  generation?: number;
  messages: readonly Message[];
  loading: boolean;
  error: unknown;
  currentUserId: number | null;
  onOpenThread?: (
    message: Message,
    options?: {
      focusComposer?: boolean;
    },
  ) => void;
  onReplyToMessage?: (message: Message) => void;
  onMessageUpdated?: (message: Message) => void;
  onReactionsChange?: (messageId: number, reactions: ReactionSummary[]) => void;
  mentionUsers?: readonly PublicUser[];
  onMentionUsers?: (users: readonly PublicUser[]) => void;
  searchMentionUsers?: (options: SearchUsersOptions) => Promise<PublicUser[]>;
  mentionSearchLimit?: number;
}
function ChannelMessages(props: ChannelMessagesProps) {
  const customEmojis = useOptionalCustomEmojis();
  const scopeKey = `${props.channelId ?? props.messages[0]?.channel_id ?? 0}:${props.generation ?? 0}`;
  const activeScopeRef = useRef(scopeKey);
  activeScopeRef.current = scopeKey;
  const mountedRef = useRef(true);
  const activeCustomEmojis = customEmojis?.activeEmojis ?? [];
  const reactionEmojiEntries = [
    ...CONSERVATIVE_EMOJIS,
    ...customEmojisToEntries(activeCustomEmojis),
  ];
  const visibleMessageIds = new Set(props.messages.map((message) => message.id));
  const customEmojiById = (id: number) => customEmojis?.byId(id) ?? null;
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [draft, setDraft] = useState("");
  const [pendingDeleteId, setPendingDeleteId] = useState<number | null>(null);
  const [reactionPicker, setReactionPicker] = useState<ReactionPickerState | null>(null);
  const reactionOperationTokensRef = useRef(new Map<number, number>());
  const latestMessagesRef = useRef(props.messages);
  latestMessagesRef.current = props.messages;
  const closeMenu = () => setContextMenu(null);
  const closeReactionPicker = () => setReactionPicker(null);
  const scopeIsCurrent = (capturedScope: string) =>
    mountedRef.current && activeScopeRef.current === capturedScope;
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);
  useEffect(() => {
    setContextMenu(null);
    setEditingId(null);
    setDraft("");
    setPendingDeleteId(null);
    setReactionPicker(null);
    reactionOperationTokensRef.current.clear();
  }, [scopeKey]);
  useEffect(() => {
    const onDocClick = () => setContextMenu(null);
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setContextMenu(null);
        setReactionPicker(null);
      }
    };
    document.addEventListener("click", onDocClick);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("click", onDocClick);
      document.removeEventListener("keydown", onKey);
    };
  }, []);
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
    const id = pendingDeleteId;
    if (id === null) return;
    const capturedScope = scopeKey;
    try {
      await deleteMessage(id);
    } catch (e) {
      console.error("failed to delete message", e);
    }
    if (!scopeIsCurrent(capturedScope)) return;
    setPendingDeleteId(null);
    if (editingId === id) cancelEditing();
  };
  const suppressEmbeds = async (msg: Message) => {
    try {
      await setMessageEmbedsSuppressed(msg.id, true);
    } catch (e) {
      console.error("failed to suppress embeds", e);
    }
  };
  const saveEdit = async (msg: Message) => {
    const capturedScope = scopeKey;
    const next = draft;
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
      if (!scopeIsCurrent(capturedScope)) return;
      props.onMentionUsers?.(updated?.mentions ?? []);
      props.onMessageUpdated?.(updated);
    } catch (e) {
      console.error("failed to edit message", e);
    }
    if (scopeIsCurrent(capturedScope)) cancelEditing();
  };
  const mutateReaction = async (
    msg: Message,
    reaction: ReactionRequest,
    mutation: "add" | "remove",
  ) => {
    const capturedScope = scopeKey;
    const previous = msg.reactions ?? [];
    const optimistic = applyOptimisticReaction(previous, reaction, mutation);
    const operationToken = (reactionOperationTokensRef.current.get(msg.id) ?? 0) + 1;
    reactionOperationTokensRef.current.set(msg.id, operationToken);
    const canApplyHttpResult = () => {
      if (!scopeIsCurrent(capturedScope)) return false;
      if (reactionOperationTokensRef.current.get(msg.id) !== operationToken) return false;
      const currentMessage = latestMessagesRef.current.find((message) => message.id === msg.id);
      if (!currentMessage) return false;
      const current = currentMessage.reactions ?? [];
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
    void mutateReaction(
      msg,
      {
        kind: "native",
        emoji,
      },
      "add",
    );
  };
  const toggleReaction = (msg: Message, reaction: ReactionSummary) => {
    const request: ReactionRequest =
      reaction.kind === "native"
        ? {
            kind: "native",
            emoji: reaction.emoji,
          }
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
    targetId != null && visibleMessageIds.has(targetId);
  const jumpToReferencedMessage = (targetId: number | null | undefined) => {
    if (targetId == null) return;
    document.getElementById(channelMessageElementId(targetId))?.scrollIntoView({
      block: "center",
      inline: "nearest",
    });
  };
  const handleContextMenu = (e: ReactMouseEvent, msg: Message) => {
    if (!isOwnMessage(msg)) return;
    e.preventDefault();
    setContextMenu({
      messageId: msg.id,
      x: e.clientX,
      y: e.clientY,
    });
  };
  const currentContextMenu = contextMenu;

  return (
    <section className="bg-background text-foreground p-8 min-h-full flex flex-1 flex-col">
      {/* Bottom-anchor short histories without flex-end clipping overflowing histories. */}
      <div className="mt-auto" aria-hidden="true" />
      {props.error && props.messages.length === 0 ? (
        <span role="alert">Error getting messages: {String(props.error as string)}</span>
      ) : props.loading && props.messages.length === 0 ? (
        <p>Loading...</p>
      ) : (
        <>
          {props.error ? (
            <p role="alert" className="mb-2 text-sm text-destructive">
              Error refreshing messages: {String(props.error as string)}
            </p>
          ) : null}
          {props.messages.map((message) => (
            <div
              key={message.id}
              id={channelMessageElementId(message.id)}
              data-message-id={String(message.id)}
              data-authored-by-current-user={isOwnMessage(message) ? "true" : undefined}
              data-mentioned-current-user={isMentionedCurrentUser(message) ? "true" : undefined}
              className={messageRowClass(isOwnMessage(message), isMentionedCurrentUser(message))}
              onContextMenu={(e) => handleContextMenu(e, message)}
            >
              <Avatar
                url={isDeletedMessage(message) ? null : message.avatar_url}
                username={
                  isDeletedMessage(message) ? "Deleted message" : messageDisplayName(message)
                }
                size={32}
              />

              <div className="min-w-0 flex-1">
                {!isDeletedMessage(message) ? (
                  <>
                    <div className="font-bold">{messageDisplayName(message)}</div>
                    {(message.reply_to ?? message.reply_to_message_id ?? null) ? (
                      <>
                        <MessageReferencePreview
                          reference={message.reply_to ?? null}
                          targetId={referencedTargetId(message)}
                          onActivate={
                            canJumpToReferencedMessage(referencedTargetId(message))
                              ? () => jumpToReferencedMessage(referencedTargetId(message))
                              : undefined
                          }
                        />
                      </>
                    ) : null}
                    {editingId === message.id ? (
                      <>
                        <form
                          className="flex gap-2 items-center"
                          onSubmit={(e) => {
                            e.preventDefault();
                            void saveEdit(message);
                          }}
                        >
                          <MessageInput
                            value={draft}
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
                            inputClass="bg-muted rounded-md px-2 py-1 w-full"
                            emojiButtonClass="cursor-pointer rounded-md bg-muted px-2 py-1 text-muted-foreground hover:bg-accent hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                            emojiButtonLabel="Open emoji picker for edit"
                          />

                          <button type="submit" className="text-sm text-primary">
                            Save
                          </button>
                          <button
                            type="button"
                            className="text-sm text-muted-foreground"
                            onClick={cancelEditing}
                          >
                            Cancel
                          </button>
                        </form>
                      </>
                    ) : (
                      <MessageText
                        text={message.text}
                        mentions={message.mentions ?? []}
                        currentUserId={props.currentUserId}
                      />
                    )}
                  </>
                ) : (
                  <p className="italic text-muted-foreground" aria-label="Original message deleted">
                    Original message deleted
                  </p>
                )}
                {!isDeletedMessage(message) && message.attachments.length > 0 ? (
                  <>
                    <AttachmentGrid
                      attachments={message.attachments}
                      authorName={messageDisplayName(message)}
                    />
                  </>
                ) : null}
                {!isDeletedMessage(message) &&
                !message.suppress_embeds &&
                message.embeds.length > 0 ? (
                  <>
                    <div className="flex flex-col gap-1">
                      {message.embeds.map((embed) => (
                        <MessageEmbed
                          key={embed.id}
                          embed={embed}
                          onRemove={
                            isOwnMessage(message) ? () => void suppressEmbeds(message) : undefined
                          }
                        />
                      ))}
                    </div>
                  </>
                ) : null}
                {!isDeletedMessage(message) ? (
                  <>
                    <ReactionRow
                      reactions={message.reactions ?? []}
                      onToggle={(reaction) => toggleReaction(message, reaction)}
                    />
                  </>
                ) : null}
                {message.thread_summary?.reply_count ? (
                  <button
                    type="button"
                    className="mt-1 text-sm font-medium text-primary hover:underline focus:outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-md"
                    title={`Last reply ${formatThreadTimestampTitle(message.thread_summary.last_reply_created_at)}`}
                    aria-label={`Open thread with ${replyCountLabel(message.thread_summary.reply_count)}, last reply ${formatThreadTimestamp(message.thread_summary.last_reply_created_at)}`}
                    onClick={() => props.onOpenThread?.(message, { focusComposer: false })}
                  >
                    {replyCountLabel(message.thread_summary.reply_count)} · Last reply{" "}
                    {formatThreadTimestamp(message.thread_summary.last_reply_created_at)}
                  </button>
                ) : null}
              </div>
              {hasAnyAction(message) && editingId !== message.id ? (
                <>
                  <div
                    role="toolbar"
                    aria-label={`Message actions for ${referencedMessageLabel(message)}`}
                    className="absolute -top-3 right-2 flex gap-1 rounded-md border border-border bg-card shadow-sm opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity"
                  >
                    {canReact(message) ? (
                      <>
                        <button
                          type="button"
                          aria-label={`Add reaction to message by ${messageDisplayName(message)}`}
                          title="Add reaction"
                          className="p-1.5 rounded-md text-muted-foreground hover:bg-accent hover:text-foreground transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                          onClick={(event) => {
                            event.stopPropagation();
                            setReactionPicker({
                              messageId: message.id,
                              anchor: event.currentTarget,
                            });
                          }}
                        >
                          <EmojiIcon size={14} />
                        </button>
                      </>
                    ) : null}
                    {canInlineReply(message) ? (
                      <>
                        <button
                          type="button"
                          aria-label={`Reply inline to ${referencedMessageLabel(message)}`}
                          title="Reply inline"
                          className="rounded-md bg-primary/10 px-2 py-1 text-xs font-medium text-primary hover:bg-primary/20 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                          onClick={() => props.onReplyToMessage?.(message)}
                        >
                          Reply
                        </button>
                      </>
                    ) : null}
                    {canOpenThread(message) ? (
                      <>
                        <button
                          type="button"
                          aria-label={`Reply in thread to ${referencedMessageLabel(message)}`}
                          title="Reply in thread"
                          className="rounded-md bg-purple-50 px-2 py-1 text-xs font-medium text-purple-700 hover:bg-purple-100 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                          onClick={() =>
                            props.onOpenThread?.(message, {
                              focusComposer: true,
                            })
                          }
                        >
                          Thread
                        </button>
                      </>
                    ) : null}
                    {isOwnMessage(message) ? (
                      <>
                        <button
                          type="button"
                          aria-label="Edit"
                          title="Edit"
                          className="p-1.5 rounded-md text-muted-foreground hover:bg-accent hover:text-foreground transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                          onClick={() => startEditing(message)}
                        >
                          <EditIcon size={14} />
                        </button>
                        <button
                          type="button"
                          aria-label="Delete"
                          title="Delete"
                          className="p-1.5 rounded-md text-destructive hover:bg-destructive/10 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                          onClick={() => requestDelete(message.id)}
                        >
                          <DeleteIcon size={14} />
                        </button>
                      </>
                    ) : null}
                  </div>
                </>
              ) : null}
            </div>
          ))}
        </>
      )}

      <EmojiPicker
        open={reactionPicker !== null}
        anchor={() => reactionPicker?.anchor}
        emojis={reactionEmojiEntries}
        onSelect={(emoji) => {
          const state = reactionPicker;
          const msg = state
            ? props.messages.find((message) => message.id === state.messageId)
            : null;
          if (msg) addReactionFromPicker(msg, emoji);
        }}
        onClose={closeReactionPicker}
      />

      {currentContextMenu ? (
        <ul
          role="menu"
          className="fixed z-50 bg-popover text-popover-foreground border border-border rounded-md shadow-md py-1"
          style={{ top: `${currentContextMenu.y}px`, left: `${currentContextMenu.x}px` }}
          onClick={(e) => e.stopPropagation()}
        >
          <li>
            <button
              role="menuitem"
              type="button"
              className="w-full text-left px-4 py-1 transition-colors hover:bg-accent hover:text-accent-foreground"
              onClick={() => {
                const msg = props.messages.find((m) => m.id === currentContextMenu.messageId);
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
              className="w-full text-left px-4 py-1 text-destructive transition-colors hover:bg-destructive/10"
              onClick={() => requestDelete(currentContextMenu.messageId)}
            >
              Delete message
            </button>
          </li>
        </ul>
      ) : null}

      <Modal open={pendingDeleteId !== null} onClose={cancelDelete} title="Delete message?">
        <p className="text-sm text-muted-foreground mb-4">
          This will permanently delete the message. This cannot be undone.
        </p>
        <div className="flex gap-2 justify-end">
          <Button type="button" variant="ghost" onClick={cancelDelete}>
            Cancel
          </Button>
          <Button type="button" variant="destructive" onClick={() => void confirmDelete()}>
            Delete
          </Button>
        </div>
      </Modal>
    </section>
  );
}
export default ChannelMessages;
