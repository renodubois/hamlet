import {
  For,
  Show,
  createEffect,
  createResource,
  createSignal,
  onCleanup,
  onMount,
} from "solid-js";
import {
  deleteMessage,
  editMessage,
  getThread,
  messageDisplayName,
  sendThreadReply,
  setMessageEmbedsSuppressed,
  type Message,
} from "../api";
import { useEvents } from "../contexts/events";
import { linkifyText } from "../linkify";
import Avatar from "./avatar";
import { DeleteIcon, EditIcon } from "./icons";
import MessageEmbed from "./message-embed";
import MessageInput from "./message-input";
import Modal from "./modal";

function isDeletedMessage(message: Message): boolean {
  return message.deleted_at != null;
}

function MessageBody(props: { message: Message }) {
  return (
    <Show
      when={!isDeletedMessage(props.message)}
      fallback={
        <p class="italic text-gray-500" aria-label="Original message deleted">
          Original message deleted
        </p>
      }
    >
      <div class="whitespace-pre-wrap break-words [overflow-wrap:anywhere]">
        {linkifyText(props.message.text).map((tok) =>
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
            tok.value
          ),
        )}
      </div>
    </Show>
  );
}

function ThreadMessage(props: {
  message: Message;
  currentUserId: number | null;
  editing: boolean;
  draft: string;
  onDraftChange: (value: string) => void;
  onStartEdit: (message: Message) => void;
  onCancelEdit: () => void;
  onSaveEdit: (message: Message) => void;
  onRequestDelete: (message: Message) => void;
  onSuppressEmbeds: (message: Message) => void;
}) {
  const isOwnReply = () =>
    !isDeletedMessage(props.message) &&
    props.message.parent_id != null &&
    props.currentUserId !== null &&
    props.message.user_id === props.currentUserId;

  return (
    <article class="group relative flex items-start gap-3 rounded-md py-2 pr-12 hover:bg-gray-50 focus-within:bg-gray-50">
      <Avatar
        url={isDeletedMessage(props.message) ? null : props.message.avatar_url}
        username={
          isDeletedMessage(props.message) ? "Deleted message" : messageDisplayName(props.message)
        }
        size={32}
      />
      <div class="min-w-0 flex-1">
        <Show when={!isDeletedMessage(props.message)}>
          <div class="font-bold">{messageDisplayName(props.message)}</div>
        </Show>
        <Show
          when={props.editing}
          fallback={
            <>
              <MessageBody message={props.message} />
              <Show
                when={
                  !isDeletedMessage(props.message) &&
                  !props.message.suppress_embeds &&
                  props.message.embeds.length > 0
                }
              >
                <div class="mt-1 flex flex-col gap-1">
                  <For each={props.message.embeds}>
                    {(embed) => (
                      <MessageEmbed
                        embed={embed}
                        onRemove={
                          isOwnReply() ? () => props.onSuppressEmbeds(props.message) : undefined
                        }
                      />
                    )}
                  </For>
                </div>
              </Show>
            </>
          }
        >
          <form
            class="flex gap-2 items-center"
            onSubmit={(e) => {
              e.preventDefault();
              props.onSaveEdit(props.message);
            }}
          >
            <MessageInput
              value={props.draft}
              onChange={props.onDraftChange}
              ariaLabel="Edit reply"
              inputRef={(el) => queueMicrotask(() => el.focus())}
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  e.preventDefault();
                  props.onCancelEdit();
                }
              }}
              class="flex min-w-0 flex-1 items-center gap-2"
              inputClass="bg-gray-100 rounded-md px-2 py-1 w-full"
              emojiButtonClass="cursor-pointer rounded-md bg-gray-100 px-2 py-1 text-gray-700 hover:bg-gray-200 focus:outline-none focus:ring-2 focus:ring-blue-400"
              emojiButtonLabel="Open emoji picker for reply edit"
            />
            <button type="submit" class="text-sm text-blue-600">
              Save
            </button>
            <button type="button" class="text-sm text-gray-500" onClick={props.onCancelEdit}>
              Cancel
            </button>
          </form>
        </Show>
      </div>
      <Show when={isOwnReply() && !props.editing}>
        <div
          role="toolbar"
          aria-label="Reply actions"
          class="absolute right-1 top-1 flex gap-1 rounded-md border border-gray-200 bg-white shadow-sm opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity"
        >
          <button
            type="button"
            aria-label="Edit reply"
            title="Edit reply"
            class="p-1.5 rounded-md hover:bg-gray-100"
            onClick={() => props.onStartEdit(props.message)}
          >
            <EditIcon size={14} />
          </button>
          <button
            type="button"
            aria-label="Delete reply"
            title="Delete reply"
            class="p-1.5 rounded-md text-red-600 hover:bg-red-50"
            onClick={() => props.onRequestDelete(props.message)}
          >
            <DeleteIcon size={14} />
          </button>
        </div>
      </Show>
    </article>
  );
}

