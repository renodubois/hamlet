import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useReducer,
  useRef,
  useState,
} from "react";
import {
  addMessageReaction,
  deleteMessage,
  editMessage,
  getThread,
  messageDisplayName,
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
  createThreadState,
  isValidThreadPayload,
  threadReducer,
  type ThreadLiveAction,
} from "../messages/thread-reducer";
import { applyOptimisticReaction, reactionSummariesEqual } from "../reactions/reaction-summaries";
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
type ThreadLivePayload = ThreadLiveAction extends infer Action
  ? Action extends { channelId: number; rootMessageId: number; generation: number }
    ? Omit<Action, "channelId" | "rootMessageId" | "generation">
    : never
  : never;
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
  const canReact = !isDeletedMessage(props.message) && props.currentUserId !== null;
  const isOwnMessage =
    !isDeletedMessage(props.message) &&
    props.currentUserId !== null &&
    props.message.user_id === props.currentUserId;
  const isOwnReply = isOwnMessage && props.message.parent_id != null;
  const isMentionedCurrentUser = messageMentionsCurrentUser(props.message, props.currentUserId);
  const actionAuthorName =
    props.currentUserId !== null &&
    props.message.user_id === props.currentUserId &&
    props.currentUserName
      ? props.currentUserName
      : messageDisplayName(props.message);
  return (
    <article
      data-message-id={String(props.message.id)}
      data-authored-by-current-user={isOwnMessage ? "true" : undefined}
      data-mentioned-current-user={isMentionedCurrentUser ? "true" : undefined}
      className={threadMessageClass(isOwnMessage, isMentionedCurrentUser)}
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
                inputRef={(el) => {
                  if (el) queueMicrotask(() => el.isConnected && el.focus());
                }}
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
                  onRemove={isOwnReply ? () => props.onSuppressEmbeds(props.message) : undefined}
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
      {(canReact || isOwnReply) && !props.editing ? (
        <>
          <div
            role="toolbar"
            aria-label="Thread message actions"
            className="absolute right-1 top-1 flex gap-1 rounded-md border border-border bg-card shadow-sm opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity"
          >
            {canReact ? (
              <>
                <button
                  type="button"
                  aria-label={
                    props.isRoot
                      ? `Add reaction to thread root by ${actionAuthorName}`
                      : `Add reaction to message by ${actionAuthorName}`
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
            {isOwnReply ? (
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
  const events = useEvents();
  const [thread, dispatch] = useReducer(threadReducer, undefined, () =>
    createThreadState(props.channelId, props.rootMessageId),
  );
  const [draft, setDraft] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editDraft, setEditDraft] = useState("");
  const [pendingDeleteId, setPendingDeleteId] = useState<number | null>(null);
  const [reactionPicker, setReactionPicker] = useState<ReactionPickerState | null>(null);
  const photoSelection = useComposerPhotoSelection();
  const photoSelectionErrorId = useId();
  const inputRef = useRef<HTMLDivElement | null>(null);
  const repliesScrollRef = useRef<HTMLDivElement | null>(null);
  const threadRef = useRef(thread);
  threadRef.current = thread;
  const propsRef = useRef(props);
  propsRef.current = props;
  const mountedRef = useRef(false);
  const generationRef = useRef(0);
  const draftVersionRef = useRef(0);
  const initialControllerRef = useRef<AbortController | null>(null);
  const pageControllerRef = useRef<AbortController | null>(null);
  const operationControllersRef = useRef(new Set<AbortController>());
  const reactionOperationRef = useRef(new Map<number, number>());
  const pendingSendRef = useRef<{
    controller: AbortController;
    text: string;
    draftVersion: number;
  } | null>(null);
  const pendingScrollRef = useRef<{ generation: number; previousHeight: number } | null>(null);
  const pendingInitialScrollRef = useRef<number | null>(null);
  const didInitialScrollRef = useRef(false);
  const rootHardDeletedRef = useRef(false);

  const identityFor = useCallback(
    (generation: number) => ({
      channelId: props.channelId,
      rootMessageId: props.rootMessageId,
      generation,
    }),
    [props.channelId, props.rootMessageId],
  );
  const ownsOperation = useCallback((generation: number, controller: AbortController) => {
    const current = threadRef.current;
    return (
      mountedRef.current &&
      !controller.signal.aborted &&
      generationRef.current === generation &&
      current.generation === generation &&
      current.channelId === propsRef.current.channelId &&
      current.rootMessageId === propsRef.current.rootMessageId
    );
  }, []);
  const beginOperation = useCallback(() => {
    const controller = new AbortController();
    operationControllersRef.current.add(controller);
    return controller;
  }, []);
  const finishOperation = useCallback((controller: AbortController) => {
    operationControllersRef.current.delete(controller);
  }, []);
  const dispatchLive = useCallback((action: ThreadLivePayload) => {
    const current = threadRef.current;
    dispatch({
      ...action,
      channelId: current.channelId,
      rootMessageId: current.rootMessageId,
      generation: generationRef.current,
    } as ThreadLiveAction);
  }, []);

  const startInitialLoad = useCallback(() => {
    initialControllerRef.current?.abort();
    pageControllerRef.current?.abort();
    const pendingSend = pendingSendRef.current;
    if (pendingSend && draftVersionRef.current === pendingSend.draftVersion) {
      setDraft(pendingSend.text);
    }
    pendingSendRef.current = null;
    for (const operationController of operationControllersRef.current) {
      operationController.abort();
    }
    operationControllersRef.current.clear();
    reactionOperationRef.current.clear();
    setSubmitting(false);
    pageControllerRef.current = null;
    pendingScrollRef.current = null;
    const controller = new AbortController();
    initialControllerRef.current = controller;
    const generation = ++generationRef.current;
    const identity = identityFor(generation);
    const ownsInitialLoad = () =>
      mountedRef.current &&
      !controller.signal.aborted &&
      generationRef.current === generation &&
      propsRef.current.channelId === identity.channelId &&
      propsRef.current.rootMessageId === identity.rootMessageId;
    dispatch({ type: "initial-load-started", ...identity });
    void getThread(props.rootMessageId, {}, controller.signal).then(
      (loaded) => {
        if (!ownsInitialLoad()) return;
        if (!isValidThreadPayload(loaded, props.channelId, props.rootMessageId)) {
          dispatch({
            type: "initial-load-failed",
            ...identity,
            error: new Error("Invalid thread response"),
          });
          return;
        }
        if (!didInitialScrollRef.current) pendingInitialScrollRef.current = generation;
        dispatch({ type: "initial-load-succeeded", ...identity, thread: loaded });
      },
      (error: unknown) => {
        if (!ownsInitialLoad()) return;
        dispatch({ type: "initial-load-failed", ...identity, error });
      },
    );
  }, [identityFor, props.channelId, props.rootMessageId]);

  const abortOwnedOperations = useCallback(() => {
    initialControllerRef.current?.abort();
    pageControllerRef.current?.abort();
    for (const controller of operationControllersRef.current) controller.abort();
    operationControllersRef.current.clear();
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    startInitialLoad();
    return () => {
      mountedRef.current = false;
      abortOwnedOperations();
    };
  }, [abortOwnedOperations, startInitialLoad]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setReactionPicker(null);
    };
    document.addEventListener("keydown", onKey);
    const unsubscribers = [
      events.onThreadReplyCreated((event) => dispatchLive({ type: "reply-created", event })),
      events.onThreadReplyDeleted((event) => dispatchLive({ type: "reply-deleted", event })),
      events.onMessageUpdated((message) => dispatchLive({ type: "message-updated", message })),
      events.onMessageDeleted((event) => {
        const current = threadRef.current;
        if (event.channel_id !== current.channelId) return;
        if (event.id === current.rootMessageId) {
          if (rootHardDeletedRef.current) return;
          rootHardDeletedRef.current = true;
          dispatchLive({ type: "message-deleted", messageId: event.id });
          propsRef.current.onClose();
          return;
        }
        if (!rootHardDeletedRef.current) {
          dispatchLive({ type: "message-deleted", messageId: event.id });
        }
      }),
      events.onMessageEmbedsUpdated((event) => dispatchLive({ type: "embeds-updated", event })),
      events.onMessageReactionsUpdated((event) => {
        const current = threadRef.current;
        dispatch({
          type: "reactions-updated",
          event,
          currentUserId: propsRef.current.currentUserId,
          channelId: current.channelId,
          rootMessageId: current.rootMessageId,
          generation: generationRef.current,
        });
      }),
      events.onConnected(() => startInitialLoad()),
    ];
    return () => {
      document.removeEventListener("keydown", onKey);
      for (const unsubscribe of unsubscribers) unsubscribe();
    };
  }, [dispatchLive, events, startInitialLoad]);

  useEffect(() => {
    if (!props.focusComposer || props.rootMessageId <= 0) return;
    queueMicrotask(() => {
      inputRef.current?.focus();
      propsRef.current.onComposerFocusConsumed?.();
    });
  }, [props.focusComposer, props.rootMessageId]);

  useEffect(() => {
    if (!thread.root) return;
    propsRef.current.onMentionUsers?.([
      ...(thread.root.mentions ?? []),
      ...thread.replies.flatMap((reply) => reply.mentions ?? []),
    ]);
  }, [thread.root, thread.replies]);

  useLayoutEffect(() => {
    const element = repliesScrollRef.current;
    if (!element) return;
    const pending = pendingScrollRef.current;
    if (pending && pending.generation === thread.generation) {
      pendingScrollRef.current = null;
      element.scrollTop += element.scrollHeight - pending.previousHeight;
      return;
    }
    if (
      pendingInitialScrollRef.current === thread.generation &&
      thread.status === "ready" &&
      thread.root
    ) {
      pendingInitialScrollRef.current = null;
      didInitialScrollRef.current = true;
      element.scrollTop = element.scrollHeight;
    }
  }, [thread.generation, thread.olderStatus, thread.replies, thread.root, thread.status]);

  const findThreadMessage = (messageId: number): Message | null => {
    if (thread.root?.id === messageId) return thread.root;
    return thread.replies.find((reply) => reply.id === messageId) ?? null;
  };
  const updateMessage = (message: Message) => dispatchLive({ type: "message-updated", message });
  const updateReactions = (message: Message, reactions: ReactionSummary[]) =>
    updateMessage({ ...message, reactions });

  const mutateReaction = async (
    message: Message,
    reaction: ReactionRequest,
    mutation: "add" | "remove",
  ) => {
    const generation = thread.generation;
    const controller = beginOperation();
    const operation = (reactionOperationRef.current.get(message.id) ?? 0) + 1;
    reactionOperationRef.current.set(message.id, operation);
    const previous = message.reactions ?? [];
    const optimistic = applyOptimisticReaction(previous, reaction, mutation);
    updateReactions(message, optimistic);
    const canApply = () => {
      if (
        !ownsOperation(generation, controller) ||
        reactionOperationRef.current.get(message.id) !== operation
      )
        return false;
      const current =
        threadRef.current.root?.id === message.id
          ? (threadRef.current.root.reactions ?? [])
          : (threadRef.current.replies.find((reply) => reply.id === message.id)?.reactions ?? []);
      return (
        reactionSummariesEqual(current, optimistic) || reactionSummariesEqual(current, previous)
      );
    };
    try {
      const canonical =
        mutation === "add"
          ? await addMessageReaction(message.id, reaction, controller.signal)
          : await removeMessageReaction(message.id, reaction, controller.signal);
      if (canApply()) {
        const latest =
          threadRef.current.root?.id === message.id
            ? threadRef.current.root
            : threadRef.current.replies.find((reply) => reply.id === message.id);
        if (latest) updateReactions(latest, canonical);
      }
    } catch (error) {
      if (canApply()) {
        const latest =
          threadRef.current.root?.id === message.id
            ? threadRef.current.root
            : threadRef.current.replies.find((reply) => reply.id === message.id);
        if (latest) updateReactions(latest, previous);
      }
      if (!controller.signal.aborted) console.error("failed to update reaction", error);
    } finally {
      finishOperation(controller);
    }
  };

  const addReactionFromPicker = (message: Message, emoji: string) => {
    setReactionPicker(null);
    const customToken = parseCustomEmojiMarkers(emoji)[0];
    if (customToken?.type === "custom-emoji" && customToken.marker === emoji) {
      const customEmoji = customEmojis?.byId(customToken.id) ?? null;
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
    void mutateReaction(message, { kind: "native", emoji }, "add");
  };
  const toggleReaction = (message: Message, reaction: ReactionSummary) => {
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
    void mutateReaction(message, request, reaction.me_reacted ? "remove" : "add");
  };

  const loadOlderReplies = async () => {
    const oldest = thread.replies[0];
    if (
      !thread.hasMoreReplies ||
      !oldest ||
      oldest.created_at === undefined ||
      thread.olderStatus === "loading"
    )
      return;
    pageControllerRef.current?.abort();
    const controller = new AbortController();
    pageControllerRef.current = controller;
    const generation = thread.generation;
    const identity = identityFor(generation);
    dispatch({ type: "older-page-started", ...identity });
    const previousHeight = repliesScrollRef.current?.scrollHeight ?? 0;
    try {
      const page = await getThread(
        props.rootMessageId,
        { beforeCreatedAt: oldest.created_at, beforeId: oldest.id },
        controller.signal,
      );
      if (!ownsOperation(generation, controller)) return;
      if (!isValidThreadPayload(page, props.channelId, props.rootMessageId)) {
        dispatch({
          type: "older-page-failed",
          ...identity,
          error: new Error("Invalid thread response"),
        });
        return;
      }
      pendingScrollRef.current = { generation, previousHeight };
      dispatch({ type: "older-page-succeeded", ...identity, thread: page });
    } catch (error) {
      if (ownsOperation(generation, controller))
        dispatch({ type: "older-page-failed", ...identity, error });
    } finally {
      if (pageControllerRef.current === controller) pageControllerRef.current = null;
    }
  };

  const draftText = draft;
  const handleDraftChange = (value: string) => {
    draftVersionRef.current += 1;
    setDraft(value);
  };
  const submitReply = async () => {
    const text = draftText;
    const photos = photoSelection.photos;
    if ((text.trim().length === 0 && photos.length === 0) || submitting) return;
    const generation = thread.generation;
    const submittedDraftVersion = draftVersionRef.current;
    const controller = beginOperation();
    pendingSendRef.current = { controller, text, draftVersion: submittedDraftVersion };
    setSubmitting(true);
    setSendError(null);
    setDraft("");
    try {
      const reply = await sendThreadReply(
        props.rootMessageId,
        text,
        photos.map((photo) => photo.file),
        controller.signal,
      );
      if (!ownsOperation(generation, controller)) return;
      if (reply.channel_id !== props.channelId || reply.parent_id !== props.rootMessageId) {
        if (draftVersionRef.current === submittedDraftVersion) setDraft(text);
        setSendError("Invalid thread reply response");
        return;
      }
      dispatchLive({
        type: "reply-created",
        event: {
          channel_id: props.channelId,
          root_message_id: props.rootMessageId,
          reply,
          thread_summary: {
            reply_count: threadRef.current.replies.length + 1,
            last_reply_created_at: reply.created_at ?? reply.id,
          },
        },
      });
      if (draftVersionRef.current === submittedDraftVersion) setDraft("");
      for (const photo of photos) photoSelection.removePhoto(photo.id);
      queueMicrotask(() => inputRef.current?.focus());
    } catch (error) {
      if (
        ownsOperation(generation, controller) &&
        draftVersionRef.current === submittedDraftVersion
      ) {
        setDraft(text);
        setSendError(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (pendingSendRef.current?.controller === controller) pendingSendRef.current = null;
      if (ownsOperation(generation, controller)) setSubmitting(false);
      finishOperation(controller);
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
    const next = editDraft;
    if (next.length === 0 && message.attachments.length === 0) {
      setPendingDeleteId(message.id);
      return;
    }
    if (next === message.text) {
      cancelEditing();
      return;
    }
    const generation = thread.generation;
    const controller = beginOperation();
    try {
      const updated = await editMessage(message.id, next, controller.signal);
      if (ownsOperation(generation, controller)) updateMessage(updated);
    } catch (error) {
      if (!controller.signal.aborted) console.error("failed to edit thread reply", error);
    } finally {
      if (ownsOperation(generation, controller)) cancelEditing();
      finishOperation(controller);
    }
  };
  const confirmDelete = async () => {
    const id = pendingDeleteId;
    if (id === null) return;
    const generation = thread.generation;
    const controller = beginOperation();
    try {
      await deleteMessage(id, controller.signal);
      if (ownsOperation(generation, controller)) {
        dispatchLive({
          type: "reply-deleted",
          event: {
            channel_id: props.channelId,
            root_message_id: props.rootMessageId,
            reply_id: id,
          },
        });
      }
    } catch (error) {
      if (!controller.signal.aborted) console.error("failed to delete thread reply", error);
    } finally {
      if (ownsOperation(generation, controller)) {
        setPendingDeleteId(null);
        if (editingId === id) cancelEditing();
      }
      finishOperation(controller);
    }
  };
  const suppressEmbeds = async (message: Message) => {
    const generation = thread.generation;
    const controller = beginOperation();
    try {
      const updated = await setMessageEmbedsSuppressed(message.id, true, controller.signal);
      if (ownsOperation(generation, controller))
        dispatchLive({ type: "embeds-updated", event: updated });
    } catch (error) {
      if (!controller.signal.aborted)
        console.error("failed to suppress thread reply embeds", error);
    } finally {
      finishOperation(controller);
    }
  };

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
      <div className="min-h-0 flex-1 overflow-y-auto p-4" ref={repliesScrollRef}>
        {thread.status === "error" && !thread.root ? (
          <p role="alert" className="text-destructive">
            Error loading thread: {String(thread.error)}
          </p>
        ) : thread.status === "idle" || (thread.status === "loading" && !thread.root) ? (
          <p>Loading thread...</p>
        ) : thread.root ? (
          <div className="flex flex-col gap-2">
            {thread.status === "error" ? (
              <p role="alert" className="text-sm text-destructive">
                Error refreshing thread: {String(thread.error)}
              </p>
            ) : null}
            <ThreadMessage
              message={thread.root}
              currentUserId={props.currentUserId}
              currentUserName={props.currentUserName}
              isRoot
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
            {thread.replies.length > 0 ? (
              <div className="border-t border-border pt-2">
                {thread.hasMoreReplies ? (
                  <div className="pb-2 text-center">
                    <button
                      type="button"
                      className="rounded-md border border-border px-3 py-1 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground disabled:opacity-50"
                      onClick={() => void loadOlderReplies()}
                      disabled={thread.olderStatus === "loading"}
                    >
                      {thread.olderStatus === "loading"
                        ? "Loading older replies..."
                        : "Load older replies"}
                    </button>
                  </div>
                ) : null}
                {thread.olderStatus === "error" ? (
                  <p role="alert" className="pb-2 text-sm text-destructive">
                    Error loading older replies: {String(thread.olderError)}
                  </p>
                ) : null}
                {thread.replies.map((reply) => (
                  <ThreadMessage
                    key={reply.id}
                    message={reply}
                    currentUserId={props.currentUserId}
                    currentUserName={props.currentUserName}
                    editing={editingId === reply.id}
                    draft={editDraft}
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
        onSubmit={(event) => {
          event.preventDefault();
          void submitReply();
        }}
      >
        {sendError ? (
          <p role="alert" className="mb-2 text-sm text-destructive">
            {sendError}
          </p>
        ) : null}
        <SelectedPhotoPreviewList
          photos={photoSelection.photos}
          error={photoSelection.error}
          errorId={photoSelectionErrorId}
          disabled={submitting}
          onRemove={photoSelection.removePhoto}
        />
        <div className="flex items-end gap-2">
          <PhotoAttachControl
            onFilesSelected={photoSelection.addFiles}
            disabled={submitting}
            describedBy={photoSelection.error ? photoSelectionErrorId : undefined}
          />
          <MessageInput
            value={draft}
            onChange={handleDraftChange}
            ariaLabel="Thread reply"
            placeholder="Reply in thread..."
            className="flex min-w-0 flex-1 items-end gap-2"
            inputRef={(element) => {
              inputRef.current = element;
            }}
            mentionUsers={props.mentionUsers}
            onMentionUsers={props.onMentionUsers}
            searchMentionUsers={props.searchMentionUsers}
            mentionSearchLimit={props.mentionSearchLimit}
          />
          <button
            className="rounded-md bg-primary/10 p-4 text-primary transition-colors hover:bg-primary/20 disabled:opacity-50"
            type="submit"
            aria-label="Send response to thread"
            disabled={
              submitting || (draftText.trim().length === 0 && photoSelection.photos.length === 0)
            }
          >
            Send
          </button>
        </div>
      </form>
      <EmojiPicker
        open={reactionPicker !== null}
        anchor={() => reactionPicker?.anchor}
        emojis={[...CONSERVATIVE_EMOJIS, ...customEmojisToEntries(activeCustomEmojis)]}
        onSelect={(emoji) => {
          const message = reactionPicker ? findThreadMessage(reactionPicker.messageId) : null;
          if (message) addReactionFromPicker(message, emoji);
        }}
        onClose={() => setReactionPicker(null)}
      />
      <Modal
        open={pendingDeleteId !== null}
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
