import { useLocation, useNavigate, useParams } from "@solidjs/router";
import {
  createEffect,
  createMemo,
  createResource,
  createSignal,
  createUniqueId,
  onCleanup,
  onMount,
} from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import ChannelMessages from "../components/channel-messages";
import {
  PhotoAttachControl,
  SelectedPhotoPreviewList,
  createComposerPhotoSelection,
} from "../components/composer-photo-selection";
import LocalCameraTile from "../components/local-camera-tile";
import MessageInput from "../components/message-input";
import RemoteCameraTiles from "../components/remote-camera-tiles";
import ScreenShareViewer from "../components/screen-share-viewer";
import ThreadPanel from "../components/thread-panel";
import TypingIndicator from "../components/typing-indicator";
import { listMessages, sendMessage, sendTyping, type Message } from "../api";
import { useChannels } from "../contexts/channels";
import { useEvents } from "../contexts/events";
import { useAuth } from "../contexts/auth";
import { TYPING_PING_INTERVAL_MS } from "../constants";
import { mergeReactionUpdateForViewer } from "../reactions/reaction-summaries";

function parseThreadId(value: string | string[] | undefined): number | null {
  const raw = Array.isArray(value) ? value[0] : value;
  if (!raw) return null;
  const parsed = Number(raw);
  return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : null;
}

export default function ChannelView() {
  const params = useParams<{ id: string }>();
  const location = useLocation();
  const navigate = useNavigate();
  const { channels } = useChannels();
  const events = useEvents();
  const { user } = useAuth();
  const channel = () => channels()?.find((c) => String(c.id) === params.id);
  const [message, setMessage] = createSignal("");
  const [submitting, setSubmitting] = createSignal(false);
  const photoSelection = createComposerPhotoSelection();
  const photoSelectionErrorId = createUniqueId();
  const [focusComposerRootId, setFocusComposerRootId] = createSignal<number | null>(null);
  const openThreadRootId = createMemo(() => parseThreadId(location.query.thread));
  // The Resource owns loading/error state for the initial fetch; the Store
  // owns the reactive list that SSE events mutate granularly. createStore
  // means an embed update on one row only re-renders that row.
  const [resource] = createResource(() => params.id, listMessages);
  const [messages, setMessages] = createStore<Message[]>([]);
  let lastTypingSentAt = 0;
  let composerRef: HTMLElement | undefined;

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

  const channelPath = () => `/channel/${params.id}`;

  const threadPath = (rootMessageId: number) => `${channelPath()}?thread=${rootMessageId}`;

  const openThread = (root: Message, options?: { focusComposer?: boolean }) => {
    setFocusComposerRootId(options?.focusComposer ? root.id : null);
    navigate(threadPath(root.id), { scroll: false });
  };

  const closeThread = () => {
    setFocusComposerRootId(null);
    navigate(channelPath(), { scroll: false });
  };

  createEffect(() => {
    const rootId = openThreadRootId();
    if (rootId === null) {
      setFocusComposerRootId(null);
      return;
    }
    if (resource.loading) return;
    const rootInChannel = messages.some((m) => m.id === rootId && m.parent_id == null);
    if (!rootInChannel) {
      setFocusComposerRootId(null);
      navigate(channelPath(), { replace: true, scroll: false });
    }
  });

  const handleMessageChange = (value: string) => {
    setMessage(value);
    if (value.length === 0) return;
    const now = Date.now();
    if (now - lastTypingSentAt < TYPING_PING_INTERVAL_MS) return;
    lastTypingSentAt = now;
    void sendTyping(params.id);
  };

  const hasDraftContent = () => message().trim().length > 0 || photoSelection.photos().length > 0;

  const submitMessage = async () => {
    if (submitting() || !hasDraftContent()) return;

    const text = message();
    const photos = photoSelection.photos().map((photo) => photo.file);
    setSubmitting(true);
    try {
      const response = await sendMessage(params.id, text, photos);
      if (!response.ok) return;
      setMessage("");
      photoSelection.clearPhotos();
      lastTypingSentAt = 0;
      queueMicrotask(() => composerRef?.focus());
    } catch (e) {
      console.error("failed to send message", e);
    } finally {
      setSubmitting(false);
    }
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
    const unsubReactions = events.onMessageReactionsUpdated((e) => {
      if (String(e.channel_id) !== params.id) return;
      setMessages(
        (existing) => existing.id === e.id,
        (existing) => ({
          reactions: mergeReactionUpdateForViewer(
            existing.reactions ?? [],
            e.reactions,
            e.user_id,
            user()?.id ?? null,
          ),
        }),
      );
    });
    const unsubThreadReply = events.onThreadReplyCreated((e) => {
      if (String(e.channel_id) !== params.id) return;
      setMessages((existing) => existing.id === e.root_message_id && existing.parent_id == null, {
        thread_summary: e.thread_summary,
      });
    });
    const unsubThreadReplyDeleted = events.onThreadReplyDeleted((e) => {
      if (String(e.channel_id) !== params.id) return;
      setMessages((existing) => existing.id === e.root_message_id && existing.parent_id == null, {
        thread_summary: e.thread_summary ?? undefined,
      });
    });
    onCleanup(() => {
      unsubCreated();
      unsubUpdated();
      unsubDeleted();
      unsubEmbeds();
      unsubReactions();
      unsubThreadReply();
      unsubThreadReplyDeleted();
    });
  });

  // TODO(reno): Can the router ensure this parameter exists?
  if (!params.id) {
    return <div>Error: channel required</div>;
  }

  return (
    <div class="flex flex-col h-full bg-white text-gray-900">
      <section class="bg-gray-100 text-gray-700 p-4 flex-shrink-0">
        <h1 class="text-2xl font-bold"># {channel()?.name ?? params.id}</h1>
      </section>

      <ScreenShareViewer />
      <LocalCameraTile />
      <RemoteCameraTiles />

      <div class="flex-1 min-h-0 flex">
        <div class="min-w-0 flex-1 overflow-y-auto">
          <ChannelMessages
            messages={messages}
            loading={resource.loading}
            error={resource.error}
            currentUserId={user()?.id ?? null}
            onOpenThread={openThread}
            onReactionsChange={(messageId, reactions) => {
              setMessages((existing) => existing.id === messageId, { reactions });
            }}
          />
        </div>
        {openThreadRootId() !== null && (
          <ThreadPanel
            rootMessageId={openThreadRootId() as number}
            channelId={Number(params.id)}
            currentUserId={user()?.id ?? null}
            focusComposer={focusComposerRootId() === openThreadRootId()}
            onComposerFocusConsumed={() => setFocusComposerRootId(null)}
            onClose={closeThread}
          />
        )}
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
            void submitMessage();
          }}
        >
          <SelectedPhotoPreviewList
            photos={photoSelection.photos()}
            error={photoSelection.error()}
            errorId={photoSelectionErrorId}
            disabled={submitting()}
            onRemove={photoSelection.removePhoto}
          />
          <div class="flex items-center gap-2">
            <PhotoAttachControl
              onFilesSelected={photoSelection.addFiles}
              disabled={submitting()}
              describedBy={photoSelection.error() ? photoSelectionErrorId : undefined}
            />
            <MessageInput
              value={message()}
              onChange={handleMessageChange}
              ariaLabel="New message"
              placeholder="Send a new message..."
              inputRef={(el) => {
                composerRef = el;
              }}
            />
            <button
              class="bg-blue-100 p-4 rounded-md disabled:opacity-50"
              type="submit"
              disabled={submitting() || !hasDraftContent()}
            >
              Send
            </button>
          </div>
        </form>
      </section>
    </div>
  );
}