export default function ThreadPanel(props: {
  rootMessageId: number;
  channelId: number;
  currentUserId: number | null;
  onClose: () => void;
  focusComposer?: boolean;
  onComposerFocusConsumed?: () => void;
}) {
  const [thread, { mutate }] = createResource(
    () => props.rootMessageId,
    (rootMessageId) => getThread(rootMessageId),
  );
  const [draft, setDraft] = createSignal("");
  const [submitting, setSubmitting] = createSignal(false);
  const [loadingOlder, setLoadingOlder] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [olderError, setOlderError] = createSignal<string | null>(null);
  const [editingId, setEditingId] = createSignal<number | null>(null);
  const [editDraft, setEditDraft] = createSignal("");
  const [pendingDeleteId, setPendingDeleteId] = createSignal<number | null>(null);
  const events = useEvents();
  let inputRef: (HTMLElement & { value?: string }) | undefined;
  let repliesScrollRef: HTMLDivElement | undefined;

  const appendReply = (reply: Message) => {
    mutate((current) => {
      if (!current) return current;
      if (current.replies.some((existing) => existing.id === reply.id)) return current;
      return { ...current, replies: [...current.replies, reply] };
    });
  };

  const updateMessageInThread = (message: Message) => {
    mutate((current) => {
      if (!current) return current;
      if (current.root.id === message.id) return { ...current, root: message };
      return {
        ...current,
        replies: current.replies.map((reply) => (reply.id === message.id ? message : reply)),
      };
    });
  };

  const removeReply = (replyId: number) => {
    mutate((current) => {
      if (!current) return current;
      return { ...current, replies: current.replies.filter((reply) => reply.id !== replyId) };
    });
  };

  onMount(() => {
    const unsubscribeCreated = events.onThreadReplyCreated((event) => {
      if (event.channel_id !== props.channelId || event.root_message_id !== props.rootMessageId) {
        return;
      }
      appendReply(event.reply);
    });
    const unsubscribeDeleted = events.onThreadReplyDeleted((event) => {
      if (event.channel_id !== props.channelId || event.root_message_id !== props.rootMessageId) {
        return;
      }
      removeReply(event.reply_id);
    });
    const unsubscribeUpdated = events.onMessageUpdated((message) => {
      if (message.channel_id !== props.channelId) return;
      if (message.id !== props.rootMessageId && message.parent_id !== props.rootMessageId) return;
      updateMessageInThread(message);
    });
    const unsubscribeEmbeds = events.onMessageEmbedsUpdated((event) => {
      if (event.channel_id !== props.channelId) return;
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
              ? { ...reply, suppress_embeds: event.suppress_embeds, embeds: event.embeds }
              : reply,
          ),
        };
      });
    });
    onCleanup(() => {
      unsubscribeCreated();
      unsubscribeDeleted();
      unsubscribeUpdated();
      unsubscribeEmbeds();
    });
  });

  createEffect(() => {
    const rootMessageId = props.rootMessageId;
    if (props.focusComposer && rootMessageId > 0) {
      queueMicrotask(() => {
        inputRef?.focus();
        props.onComposerFocusConsumed?.();
      });
    }
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
    const previousScrollHeight = repliesScrollRef?.scrollHeight ?? 0;
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
        if (!repliesScrollRef) return;
        repliesScrollRef.scrollTop += repliesScrollRef.scrollHeight - previousScrollHeight;
      });
    } catch (e) {
      setOlderError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoadingOlder(false);
    }
  };

  const submitReply = async () => {
    const text = draft() || inputRef?.value || inputRef?.textContent || "";
    if (text.length === 0 || submitting()) return;
    setSubmitting(true);
    setError(null);
    setDraft("");
    try {
      const reply = await sendThreadReply(props.rootMessageId, text);
      appendReply(reply);
      queueMicrotask(() => inputRef?.focus());
    } catch (e) {
      setDraft(text);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSubmitting(false);
    }
  };

  const startEditing = (message: Message) => {
    setEditDraft(message.text);
    setEditingId(message.id);
  };

  const cancelEditing = () => {
    setEditingId(null);
    setEditDraft("");
  };

  const saveEdit = async (message: Message) => {
    const next = editDraft();
    if (next.length === 0) {
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
              ? { ...reply, suppress_embeds: updated.suppress_embeds, embeds: updated.embeds }
              : reply,
          ),
        };
      });
    } catch (e) {
      console.error("failed to suppress thread reply embeds", e);
    }
  };

  return (
    <aside
      class="w-96 max-w-[45vw] min-h-0 flex-shrink-0 border-l border-gray-200 bg-white text-gray-900 flex flex-col"
      aria-label="Thread panel"
    >
      <header class="flex items-center justify-between border-b border-gray-200 p-4">
        <h2 class="font-bold text-lg">Thread</h2>
        <button
          type="button"
          class="rounded-md px-2 py-1 text-sm text-gray-600 hover:bg-gray-100"
          aria-label="Close thread"
          onClick={props.onClose}
        >
          Close
        </button>
      </header>

      <div
        class="min-h-0 flex-1 overflow-y-auto p-4"
        ref={(el) => {
          repliesScrollRef = el;
        }}
      >
        <Show when={thread.loading}>
          <p>Loading thread...</p>
        </Show>
        <Show when={thread.error}>
          <p role="alert" class="text-red-700">
            Error loading thread: {String(thread.error)}
          </p>
        </Show>
        <Show when={thread()}>
          {(loaded) => (
            <div class="flex flex-col gap-2">
              <ThreadMessage
                message={loaded().root}
                currentUserId={props.currentUserId}
                editing={false}
                draft=""
                onDraftChange={() => undefined}
                onStartEdit={startEditing}
                onCancelEdit={cancelEditing}
                onSaveEdit={saveEdit}
                onRequestDelete={(message) => setPendingDeleteId(message.id)}
                onSuppressEmbeds={suppressEmbeds}
              />
              <Show when={loaded().replies.length > 0}>
                <div class="border-t border-gray-100 pt-2">
                  <Show when={loaded().has_more_replies}>
                    <div class="pb-2 text-center">
                      <button
                        type="button"
                        class="rounded-md border border-gray-300 px-3 py-1 text-sm text-gray-700 hover:bg-gray-50 disabled:opacity-50"
                        onClick={() => void loadOlderReplies()}
                        disabled={loadingOlder()}
                      >
                        {loadingOlder() ? "Loading older replies..." : "Load older replies"}
                      </button>
                    </div>
                  </Show>
                  <Show when={olderError()}>
                    {(message) => (
                      <p role="alert" class="pb-2 text-sm text-red-700">
                        Error loading older replies: {message()}
                      </p>
                    )}
                  </Show>
                  <For each={loaded().replies}>
                    {(reply) => (
                      <ThreadMessage
                        message={reply}
                        currentUserId={props.currentUserId}
                        editing={editingId() === reply.id}
                        draft={editDraft()}
                        onDraftChange={setEditDraft}
                        onStartEdit={startEditing}
                        onCancelEdit={cancelEditing}
                        onSaveEdit={saveEdit}
                        onRequestDelete={(message) => setPendingDeleteId(message.id)}
                        onSuppressEmbeds={suppressEmbeds}
                      />
                    )}
                  </For>
                </div>
              </Show>
            </div>
          )}
        </Show>
      </div>

      <form
        class="flex-shrink-0 border-t border-gray-200 p-4"
        onSubmit={(e) => {
          e.preventDefault();
          void submitReply();
        }}
      >
        <Show when={error()}>
          {(message) => (
            <p role="alert" class="mb-2 text-sm text-red-700">
              {message()}
            </p>
          )}
        </Show>
        <div class="flex items-end gap-2">
          <MessageInput
            value={draft()}
            onChange={setDraft}
            ariaLabel="Thread reply"
            placeholder="Reply in thread..."
            class="flex min-w-0 flex-1 items-end gap-2"
            inputRef={(el) => {
              inputRef = el;
            }}
          />
          <button
            class="rounded-md bg-blue-100 p-4 disabled:opacity-50"
            type="submit"
            disabled={submitting()}
          >
            Send
          </button>
        </div>
      </form>

      <Modal
        open={pendingDeleteId() !== null}
        onClose={() => setPendingDeleteId(null)}
        title="Delete reply?"
      >
        <p class="text-sm text-gray-200 mb-4">
          This will permanently delete the reply. This cannot be undone.
        </p>
        <div class="flex gap-2 justify-end">
          <button
            type="button"
            class="text-gray-300 hover:text-gray-100 text-sm px-3 py-2"
            onClick={() => setPendingDeleteId(null)}
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
    </aside>
  );
}
