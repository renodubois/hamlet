import { useEffect, useId, useRef } from "react";
import {
  useAfterRenderEffect,
  useCallableResource,
  useSignalState,
  registerCleanup,
  useMountEffect,
} from "../hooks/react-state";
import {
  addMessageReaction,
  deleteMessage,
  editMessage,
  getThread,
  messageDisplayName,
  messageReferenceFromMessage,
  messageReferencesTarget,
  removeMessageReaction,
  sendThreadReply,
  setMessageEmbedsSuppressed,
  type Message,
  type PublicUser,
  type ReactionRequest,
  type ReactionSummary,
  type SearchUsersOptions,
} from "../api";
import { useOptionalCustomEmojis } from "../contexts/custom-emojis";
import { useEvents } from "../contexts/events";
import { customEmojisToEntries, parseCustomEmojiMarkers } from "../emoji/custom-emojis";
import { CONSERVATIVE_EMOJIS } from "../emoji/emoji-data";
import { messageMentionsCurrentUser } from "../mentions/mentions";
import {
  applyOptimisticReaction,
  mergeReactionUpdateForViewer,
  reactionSummariesEqual,
} from "../reactions/reaction-summaries";
import AttachmentGrid from "./attachment-grid";
import Avatar from "./avatar";
import {
  PhotoAttachControl,
  SelectedPhotoPreviewList,
  useComposerPhotoSelection,
} from "./composer-photo-selection";
import EmojiPicker from "./emoji-picker";
import { DeleteIcon, EditIcon, EmojiIcon } from "./icons";
import MessageEmbed from "./message-embed";
import MessageInput from "./message-input";
import MessageReferencePreview from "./message-reference-preview";
import MessageText from "./message-text";
import Modal from "./modal";
import ReactionRow from "./reaction-row";
interface ReactionPickerState {
  messageId: number;
  anchor: HTMLElement;
}
function isDeletedMessage(message: Message): boolean {
  return message.deleted_at != null;
}
function threadMessageClass(authoredByCurrentUser: boolean, mentionedCurrentUser: boolean): string {
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
  return `group relative flex items-start gap-3 rounded-md border-l-4 py-2 pl-2 pr-12 transition-colors ${borderClass} ${stateClass}`;
}
function MessageBody(props: { message: Message; currentUserId: number | null }) {
  return !isDeletedMessage(props.message) ? (
    <>
      <MessageText
        text={props.message.text}
        mentions={props.message.mentions ?? []}
        currentUserId={props.currentUserId}
      />
    </>
  ) : (
    <p className="italic text-muted-foreground" aria-label="Original message deleted">
      Original message deleted
    </p>
  );
}
function ThreadMessage(props: {
  message: Message;
  currentUserId: number | null;
  currentUserName?: string | null;
  isRoot?: boolean;
  editing: boolean;
  draft: string;
  onDraftChange: (value: string) => void;
  onStartEdit: (message: Message) => void;
  onCancelEdit: () => void;
  onSaveEdit: (message: Message) => void;
  onRequestDelete: (message: Message) => void;
  onSuppressEmbeds: (message: Message) => void;
  onToggleReaction: (message: Message, reaction: ReactionSummary) => void;
  onOpenReactionPicker: (message: Message, anchor: HTMLElement) => void;
  mentionUsers?: readonly PublicUser[];
  onMentionUsers?: (users: readonly PublicUser[]) => void;
  searchMentionUsers?: (options: SearchUsersOptions) => Promise<PublicUser[]>;
  mentionSearchLimit?: number;
}) {
  const canReact = () => !isDeletedMessage(props.message) && props.currentUserId !== null;
  const isOwnMessage = () =>
    !isDeletedMessage(props.message) &&
    props.currentUserId !== null &&
    props.message.user_id === props.currentUserId;
  const isOwnReply = () => isOwnMessage() && props.message.parent_id != null;
  const isMentionedCurrentUser = () =>
    messageMentionsCurrentUser(props.message, props.currentUserId);
  const actionAuthorName = () =>
    props.currentUserId !== null &&
    props.message.user_id === props.currentUserId &&
    props.currentUserName
      ? props.currentUserName
      : messageDisplayName(props.message);
  return (
    <article
      data-message-id={String(props.message.id)}
      data-authored-by-current-user={isOwnMessage() ? "true" : undefined}
      data-mentioned-current-user={isMentionedCurrentUser() ? "true" : undefined}
      className={threadMessageClass(isOwnMessage(), isMentionedCurrentUser())}
    >
      <Avatar
        url={isDeletedMessage(props.message) ? null : props.message.avatar_url}
        username={
          isDeletedMessage(props.message) ? "Deleted message" : messageDisplayName(props.message)
        }
        size={32}
      />
      <div className="min-w-0 flex-1">
        {!isDeletedMessage(props.message) ? (
          <>
            <div className="font-bold">{messageDisplayName(props.message)}</div>
            {(props.message.reply_to ?? props.message.reply_to_message_id ?? null) ? (
              <>
                <MessageReferencePreview
                  reference={props.message.reply_to ?? null}
                  targetId={props.message.reply_to_message_id ?? props.message.reply_to?.id}
                />
              </>
            ) : null}
          </>
        ) : null}
        {props.editing ? (
          <>
            <form
              className="flex gap-2 items-center"
              onSubmit={(e) => {
                e.preventDefault();
                props.onSaveEdit(props.message);
              }}
            >
              <MessageInput
                value={props.draft}
                onChange={props.onDraftChange}
                ariaLabel="Edit reply"
                mentionUsers={props.mentionUsers}
                onMentionUsers={props.onMentionUsers}
                searchMentionUsers={props.searchMentionUsers}
                mentionSearchLimit={props.mentionSearchLimit}
                inputRef={(el) => queueMicrotask(() => el.focus())}
                onKeyDown={(e) => {
                  if (e.key === "Escape") {
                    e.preventDefault();
                    props.onCancelEdit();
                  }
                }}
                className="flex min-w-0 flex-1 items-center gap-2"
                inputClass="w-full rounded-md border border-input bg-transparent px-2 py-1 transition-colors focus:outline-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring"
                emojiButtonClass="cursor-pointer rounded-md bg-muted px-2 py-1 text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                emojiButtonLabel="Open emoji picker for reply edit"
              />
              <button type="submit" className="text-sm text-primary">
                Save
              </button>
              <button
                type="button"
                className="text-sm text-muted-foreground"
                onClick={props.onCancelEdit}
              >
                Cancel
              </button>
            </form>
          </>
        ) : (
          <MessageBody message={props.message} currentUserId={props.currentUserId} />
        )}
        {!isDeletedMessage(props.message) && props.message.attachments.length > 0 ? (
          <>
            <AttachmentGrid
              attachments={props.message.attachments}
              authorName={messageDisplayName(props.message)}
            />
          </>
        ) : null}
        {!isDeletedMessage(props.message) &&
        !props.message.suppress_embeds &&
        props.message.embeds.length > 0 ? (
          <>
            <div className="mt-1 flex flex-col gap-1">
              {props.message.embeds.map((embed) => (
                <MessageEmbed
                  key={embed.id}
                  embed={embed}
                  onRemove={isOwnReply() ? () => props.onSuppressEmbeds(props.message) : undefined}
                />
              ))}
            </div>
          </>
        ) : null}
        {!isDeletedMessage(props.message) ? (
          <>
            <ReactionRow
              reactions={props.message.reactions ?? []}
              onToggle={(reaction) => props.onToggleReaction(props.message, reaction)}
            />
          </>
        ) : null}
      </div>
      {(canReact() || isOwnReply()) && !props.editing ? (
        <>
          <div
            role="toolbar"
            aria-label="Thread message actions"
            className="absolute right-1 top-1 flex gap-1 rounded-md border border-border bg-card shadow-sm opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity"
          >
            {canReact() ? (
              <>
                <button
                  type="button"
                  aria-label={
                    props.isRoot
                      ? `Add reaction to thread root by ${actionAuthorName()}`
                      : `Add reaction to message by ${actionAuthorName()}`
                  }
                  title="Add reaction"
                  className="p-1.5 rounded-md transition-colors hover:bg-accent"
                  onClick={(event) => {
                    event.stopPropagation();
                    props.onOpenReactionPicker(props.message, event.currentTarget);
                  }}
                >
                  <EmojiIcon size={14} />
                </button>
              </>
            ) : null}
            {isOwnReply() ? (
              <>
                <button
                  type="button"
                  aria-label="Edit reply"
                  title="Edit reply"
                  className="p-1.5 rounded-md transition-colors hover:bg-accent"
                  onClick={() => props.onStartEdit(props.message)}
                >
                  <EditIcon size={14} />
                </button>
                <button
                  type="button"
                  aria-label="Delete reply"
                  title="Delete reply"
                  className="p-1.5 rounded-md text-destructive transition-colors hover:bg-destructive/10"
                  onClick={() => props.onRequestDelete(props.message)}
                >
                  <DeleteIcon size={14} />
                </button>
              </>
            ) : null}
          </div>
        </>
      ) : null}
    </article>
  );
}
export default function ThreadPanel(props: {
  rootMessageId: number;
  channelId: number;
  currentUserId: number | null;
  currentUserName?: string | null;
  onClose: () => void;
  focusComposer?: boolean;
  onComposerFocusConsumed?: () => void;
  mentionUsers?: readonly PublicUser[];
  onMentionUsers?: (users: readonly PublicUser[]) => void;
  searchMentionUsers?: (options: SearchUsersOptions) => Promise<PublicUser[]>;
  mentionSearchLimit?: number;
}) {
  const customEmojis = useOptionalCustomEmojis();
  const activeCustomEmojis = customEmojis?.activeEmojis ?? [];
  const reactionEmojiEntries = () => [
    ...CONSERVATIVE_EMOJIS,
    ...customEmojisToEntries(activeCustomEmojis),
  ];
  const customEmojiById = (id: number) => customEmojis?.byId(id) ?? null;
  const [thread, { mutate }] = useCallableResource(
    () => props.rootMessageId,
    (rootMessageId) => getThread(rootMessageId),
  );
  const [draft, setDraft] = useSignalState("");
  const [submitting, setSubmitting] = useSignalState(false);
  const photoSelection = useComposerPhotoSelection();
  const photoSelectionErrorId = useId();
  const [loadingOlder, setLoadingOlder] = useSignalState(false);
  const [error, setError] = useSignalState<string | null>(null);
  const [olderError, setOlderError] = useSignalState<string | null>(null);
  const [editingId, setEditingId] = useSignalState<number | null>(null);
  const [editDraft, setEditDraft] = useSignalState("");
  const [pendingDeleteId, setPendingDeleteId] = useSignalState<number | null>(null);
  const [reactionPicker, setReactionPicker] = useSignalState<ReactionPickerState | null>(null);
  const events = useEvents();
  const inputRef = useRef<
    | (HTMLElement & {
        value?: string;
      })
    | null
  >(null);
  const repliesScrollRef = useRef<HTMLDivElement | null>(null);
  const propsRef = useRef(props);
  propsRef.current = props;
  const mountedRef = useRef(false);
  const draftVersionRef = useRef(0);
  const selectedPhotoIdsRef = useRef(photoSelection.photos.map((photo) => photo.id));
  selectedPhotoIdsRef.current = photoSelection.photos.map((photo) => photo.id);
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);
  const appendReply = (reply: Message) => {
    props.onMentionUsers?.(reply.mentions ?? []);
    mutate((current) => {
      if (!current) return current;
      if (current.replies.some((existing) => existing.id === reply.id)) return current;
      return {
        ...current,
        replies: [...current.replies, reply],
      };
    });
  };
  const updateMessageInThread = (message: Message) => {
    props.onMentionUsers?.(message.mentions ?? []);
    mutate((current) => {
      if (!current) return current;
      const reference = messageReferenceFromMessage(message);
      const patchReference = (candidate: Message) =>
        messageReferencesTarget(candidate, message.id)
          ? {
              ...candidate,
              reply_to_message_id: message.id,
              reply_to: reference,
            }
          : candidate;
      const root = current.root.id === message.id ? message : patchReference(current.root);
      return {
        ...current,
        root,
        replies: current.replies.map((reply) =>
          reply.id === message.id ? message : patchReference(reply),
        ),
      };
    });
  };
  const markReferencedMessageUnavailable = (messageId: number) => {
    mutate((current) => {
      if (!current) return current;
      const patchReference = (candidate: Message) =>
        messageReferencesTarget(candidate, messageId)
          ? {
              ...candidate,
              reply_to_message_id: messageId,
              reply_to: null,
            }
          : candidate;
      return {
        ...current,
        root: patchReference(current.root),
        replies: current.replies.map((reply) => patchReference(reply)),
      };
    });
  };
  const removeReply = (replyId: number) => {
    mutate((current) => {
      if (!current) return current;
      return {
        ...current,
        replies: current.replies.filter((reply) => reply.id !== replyId),
      };
    });
  };
  const updateMessageReactions = (messageId: number, reactions: ReactionSummary[]) => {
    mutate((current) => {
      if (!current) return current;
      if (current.root.id === messageId) {
        return {
          ...current,
          root: {
            ...current.root,
            reactions,
          },
        };
      }
      return {
        ...current,
        replies: current.replies.map((reply) =>
          reply.id === messageId
            ? {
                ...reply,
                reactions,
              }
            : reply,
        ),
      };
    });
  };
  const findThreadMessage = (messageId: number): Message | null => {
    const current = thread();
    if (!current) return null;
    if (current.root.id === messageId) return current.root;
    return current.replies.find((reply) => reply.id === messageId) ?? null;
  };
  const mutateReaction = async (
    message: Message,
    reaction: ReactionRequest,
    mutation: "add" | "remove",
  ) => {
    const previous = message.reactions ?? [];
    const optimistic = applyOptimisticReaction(previous, reaction, mutation);
    const currentReactions = () => findThreadMessage(message.id)?.reactions ?? [];
    const canApplyHttpResult = () => {
      const current = currentReactions();
      return (
        reactionSummariesEqual(current, optimistic) || reactionSummariesEqual(current, previous)
      );
    };
    updateMessageReactions(message.id, optimistic);
    try {
      const canonical =
        mutation === "add"
          ? await addMessageReaction(message.id, reaction)
          : await removeMessageReaction(message.id, reaction);
      if (canApplyHttpResult()) {
        updateMessageReactions(message.id, canonical);
      }
    } catch (e) {
      if (canApplyHttpResult()) {
        updateMessageReactions(message.id, previous);
      }
      console.error("failed to update reaction", e);
    }
  };
  const addReactionFromPicker = (message: Message, emoji: string) => {
    setReactionPicker(null);
    const customToken = parseCustomEmojiMarkers(emoji)[0];
    if (customToken?.type === "custom-emoji" && customToken.marker === emoji) {
      const customEmoji = customEmojiById(customToken.id);
      if (!customEmoji || customEmoji.deleted_at !== null) return;
      void mutateReaction(
        message,
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
      message,
      {
        kind: "native",
        emoji,
      },
      "add",
    );
  };
  const toggleReaction = (message: Message, reaction: ReactionSummary) => {
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
    void mutateReaction(message, request, reaction.me_reacted ? "remove" : "add");
  };
  useMountEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setReactionPicker(null);
    };
    document.addEventListener("keydown", onKey);
    const unsubscribeCreated = events.onThreadReplyCreated((event) => {
      if (
        event.channel_id !== propsRef.current.channelId ||
        event.root_message_id !== propsRef.current.rootMessageId
      ) {
        return;
      }
      appendReply(event.reply);
    });
    const unsubscribeDeleted = events.onThreadReplyDeleted((event) => {
      if (
        event.channel_id !== propsRef.current.channelId ||
        event.root_message_id !== propsRef.current.rootMessageId
      ) {
        return;
      }
      removeReply(event.reply_id);
    });
    const unsubscribeUpdated = events.onMessageUpdated((message) => {
      if (message.channel_id !== propsRef.current.channelId) return;
      updateMessageInThread(message);
    });
    const unsubscribeMessageDeleted = events.onMessageDeleted((event) => {
      if (event.channel_id !== propsRef.current.channelId) return;
      markReferencedMessageUnavailable(event.id);
    });
    const unsubscribeEmbeds = events.onMessageEmbedsUpdated((event) => {
      if (event.channel_id !== propsRef.current.channelId) return;
      mutate((current) => {
        if (!current) return current;
        if (current.root.id === event.id) {
          return {
            ...current,
            root: {
              ...current.root,
              suppress_embeds: event.suppress_embeds,
              embeds: event.embeds,
            },
          };
        }
        return {
          ...current,
          replies: current.replies.map((reply) =>
            reply.id === event.id
              ? {
                  ...reply,
                  suppress_embeds: event.suppress_embeds,
                  embeds: event.embeds,
                }
              : reply,
          ),
        };
      });
    });
    const unsubscribeReactions = events.onMessageReactionsUpdated((event) => {
      if (event.channel_id !== propsRef.current.channelId) return;
      mutate((current) => {
        if (!current) return current;
        const eventRootId = event.root_message_id ?? event.parent_id ?? event.id;
        if (eventRootId !== propsRef.current.rootMessageId && event.id !== current.root.id) {
          return current;
        }
        if (current.root.id === event.id) {
          return {
            ...current,
            root: {
              ...current.root,
              reactions: mergeReactionUpdateForViewer(
                current.root.reactions ?? [],
                event.reactions,
                event.user_id,
                propsRef.current.currentUserId,
              ),
            },
          };
        }
        return {
          ...current,
          replies: current.replies.map((reply) =>
            reply.id === event.id
              ? {
                  ...reply,
                  reactions: mergeReactionUpdateForViewer(
                    reply.reactions ?? [],
                    event.reactions,
                    event.user_id,
                    propsRef.current.currentUserId,
                  ),
                }
              : reply,
          ),
        };
      });
    });
    registerCleanup(() => {
      document.removeEventListener("keydown", onKey);
      unsubscribeCreated();
      unsubscribeDeleted();
      unsubscribeUpdated();
      unsubscribeMessageDeleted();
      unsubscribeEmbeds();
      unsubscribeReactions();
    });
  });
  useAfterRenderEffect(() => {
    const rootMessageId = props.rootMessageId;
    if (props.focusComposer && rootMessageId > 0) {
      queueMicrotask(() => {
        inputRef.current?.focus();
        props.onComposerFocusConsumed?.();
      });
    }
  });
  useAfterRenderEffect(() => {
    const loaded = thread();
    if (!loaded) return;
    props.onMentionUsers?.([
      ...(loaded.root.mentions ?? []),
      ...loaded.replies.flatMap((reply) => reply.mentions ?? []),
    ]);
  });
  const loadOlderReplies = async () => {
    const current = thread();
    const oldestReply = current?.replies[0];
    if (
      !current?.has_more_replies ||
      !oldestReply ||
      oldestReply.created_at === undefined ||
      loadingOlder()
    ) {
      return;
    }
    setLoadingOlder(true);
    setOlderError(null);
    const previousScrollHeight = repliesScrollRef.current?.scrollHeight ?? 0;
    try {
      const olderPage = await getThread(props.rootMessageId, {
        beforeCreatedAt: oldestReply.created_at,
        beforeId: oldestReply.id,
      });
      mutate((latest) => {
        if (!latest) return latest;
        const existingIds = new Set(latest.replies.map((reply) => reply.id));
        const olderReplies = olderPage.replies.filter((reply) => !existingIds.has(reply.id));
        return {
          ...latest,
          replies: [...olderReplies, ...latest.replies],
          has_more_replies: olderPage.has_more_replies,
        };
      });
      queueMicrotask(() => {
        if (!repliesScrollRef.current) return;
        repliesScrollRef.current.scrollTop +=
          repliesScrollRef.current.scrollHeight - previousScrollHeight;
      });
    } catch (e) {
      setOlderError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoadingOlder(false);
    }
  };
  const draftText = () => draft() || inputRef.current?.value || inputRef.current?.textContent || "";
  const hasDraftContent = () => draftText().trim().length > 0 || photoSelection.photos.length > 0;
  const handleDraftChange = (value: string) => {
    draftVersionRef.current += 1;
    setDraft(value);
  };
  const submitReply = async () => {
    const submittedChannelId = props.channelId;
    const submittedRootMessageId = props.rootMessageId;
    const text = draftText();
    const submittedDraftVersion = draftVersionRef.current;
    const submittedPhotos = photoSelection.photos;
    const submittedPhotoIds = submittedPhotos.map((photo) => photo.id);
    const photos = submittedPhotos.map((photo) => photo.file);
    if ((text.trim().length === 0 && photos.length === 0) || submitting()) return;
    setSubmitting(true);
    setError(null);
    setDraft("");
    const ownsSubmissionContext = () =>
      mountedRef.current &&
      propsRef.current.channelId === submittedChannelId &&
      propsRef.current.rootMessageId === submittedRootMessageId;
    try {
      const reply = await sendThreadReply(submittedRootMessageId, text, photos);
      if (!ownsSubmissionContext()) return;
      appendReply(reply);
      for (const photo of submittedPhotos) {
        photoSelection.removePhoto(photo.id);
      }
      queueMicrotask(() => inputRef.current?.focus());
    } catch (e) {
      const stillOwnsSubmittedPhotos =
        selectedPhotoIdsRef.current.length === submittedPhotoIds.length &&
        selectedPhotoIdsRef.current.every((id, index) => id === submittedPhotoIds[index]);
      if (
        !ownsSubmissionContext() ||
        draftVersionRef.current !== submittedDraftVersion ||
        !stillOwnsSubmittedPhotos
      ) {
        return;
      }
      setDraft(text);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      if (ownsSubmissionContext()) setSubmitting(false);
    }
  };
  const startEditing = (message: Message) => {
    props.onMentionUsers?.(message.mentions ?? []);
    setEditDraft(message.text);
    setEditingId(message.id);
  };
  const cancelEditing = () => {
    setEditingId(null);
    setEditDraft("");
  };
  const saveEdit = async (message: Message) => {
    const next = editDraft();
    if (next.length === 0 && message.attachments.length === 0) {
      setPendingDeleteId(message.id);
      return;
    }
    if (next === message.text) {
      cancelEditing();
      return;
    }
    try {
      const updated = await editMessage(message.id, next);
      updateMessageInThread(updated);
    } catch (e) {
      console.error("failed to edit thread reply", e);
    }
    cancelEditing();
  };
  const confirmDelete = async () => {
    const id = pendingDeleteId();
    if (id === null) return;
    try {
      await deleteMessage(id);
      removeReply(id);
    } catch (e) {
      console.error("failed to delete thread reply", e);
    }
    setPendingDeleteId(null);
    if (editingId() === id) cancelEditing();
  };
  const suppressEmbeds = async (message: Message) => {
    try {
      const updated = await setMessageEmbedsSuppressed(message.id, true);
      mutate((current) => {
        if (!current) return current;
        return {
          ...current,
          replies: current.replies.map((reply) =>
            reply.id === message.id
              ? {
                  ...reply,
                  suppress_embeds: updated.suppress_embeds,
                  embeds: updated.embeds,
                }
              : reply,
          ),
        };
      });
    } catch (e) {
      console.error("failed to suppress thread reply embeds", e);
    }
  };
  const loadedThread = thread();
  const currentOlderError = olderError();
  const currentError = error();

  return (
    <aside
      className="w-96 max-w-[45vw] min-h-0 flex-shrink-0 border-l border-border bg-background text-foreground flex flex-col"
      aria-label="Thread panel"
    >
      <header className="flex items-center justify-between border-b border-border p-4">
        <h2 className="font-bold text-lg">Thread</h2>
        <button
          type="button"
          className="rounded-md px-2 py-1 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
          aria-label="Close thread"
          onClick={props.onClose}
        >
          Close
        </button>
      </header>

      <div
        className="min-h-0 flex-1 overflow-y-auto p-4"
        ref={(el) => {
          repliesScrollRef.current = el;
        }}
      >
        {thread.error ? (
          <p role="alert" className="text-destructive">
            Error loading thread: {String(thread.error)}
          </p>
        ) : thread.loading ? (
          <p>Loading thread...</p>
        ) : loadedThread ? (
          <div className="flex flex-col gap-2">
            <ThreadMessage
              message={loadedThread.root}
              currentUserId={props.currentUserId}
              currentUserName={props.currentUserName}
              isRoot={true}
              editing={false}
              draft=""
              onDraftChange={() => undefined}
              onStartEdit={startEditing}
              onCancelEdit={cancelEditing}
              onSaveEdit={saveEdit}
              onRequestDelete={(message) => setPendingDeleteId(message.id)}
              onSuppressEmbeds={suppressEmbeds}
              onToggleReaction={toggleReaction}
              onOpenReactionPicker={(message, anchor) =>
                setReactionPicker({ messageId: message.id, anchor })
              }
            />
            {loadedThread.replies.length > 0 ? (
              <div className="border-t border-border pt-2">
                {loadedThread.has_more_replies ? (
                  <div className="pb-2 text-center">
                    <button
                      type="button"
                      className="rounded-md border border-border px-3 py-1 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground disabled:opacity-50"
                      onClick={() => void loadOlderReplies()}
                      disabled={loadingOlder()}
                    >
                      {loadingOlder() ? "Loading older replies..." : "Load older replies"}
                    </button>
                  </div>
                ) : null}
                {currentOlderError ? (
                  <p role="alert" className="pb-2 text-sm text-destructive">
                    Error loading older replies: {currentOlderError}
                  </p>
                ) : null}
                {loadedThread.replies.map((reply) => (
                  <ThreadMessage
                    key={reply.id}
                    message={reply}
                    currentUserId={props.currentUserId}
                    currentUserName={props.currentUserName}
                    editing={editingId() === reply.id}
                    draft={editDraft()}
                    onDraftChange={setEditDraft}
                    onStartEdit={startEditing}
                    onCancelEdit={cancelEditing}
                    onSaveEdit={saveEdit}
                    onRequestDelete={(message) => setPendingDeleteId(message.id)}
                    onSuppressEmbeds={suppressEmbeds}
                    onToggleReaction={toggleReaction}
                    onOpenReactionPicker={(message, anchor) =>
                      setReactionPicker({ messageId: message.id, anchor })
                    }
                    mentionUsers={props.mentionUsers}
                    onMentionUsers={props.onMentionUsers}
                    searchMentionUsers={props.searchMentionUsers}
                    mentionSearchLimit={props.mentionSearchLimit}
                  />
                ))}
              </div>
            ) : null}
          </div>
        ) : null}
      </div>

      <form
        className="flex-shrink-0 border-t border-border p-4"
        onSubmit={(e) => {
          e.preventDefault();
          void submitReply();
        }}
      >
        {currentError ? (
          <p role="alert" className="mb-2 text-sm text-destructive">
            {currentError}
          </p>
        ) : null}
        <SelectedPhotoPreviewList
          photos={photoSelection.photos}
          error={photoSelection.error}
          errorId={photoSelectionErrorId}
          disabled={submitting()}
          onRemove={photoSelection.removePhoto}
        />
        <div className="flex items-end gap-2">
          <PhotoAttachControl
            onFilesSelected={photoSelection.addFiles}
            disabled={submitting()}
            describedBy={photoSelection.error ? photoSelectionErrorId : undefined}
          />
          <MessageInput
            value={draft()}
            onChange={handleDraftChange}
            ariaLabel="Thread reply"
            placeholder="Reply in thread..."
            className="flex min-w-0 flex-1 items-end gap-2"
            inputRef={(el) => {
              inputRef.current = el;
            }}
            mentionUsers={props.mentionUsers}
            onMentionUsers={props.onMentionUsers}
            searchMentionUsers={props.searchMentionUsers}
            mentionSearchLimit={props.mentionSearchLimit}
          />
          <button
            className="rounded-md bg-primary/10 p-4 text-primary transition-colors hover:bg-primary/20 disabled:opacity-50"
            type="submit"
            disabled={submitting() || !hasDraftContent()}
          >
            Send
          </button>
        </div>
      </form>

      <EmojiPicker
        open={reactionPicker() !== null}
        anchor={() => reactionPicker()?.anchor}
        emojis={reactionEmojiEntries()}
        onSelect={(emoji) => {
          const state = reactionPicker();
          const message = state ? findThreadMessage(state.messageId) : null;
          if (message) addReactionFromPicker(message, emoji);
        }}
        onClose={() => setReactionPicker(null)}
      />

      <Modal
        open={pendingDeleteId() !== null}
        onClose={() => setPendingDeleteId(null)}
        title="Delete reply?"
      >
        <p className="text-sm text-muted-foreground mb-4">
          This will permanently delete the reply. This cannot be undone.
        </p>
        <div className="flex gap-2 justify-end">
          <button
            type="button"
            className="text-muted-foreground hover:text-foreground text-sm px-3 py-2 transition-colors"
            onClick={() => setPendingDeleteId(null)}
          >
            Cancel
          </button>
          <button
            type="button"
            className="bg-destructive hover:bg-destructive/90 text-white rounded-md px-4 py-2 text-sm font-medium transition-colors"
            onClick={() => void confirmDelete()}
          >
            Delete
          </button>
        </div>
      </Modal>
    </aside>
  );
}
