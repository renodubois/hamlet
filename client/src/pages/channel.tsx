import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import {
  useAfterRenderEffect,
  If,
  useComputedValue,
  useCallableResource,
  useSignalState,
  useStableDomId,
  registerCleanup,
  useMountEffect,
} from "../hooks/react-state";
import { useStoreState, preserveIdentity } from "../hooks/react-state";
import ChannelMessages from "../components/channel-messages";
import {
  PhotoAttachControl,
  SelectedPhotoPreviewList,
  createComposerPhotoSelection,
} from "../components/composer-photo-selection";
import LocalCameraTile from "../components/local-camera-tile";
import MessageInput from "../components/message-input";
import MessageReferencePreview, {
  messageReferencePreviewText,
} from "../components/message-reference-preview";
import RemoteCameraTiles from "../components/remote-camera-tiles";
import ScreenShareViewer from "../components/screen-share-viewer";
import ThreadPanel from "../components/thread-panel";
import TypingIndicator from "../components/typing-indicator";
import {
  listMessages,
  messageDisplayName,
  messageReferenceFromMessage,
  messageReferencesTarget,
  searchUsers,
  sendMessage,
  sendTyping,
  type Message,
  type PublicUser,
} from "../api";
import { useChannels } from "../contexts/channels";
import { useEvents } from "../contexts/events";
import { useAuth } from "../contexts/auth";
import { useReadStates } from "../contexts/read-states";
import { TYPING_PING_INTERVAL_MS } from "../constants";
import { mergeReactionUpdateForViewer } from "../reactions/reaction-summaries";
import { getViewportReadMarkerState, isNearScrollBottom } from "../messages/viewport-read-marker";

function inlineReplyPreviewText(target: Message): string {
  return messageReferencePreviewText(target);
}

function parseThreadId(value: string | string[] | undefined): number | null {
  const raw = Array.isArray(value) ? value[0] : value;
  if (!raw) return null;
  const parsed = Number(raw);
  return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : null;
}

export default function ChannelView() {
  const params = useParams<{ id: string }>();
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const { channels } = useChannels();
  const events = useEvents();
  const { user } = useAuth();
  const readStates = useReadStates();
  const channel = () => channels()?.find((c) => String(c.id) === params.id);
  const [message, setMessage] = useSignalState("");
  const [submitting, setSubmitting] = useSignalState(false);
  const [replyTarget, setReplyTarget] = useSignalState<Message | null>(null);
  const photoSelection = createComposerPhotoSelection();
  const photoSelectionErrorId = useStableDomId();
  const replyBannerId = useStableDomId();
  const [focusComposerRootId, setFocusComposerRootId] = useSignalState<number | null>(null);
  const [hasNewMessagesBelow, setHasNewMessagesBelow] = useSignalState(false);
  const [scrollToBottomRequestVersion, setScrollToBottomRequestVersion] = useState(0);
  const openThreadRootId = useComputedValue(() =>
    parseThreadId(searchParams.get("thread") ?? undefined),
  );
  // The Resource owns loading/error state for the initial fetch; the Store
  // owns the reactive list that SSE events mutate granularly. useStoreState
  // means an embed update on one row only re-renders that row.
  const [resource] = useCallableResource(() => params.id ?? "", listMessages);
  const [messages, setMessages] = useStoreState<Message[]>([]);
  const [mentionUserCache, setMentionUserCache] = useSignalState<Map<number, PublicUser>>(
    new Map(),
  );
  const lastTypingSentAtRef = useRef(0);
  const composerRef = useRef<HTMLElement | null>(null);
  const messagesScrollRef = useRef<HTMLDivElement | null>(null);
  const markReadTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastMarkReadKeyRef = useRef<string | null>(null);
  const paramsIdRef = useRef(params.id);
  paramsIdRef.current = params.id;
  const replyTargetRef = useRef(replyTarget());
  replyTargetRef.current = replyTarget();
  const userIdRef = useRef(user()?.id ?? null);
  userIdRef.current = user()?.id ?? null;
  const hasSeenInitialUserProfileRef = useRef(false);
  const pendingScrollToBottomRef = useRef<{ afterScroll?: () => void } | null>(null);

  const scheduleActiveChannelMarkRead = () => {
    if (!messagesScrollRef.current) return;
    const channelId = Number(params.id);
    if (!Number.isSafeInteger(channelId) || channelId <= 0) return;

    const marker = getViewportReadMarkerState(messagesScrollRef.current);
    if (
      !marker.rendererEligible ||
      !marker.nearBottom ||
      marker.lastVisibleTopLevelMessageId === null
    ) {
      return;
    }

    const markReadKey = `${channelId}:${marker.lastVisibleTopLevelMessageId}`;
    if (lastMarkReadKeyRef.current === markReadKey) return;
    if (markReadTimerRef.current) clearTimeout(markReadTimerRef.current);
    markReadTimerRef.current = setTimeout(() => {
      if (!messagesScrollRef.current) return;
      const current = getViewportReadMarkerState(messagesScrollRef.current);
      if (
        !current.rendererEligible ||
        !current.nearBottom ||
        current.lastVisibleTopLevelMessageId === null
      ) {
        return;
      }
      const currentKey = `${channelId}:${current.lastVisibleTopLevelMessageId}`;
      lastMarkReadKeyRef.current = currentKey;
      void readStates.markRead(channelId, current.lastVisibleTopLevelMessageId);
    }, 120);
  };

  const scrollMessagesToBottom = (afterScroll?: () => void) => {
    if (!messagesScrollRef.current) return;
    messagesScrollRef.current.scrollTop = messagesScrollRef.current.scrollHeight;
    setHasNewMessagesBelow(false);
    afterScroll?.();
  };

  const requestScrollMessagesToBottom = (afterScroll?: () => void) => {
    pendingScrollToBottomRef.current = { afterScroll };
    setScrollToBottomRequestVersion((version) => version + 1);
  };

  const handleMessagesScroll = () => {
    if (!messagesScrollRef.current) return;
    const marker = getViewportReadMarkerState(messagesScrollRef.current);
    if (!marker.nearBottom) {
      pendingScrollToBottomRef.current = null;
    } else {
      setHasNewMessagesBelow(false);
    }
    scheduleActiveChannelMarkRead();
  };

  const jumpToLatestMessages = () => {
    scrollMessagesToBottom(scheduleActiveChannelMarkRead);
  };

  useLayoutEffect(() => {
    const pendingScroll = pendingScrollToBottomRef.current;
    if (!pendingScroll) return;
    pendingScrollToBottomRef.current = null;
    scrollMessagesToBottom(pendingScroll.afterScroll);
  }, [scrollToBottomRequestVersion]);

  const showNewMessagesBelow = () =>
    hasNewMessagesBelow() ||
    (messages.length > (resource()?.length ?? 0) &&
      !!messagesScrollRef.current &&
      !isNearScrollBottom(messagesScrollRef.current));

  const primeMentionUsers = (users: readonly PublicUser[]) => {
    if (users.length === 0) return;
    setMentionUserCache((current) => {
      let changed = false;
      const next = new Map(current);
      for (const mentionUser of users) {
        const existing = next.get(mentionUser.id);
        if (
          existing &&
          existing.username === mentionUser.username &&
          existing.display_name === mentionUser.display_name &&
          existing.avatar_url === mentionUser.avatar_url
        ) {
          continue;
        }
        next.set(mentionUser.id, mentionUser);
        changed = true;
      }
      return changed ? next : current;
    });
  };

  const primeMentionUsersFromMessages = (nextMessages: readonly Message[]) => {
    primeMentionUsers(nextMessages.flatMap((nextMessage) => nextMessage.mentions ?? []));
  };

  const mentionUsers = () => Array.from(mentionUserCache().values());
  const displayedMessages = () => (messages.length > 0 ? messages : (resource() ?? []));

  // Reconcile the fetched array into the store on initial load and on every
  // channel switch. The "id" key tells preserveIdentity to diff items rather than
  // replace the whole array, so already-rendered rows survive when the same
  // message appears in both the old and new fetches.
  useEffect(() => {
    const data = resource();
    primeMentionUsersFromMessages(data ?? []);
    setMessages(preserveIdentity(data ?? [], { key: "id" }));
    requestScrollMessagesToBottom(scheduleActiveChannelMarkRead);
  }, [params.id, resource.latest]);

  // When the current user's profile changes (display_name, avatar), patch
  // every message of theirs in place so the rendered list reflects the new
  // values without a refetch.
  useEffect(() => {
    const u = user();
    if (!u) return;
    if (!hasSeenInitialUserProfileRef.current) {
      hasSeenInitialUserProfileRef.current = true;
      return;
    }
    setMessages(
      (m) => m.user_id === u.id,
      (m) => ({
        ...m,
        username: u.username,
        display_name: u.display_name,
        avatar_url: u.avatar_url,
      }),
    );
  }, [user()?.avatar_url, user()?.display_name, user()?.id, user()?.username]);

  const channelPath = () => `/channel/${params.id}`;

  const threadPath = (rootMessageId: number) => `${channelPath()}?thread=${rootMessageId}`;

  const openThread = (root: Message, options?: { focusComposer?: boolean }) => {
    void navigate(threadPath(root.id));
    setFocusComposerRootId(options?.focusComposer ? root.id : null);
  };

  const closeThread = () => {
    setFocusComposerRootId(null);
    void navigate(channelPath());
  };

  const selectInlineReplyTarget = (target: Message) => {
    if (target.deleted_at != null || target.parent_id != null) return;
    setReplyTarget(target);
    queueMicrotask(() => composerRef.current?.focus());
  };

  const dismissInlineReplyTarget = () => {
    setReplyTarget(null);
    queueMicrotask(() => composerRef.current?.focus());
  };

  useEffect(() => {
    if (params.id) {
      setReplyTarget(null);
      setHasNewMessagesBelow(false);
      lastMarkReadKeyRef.current = null;
    }
  }, [params.id]);

  useAfterRenderEffect(() => {
    const rootId = openThreadRootId();
    if (rootId === null) return;
    if (resource.loading) return;
    const loadedMessages = resource() ?? [];
    const rootInChannel = [...loadedMessages, ...messages].some(
      (m) => m.id === rootId && m.parent_id == null,
    );
    if (!rootInChannel) {
      setFocusComposerRootId(null);
      void navigate(channelPath(), { replace: true });
    }
  });

  const handleMessageChange = (value: string) => {
    setMessage(value);
    if (value.length === 0) return;
    const now = Date.now();
    if (now - lastTypingSentAtRef.current < TYPING_PING_INTERVAL_MS) return;
    lastTypingSentAtRef.current = now;
    void sendTyping(params.id ?? "");
  };

  const hasDraftContent = () => message().trim().length > 0 || photoSelection.photos().length > 0;

  const patchVisibleReplyReferences = (targetId: number, target: Message | null) => {
    setMessages((existing) => messageReferencesTarget(existing, targetId), {
      reply_to_message_id: targetId,
      reply_to: target ? messageReferenceFromMessage(target) : null,
    });
  };

  const applyMessageUpdate = (m: Message) => {
    if (String(m.channel_id) !== paramsIdRef.current) return;
    primeMentionUsers(m.mentions ?? []);
    setMessages((existing) => existing.id === m.id, m);
    patchVisibleReplyReferences(m.id, m);
    if (replyTargetRef.current?.id === m.id) {
      setReplyTarget(m.deleted_at == null ? m : null);
    }
  };

  const submitMessage = async () => {
    if (submitting() || !hasDraftContent()) return;

    const text = message();
    const photos = photoSelection.photos().map((photo) => photo.file);
    const target = replyTarget();
    setSubmitting(true);
    try {
      const response = await sendMessage(params.id ?? "", text, photos, {
        replyToMessageId: target?.id,
      });
      if (!response.ok) return;
      const createdMessage = (await response
        .clone()
        .json()
        .catch(() => null)) as Message | null;
      primeMentionUsers(createdMessage?.mentions ?? []);
      setMessage("");
      setReplyTarget(null);
      photoSelection.clearPhotos();
      lastTypingSentAtRef.current = 0;
      queueMicrotask(() => composerRef.current?.focus());
    } catch (e) {
      console.error("failed to send message", e);
    } finally {
      setSubmitting(false);
    }
  };

  useMountEffect(() => {
    const unsubCreated = events.onMessage((m) => {
      if (String(m.channel_id) !== paramsIdRef.current) return;
      const shouldAutoFollow = messagesScrollRef.current
        ? messagesScrollRef.current.scrollHeight > messagesScrollRef.current.clientHeight &&
          isNearScrollBottom(messagesScrollRef.current)
        : true;
      primeMentionUsers(m.mentions ?? []);
      setMessages((current) => [...current, m]);
      if (shouldAutoFollow) {
        requestScrollMessagesToBottom(scheduleActiveChannelMarkRead);
      } else {
        setHasNewMessagesBelow(true);
        scheduleActiveChannelMarkRead();
      }
    });
    const unsubUpdated = events.onMessageUpdated((m) => {
      applyMessageUpdate(m);
    });
    const unsubDeleted = events.onMessageDeleted((d) => {
      if (String(d.channel_id) !== paramsIdRef.current) return;
      setMessages((arr) => arr.filter((existing) => existing.id !== d.id));
      patchVisibleReplyReferences(d.id, null);
      if (replyTargetRef.current?.id === d.id) setReplyTarget(null);
    });
    const unsubEmbeds = events.onMessageEmbedsUpdated((e) => {
      if (String(e.channel_id) !== paramsIdRef.current) return;
      setMessages((existing) => existing.id === e.id, {
        suppress_embeds: e.suppress_embeds,
        embeds: e.embeds,
      });
    });
    const unsubReactions = events.onMessageReactionsUpdated((e) => {
      if (String(e.channel_id) !== paramsIdRef.current) return;
      setMessages(
        (existing) => existing.id === e.id,
        (existing) => ({
          reactions: mergeReactionUpdateForViewer(
            existing.reactions ?? [],
            e.reactions,
            e.user_id,
            userIdRef.current,
          ),
        }),
      );
    });
    const unsubThreadReply = events.onThreadReplyCreated((e) => {
      if (String(e.channel_id) !== paramsIdRef.current) return;
      primeMentionUsers(e.reply.mentions ?? []);
      setMessages((existing) => existing.id === e.root_message_id && existing.parent_id == null, {
        thread_summary: e.thread_summary,
      });
    });
    const unsubThreadReplyDeleted = events.onThreadReplyDeleted((e) => {
      if (String(e.channel_id) !== paramsIdRef.current) return;
      setMessages((existing) => existing.id === e.root_message_id && existing.parent_id == null, {
        thread_summary: e.thread_summary ?? undefined,
      });
    });
    const onFocusOrVisibility = () => scheduleActiveChannelMarkRead();
    window.addEventListener("focus", onFocusOrVisibility);
    document.addEventListener("visibilitychange", onFocusOrVisibility);
    registerCleanup(() => {
      unsubCreated();
      unsubUpdated();
      unsubDeleted();
      unsubEmbeds();
      unsubReactions();
      unsubThreadReply();
      unsubThreadReplyDeleted();
      window.removeEventListener("focus", onFocusOrVisibility);
      document.removeEventListener("visibilitychange", onFocusOrVisibility);
      if (markReadTimerRef.current) clearTimeout(markReadTimerRef.current);
    });
  });

  // TODO(reno): Can the router ensure this parameter exists?
  if (!params.id) {
    return <div>Error: channel required</div>;
  }

  return (
    <div className="flex flex-col h-full bg-white text-gray-900">
      <section className="bg-gray-100 text-gray-700 p-4 flex-shrink-0">
        <h1 className="text-2xl font-bold"># {channel()?.name ?? params.id}</h1>
      </section>

      <ScreenShareViewer />
      <LocalCameraTile />
      <RemoteCameraTiles />

      <div className="flex-1 min-h-0 flex">
        <div
          ref={(el) => {
            messagesScrollRef.current = el;
          }}
          className="min-h-0 min-w-0 flex flex-1 flex-col overflow-y-auto"
          role="region"
          aria-label="Messages"
          onScroll={handleMessagesScroll}
        >
          <ChannelMessages
            messages={displayedMessages()}
            loading={resource.loading}
            error={resource.error}
            currentUserId={user()?.id ?? null}
            onOpenThread={openThread}
            onReplyToMessage={selectInlineReplyTarget}
            onMessageUpdated={applyMessageUpdate}
            onReactionsChange={(messageId, reactions) => {
              setMessages((existing) => existing.id === messageId, { reactions });
            }}
            mentionUsers={mentionUsers()}
            onMentionUsers={primeMentionUsers}
            searchMentionUsers={searchUsers}
          />
          <If when={showNewMessagesBelow()}>
            <div className="sticky bottom-4 z-10 flex justify-center px-4" aria-live="polite">
              <button
                type="button"
                className="rounded-full bg-blue-600 px-4 py-2 text-sm font-semibold text-white shadow-lg hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-300"
                aria-label="New messages below. Jump to latest messages"
                onClick={jumpToLatestMessages}
              >
                New messages — Jump to bottom
              </button>
            </div>
          </If>
        </div>
        {openThreadRootId() !== null && (
          <ThreadPanel
            rootMessageId={openThreadRootId() as number}
            channelId={Number(params.id)}
            currentUserId={user()?.id ?? null}
            currentUserName={user()?.username ?? null}
            focusComposer={focusComposerRootId() === openThreadRootId()}
            onComposerFocusConsumed={() => setFocusComposerRootId(null)}
            onClose={closeThread}
            mentionUsers={mentionUsers()}
            onMentionUsers={primeMentionUsers}
            searchMentionUsers={searchUsers}
          />
        )}
      </div>

      <section className="flex-shrink-0 p-4 border-t border-gray-200">
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
          <If when={replyTarget()}>
            {(target) => (
              <MessageReferencePreview
                id={replyBannerId}
                reference={target()}
                className="mb-2 flex min-w-0 items-center gap-2 rounded-md border border-blue-200 bg-blue-50 px-3 py-2 text-sm text-blue-950"
                authorClass="shrink-0 font-semibold"
                textClass="min-w-0 flex-1 truncate text-blue-900"
                authorPrefix="Replying to "
                ariaLabelPrefix="Inline reply target: "
                role="status"
                ariaLive="polite"
              >
                <button
                  type="button"
                  className="rounded px-2 py-1 text-sm font-medium text-blue-700 hover:bg-blue-100 focus:outline-none focus:ring-2 focus:ring-blue-400"
                  aria-label={`Dismiss inline reply to message by ${messageDisplayName(
                    target(),
                  )}: ${inlineReplyPreviewText(target())}`}
                  onClick={dismissInlineReplyTarget}
                >
                  Cancel
                </button>
              </MessageReferencePreview>
            )}
          </If>
          <div className="flex items-center gap-2">
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
              describedBy={replyTarget() ? replyBannerId : undefined}
              mentionUsers={mentionUsers()}
              onMentionUsers={primeMentionUsers}
              searchMentionUsers={searchUsers}
              inputRef={(el) => {
                composerRef.current = el;
              }}
            />
            <button
              className="bg-blue-100 p-4 rounded-md disabled:opacity-50"
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
