import { useEffect, useId, useLayoutEffect, useReducer, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import ChannelMessages from "../components/channel-messages";
import {
  PhotoAttachControl,
  SelectedPhotoPreviewList,
  useComposerPhotoSelection,
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
import { Button } from "../components/ui/button";
import {
  getThread,
  listMessages,
  messageDisplayName,
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
import {
  channelMessageReducer,
  createChannelMessageState,
} from "../messages/channel-message-reducer";
import { getViewportReadMarkerState, isNearScrollBottom } from "../messages/viewport-read-marker";

function inlineReplyPreviewText(target: Message): string {
  return messageReferencePreviewText(target);
}

function parseThreadId(value: string | string[] | undefined): number | null {
  const raw = Array.isArray(value) ? value[0] : value;
  if (!raw || !/^[1-9]\d*$/.test(raw)) return null;
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
  const channel = channels.find((candidate) => String(candidate.id) === params.id);
  const [message, setMessage] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [replyTargetId, setReplyTargetId] = useState<number | null>(null);
  const photoSelection = useComposerPhotoSelection();
  const photoSelectionErrorId = useId();
  const replyBannerId = useId();
  const [focusComposerRootId, setFocusComposerRootId] = useState<number | null>(null);
  const [hasNewMessagesBelow, setHasNewMessagesBelow] = useState(false);
  const [scrollToBottomRequestVersion, setScrollToBottomRequestVersion] = useState(0);
  const rawThreadQuery = searchParams.get("thread");
  const requestedThreadRootId = parseThreadId(rawThreadQuery ?? undefined);
  const openThreadRootId = requestedThreadRootId;
  const routeChannelId = Number(params.id);
  const validChannelId =
    Number.isSafeInteger(routeChannelId) && routeChannelId > 0 ? routeChannelId : null;
  const [timeline, dispatch] = useReducer(
    channelMessageReducer,
    validChannelId ?? 0,
    createChannelMessageState,
  );
  const [mentionUserCache, setMentionUserCache] = useState<Map<number, PublicUser>>(
    () => new Map(),
  );
  const lastTypingSentAtRef = useRef(0);
  const composerRef = useRef<HTMLElement | null>(null);
  const messagesScrollRef = useRef<HTMLDivElement | null>(null);
  const markReadTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastMarkReadKeyRef = useRef<string | null>(null);
  const pendingMarkReadRef = useRef<{
    key: string;
    channelId: number;
    generation: number;
    token: number;
  } | null>(null);
  const markReadTokenRef = useRef(0);
  const paramsIdRef = useRef(params.id);
  paramsIdRef.current = params.id;
  const timelineRef = useRef(timeline);
  timelineRef.current = timeline;
  const generationSequenceRef = useRef(0);
  const activeHistoryRef = useRef<{
    channelId: number;
    generation: number;
    controller: AbortController;
  } | null>(null);
  const scrollAfterLoadGenerationRef = useRef<number | null>(null);
  const replyTargetIdRef = useRef(replyTargetId);
  replyTargetIdRef.current = replyTargetId;
  const draftVersionRef = useRef(0);
  const submissionVersionRef = useRef(0);
  const replyTargetVersionRef = useRef(0);
  const userIdRef = useRef(user?.id ?? null);
  userIdRef.current = user?.id ?? null;
  const hasSeenInitialUserProfileRef = useRef(false);
  const pendingScrollToBottomRef = useRef<{
    channelId: number;
    generation: number;
    afterScroll?: () => void;
  } | null>(null);
  const [validatedThreadKey, setValidatedThreadKey] = useState<string | null>(null);

  const scheduleActiveChannelMarkRead = () => {
    if (!messagesScrollRef.current) return;
    const active = activeHistoryRef.current;
    if (!active || active.channelId !== validChannelId) return;
    const { channelId, generation } = active;

    const marker = getViewportReadMarkerState(messagesScrollRef.current);
    if (
      !marker.rendererEligible ||
      !marker.nearBottom ||
      marker.lastVisibleTopLevelMessageId === null
    ) {
      return;
    }

    const markReadKey = `${channelId}:${marker.lastVisibleTopLevelMessageId}`;
    const pendingMarkRead = pendingMarkReadRef.current;
    if (
      lastMarkReadKeyRef.current === markReadKey ||
      (pendingMarkRead?.key === markReadKey && pendingMarkRead.generation === generation)
    )
      return;
    if (markReadTimerRef.current) clearTimeout(markReadTimerRef.current);
    const messageId = marker.lastVisibleTopLevelMessageId;
    markReadTimerRef.current = setTimeout(() => {
      markReadTimerRef.current = null;
      const currentScope = activeHistoryRef.current;
      if (
        !messagesScrollRef.current ||
        currentScope?.channelId !== channelId ||
        currentScope.generation !== generation
      ) {
        return;
      }
      const current = getViewportReadMarkerState(messagesScrollRef.current);
      if (
        !current.rendererEligible ||
        !current.nearBottom ||
        current.lastVisibleTopLevelMessageId !== messageId
      ) {
        return;
      }
      const ownership = {
        key: markReadKey,
        channelId,
        generation,
        token: ++markReadTokenRef.current,
      };
      pendingMarkReadRef.current = ownership;
      void readStates
        .markRead(channelId, messageId)
        .then((accepted) => {
          const acceptedScope = activeHistoryRef.current;
          if (
            accepted &&
            acceptedScope?.channelId === channelId &&
            acceptedScope.generation === generation &&
            accepted.channel_id === channelId
          ) {
            lastMarkReadKeyRef.current = markReadKey;
          }
        })
        .finally(() => {
          if (pendingMarkReadRef.current === ownership) pendingMarkReadRef.current = null;
        });
    }, 120);
  };

  const scrollMessagesToBottom = (afterScroll?: () => void) => {
    if (!messagesScrollRef.current) return;
    messagesScrollRef.current.scrollTop = messagesScrollRef.current.scrollHeight;
    setHasNewMessagesBelow(false);
    afterScroll?.();
  };

  const requestScrollMessagesToBottom = (afterScroll?: () => void) => {
    const active = activeHistoryRef.current;
    if (!active) return;
    pendingScrollToBottomRef.current = {
      channelId: active.channelId,
      generation: active.generation,
      afterScroll,
    };
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
    const active = activeHistoryRef.current;
    if (!pendingScroll) return;
    pendingScrollToBottomRef.current = null;
    if (
      active?.channelId !== pendingScroll.channelId ||
      active.generation !== pendingScroll.generation ||
      timelineRef.current.channelId !== pendingScroll.channelId ||
      timelineRef.current.generation !== pendingScroll.generation
    ) {
      return;
    }
    scrollMessagesToBottom(pendingScroll.afterScroll);
  }, [scrollToBottomRequestVersion]);

  const showNewMessagesBelow = hasNewMessagesBelow;

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

  const mentionUsers = Array.from(mentionUserCache.values());

  const startHistoryLoad = (channelId: number, scrollAfterLoad: boolean) => {
    activeHistoryRef.current?.controller.abort();
    const generation = ++generationSequenceRef.current;
    const controller = new AbortController();
    activeHistoryRef.current = { channelId, generation, controller };
    const carriesInitialScroll =
      scrollAfterLoadGenerationRef.current !== null && timelineRef.current.status !== "ready";
    if (scrollAfterLoad || carriesInitialScroll) {
      scrollAfterLoadGenerationRef.current = generation;
    }
    dispatch({ type: "loadStarted", channelId, generation });
    void listMessages(String(channelId), controller.signal).then(
      (loadedMessages) => {
        if (activeHistoryRef.current?.generation !== generation) return;
        primeMentionUsersFromMessages(loadedMessages);
        dispatch({
          type: "loadSucceeded",
          channelId,
          generation,
          messages: loadedMessages,
        });
      },
      (error: unknown) => {
        if (controller.signal.aborted || activeHistoryRef.current?.generation !== generation)
          return;
        dispatch({ type: "loadFailed", channelId, generation, error });
      },
    );
  };

  useEffect(() => {
    if (validChannelId === null) return;
    startHistoryLoad(validChannelId, true);
    return () => {
      activeHistoryRef.current?.controller.abort();
    };
  }, [validChannelId]);

  useLayoutEffect(() => {
    if (
      timeline.status !== "ready" ||
      scrollAfterLoadGenerationRef.current !== timeline.generation
    ) {
      return;
    }

    scrollAfterLoadGenerationRef.current = null;
    const channelId = timeline.channelId;
    const generation = timeline.generation;
    const ownsScroll = () => {
      const active = activeHistoryRef.current;
      return active?.channelId === channelId && active.generation === generation;
    };
    if (ownsScroll()) scrollMessagesToBottom(scheduleActiveChannelMarkRead);
    const expectedScrollTop = messagesScrollRef.current?.scrollTop;

    // Browser layout can grow the flex scroller after layout effects run. A
    // single owned post-paint correction keeps long initial histories pinned
    // to the newest row without leaking into a later channel generation or
    // overriding a user scroll that occurred before the frame.
    const frame = window.requestAnimationFrame(() => {
      if (ownsScroll() && messagesScrollRef.current?.scrollTop === expectedScrollTop) {
        scrollMessagesToBottom(scheduleActiveChannelMarkRead);
      }
    });
    return () => window.cancelAnimationFrame(frame);
  }, [timeline.channelId, timeline.generation, timeline.status]);

  // When the current user's profile changes, patch authored rows and references
  // through the same canonical reducer as history and live events.
  useEffect(() => {
    if (!user || timeline.channelId !== validChannelId) return;
    if (!hasSeenInitialUserProfileRef.current) {
      hasSeenInitialUserProfileRef.current = true;
      return;
    }
    dispatch({
      type: "currentUserProfileUpdated",
      channelId: timeline.channelId,
      generation: timeline.generation,
      user,
    });
  }, [user?.avatar_url, user?.display_name, user?.id, user?.username]);

  const channelPath = `/channel/${params.id}`;

  const threadPath = (rootMessageId: number) => `${channelPath}?thread=${rootMessageId}`;

  const openThread = (root: Message, options?: { focusComposer?: boolean }) => {
    void navigate(threadPath(root.id));
    setFocusComposerRootId(options?.focusComposer ? root.id : null);
  };

  const closeThread = () => {
    setFocusComposerRootId(null);
    void navigate(channelPath);
  };

  const selectInlineReplyTarget = (target: Message) => {
    if (target.deleted_at != null || target.parent_id != null) return;
    replyTargetVersionRef.current += 1;
    setReplyTargetId(target.id);
    queueMicrotask(() => composerRef.current?.focus());
  };

  const dismissInlineReplyTarget = () => {
    replyTargetVersionRef.current += 1;
    setReplyTargetId(null);
    queueMicrotask(() => composerRef.current?.focus());
  };

  useEffect(() => {
    if (params.id) {
      replyTargetVersionRef.current += 1;
      setReplyTargetId(null);
      setHasNewMessagesBelow(false);
      submissionVersionRef.current += 1;
      setSubmitting(false);
      lastTypingSentAtRef.current = 0;
      pendingScrollToBottomRef.current = null;
      if (markReadTimerRef.current) {
        clearTimeout(markReadTimerRef.current);
        markReadTimerRef.current = null;
      }
      lastMarkReadKeyRef.current = null;
      pendingMarkReadRef.current = null;
    }
  }, [params.id]);

  const requestedThreadKey =
    validChannelId !== null && requestedThreadRootId !== null
      ? `${validChannelId}:${requestedThreadRootId}`
      : null;
  const requestedTimelineRoot =
    requestedThreadRootId === null
      ? null
      : (timeline.messages.find((message) => message.id === requestedThreadRootId) ?? null);

  useEffect(() => {
    setValidatedThreadKey((current) => (current === requestedThreadKey ? current : null));
  }, [requestedThreadKey]);

  useEffect(() => {
    if (rawThreadQuery !== null && requestedThreadRootId === null) {
      setFocusComposerRootId(null);
      void navigate(channelPath, { replace: true });
      return;
    }
    if (
      requestedThreadRootId === null ||
      validChannelId === null ||
      timeline.channelId !== validChannelId ||
      timeline.status !== "ready"
    ) {
      return;
    }
    if (
      !requestedTimelineRoot ||
      requestedTimelineRoot.channel_id !== validChannelId ||
      requestedTimelineRoot.parent_id != null
    ) {
      setFocusComposerRootId(null);
      void navigate(channelPath, { replace: true });
      return;
    }

    const threadKey = `${validChannelId}:${requestedThreadRootId}`;
    if (validatedThreadKey === threadKey) return;

    const controller = new AbortController();
    void getThread(requestedThreadRootId, {}, controller.signal).then(
      (payload) => {
        if (controller.signal.aborted) return;
        if (
          payload.root.id !== requestedThreadRootId ||
          payload.root.channel_id !== validChannelId ||
          payload.root.parent_id != null
        ) {
          setFocusComposerRootId(null);
          void navigate(channelPath, { replace: true });
          return;
        }
        setValidatedThreadKey(threadKey);
      },
      () => {
        // Keep a plausible URL on transport/history errors, but never publish an
        // unvalidated panel.
      },
    );
    return () => controller.abort();
  }, [
    rawThreadQuery,
    requestedThreadRootId,
    timeline.channelId,
    timeline.status,
    requestedTimelineRoot,
    validChannelId,
    validatedThreadKey,
  ]);

  const handleMessageChange = (value: string) => {
    draftVersionRef.current += 1;
    setMessage(value);
    if (value.length === 0) return;
    const now = Date.now();
    if (now - lastTypingSentAtRef.current < TYPING_PING_INTERVAL_MS) return;
    lastTypingSentAtRef.current = now;
    void sendTyping(params.id ?? "");
  };

  const hasDraftContent = message.trim().length > 0 || photoSelection.photos.length > 0;

  const applyMessageUpdate = (updatedMessage: Message) => {
    const active = activeHistoryRef.current;
    if (!active || updatedMessage.channel_id !== active.channelId) return;
    primeMentionUsers(updatedMessage.mentions ?? []);
    dispatch({
      type: "messageUpdated",
      channelId: active.channelId,
      generation: active.generation,
      message: updatedMessage,
    });
    if (replyTargetIdRef.current === updatedMessage.id && updatedMessage.deleted_at != null) {
      replyTargetVersionRef.current += 1;
      setReplyTargetId(null);
    }
  };

  const submitMessage = async () => {
    if (submitting || !hasDraftContent) return;

    const submissionVersion = ++submissionVersionRef.current;
    const submittedChannelId = params.id ?? "";
    const submittedGeneration = activeHistoryRef.current?.generation ?? -1;
    const text = message;
    const submittedDraftVersion = draftVersionRef.current;
    const submittedPhotos = photoSelection.photos;
    const photos = submittedPhotos.map((photo) => photo.file);
    const target =
      replyTargetId === null
        ? null
        : (timeline.messages.find(
            (candidate) => candidate.id === replyTargetId && candidate.deleted_at == null,
          ) ?? null);
    const submittedReplyTargetVersion = replyTargetVersionRef.current;
    setSubmitting(true);
    try {
      const response = await sendMessage(submittedChannelId, text, photos, {
        replyToMessageId: target?.id,
      });
      if (!response.ok) return;
      const createdMessage = (await response
        .clone()
        .json()
        .catch(() => null)) as Message | null;
      const numericSubmittedChannelId = Number(submittedChannelId);
      if (
        !createdMessage ||
        !Number.isSafeInteger(createdMessage.id) ||
        createdMessage.channel_id !== numericSubmittedChannelId ||
        createdMessage.parent_id != null
      ) {
        window.alert(
          "The server returned an invalid message. Your draft and photos were preserved.",
        );
        return;
      }
      primeMentionUsers(createdMessage.mentions ?? []);
      const active = activeHistoryRef.current;
      if (
        paramsIdRef.current !== submittedChannelId ||
        active?.generation !== submittedGeneration
      ) {
        return;
      }
      if (createdMessage.channel_id === active.channelId) {
        dispatch({
          type: "messageCreated",
          channelId: active.channelId,
          generation: active.generation,
          message: createdMessage,
        });
      }
      if (draftVersionRef.current === submittedDraftVersion) {
        setMessage("");
      }
      if (replyTargetVersionRef.current === submittedReplyTargetVersion) {
        setReplyTargetId(null);
      }
      for (const photo of submittedPhotos) {
        photoSelection.removePhoto(photo.id);
      }
      lastTypingSentAtRef.current = 0;
      queueMicrotask(() => composerRef.current?.focus());
    } catch (e) {
      console.error("failed to send message", e);
    } finally {
      if (submissionVersionRef.current === submissionVersion) setSubmitting(false);
    }
  };

  useEffect(() => {
    const unsubCreated = events.onMessage((created) => {
      const active = activeHistoryRef.current;
      if (!active || created.channel_id !== active.channelId || created.parent_id != null) return;
      const isNew = !timelineRef.current.messages.some((message) => message.id === created.id);
      const shouldAutoFollow = messagesScrollRef.current
        ? messagesScrollRef.current.scrollHeight <= messagesScrollRef.current.clientHeight ||
          isNearScrollBottom(messagesScrollRef.current)
        : true;
      primeMentionUsers(created.mentions ?? []);
      dispatch({
        type: "messageCreated",
        channelId: active.channelId,
        generation: active.generation,
        message: created,
      });
      if (!isNew) return;
      if (shouldAutoFollow) requestScrollMessagesToBottom(scheduleActiveChannelMarkRead);
      else {
        setHasNewMessagesBelow(true);
        scheduleActiveChannelMarkRead();
      }
    });
    const unsubUpdated = events.onMessageUpdated(applyMessageUpdate);
    const unsubDeleted = events.onMessageDeleted((deletion) => {
      const active = activeHistoryRef.current;
      if (!active || deletion.channel_id !== active.channelId) return;
      dispatch({
        type: "messageHardDeleted",
        channelId: active.channelId,
        generation: active.generation,
        deletion,
      });
      if (replyTargetIdRef.current === deletion.id) setReplyTargetId(null);
    });
    const unsubEmbeds = events.onMessageEmbedsUpdated((update) => {
      const active = activeHistoryRef.current;
      if (!active || update.channel_id !== active.channelId) return;
      dispatch({
        type: "messageEmbedsUpdated",
        channelId: active.channelId,
        generation: active.generation,
        update,
      });
    });
    const unsubReactions = events.onMessageReactionsUpdated((update) => {
      const active = activeHistoryRef.current;
      if (!active || update.channel_id !== active.channelId) return;
      dispatch({
        type: "messageReactionsUpdated",
        channelId: active.channelId,
        generation: active.generation,
        update,
        currentUserId: userIdRef.current,
      });
    });
    const unsubThreadReply = events.onThreadReplyCreated((update) => {
      const active = activeHistoryRef.current;
      if (!active || update.channel_id !== active.channelId) return;
      primeMentionUsers(update.reply.mentions ?? []);
      dispatch({
        type: "threadSummaryCreated",
        channelId: active.channelId,
        generation: active.generation,
        update,
      });
    });
    const unsubThreadReplyDeleted = events.onThreadReplyDeleted((update) => {
      const active = activeHistoryRef.current;
      if (!active || update.channel_id !== active.channelId) return;
      dispatch({
        type: "threadSummaryDeleted",
        channelId: active.channelId,
        generation: active.generation,
        update,
      });
    });
    const unsubConnected = events.onConnected(() => {
      const active = activeHistoryRef.current;
      if (active) startHistoryLoad(active.channelId, false);
    });
    const onFocusOrVisibility = () => scheduleActiveChannelMarkRead();
    window.addEventListener("focus", onFocusOrVisibility);
    document.addEventListener("visibilitychange", onFocusOrVisibility);
    return () => {
      unsubCreated();
      unsubUpdated();
      unsubDeleted();
      unsubEmbeds();
      unsubReactions();
      unsubThreadReply();
      unsubThreadReplyDeleted();
      unsubConnected();
      window.removeEventListener("focus", onFocusOrVisibility);
      document.removeEventListener("visibilitychange", onFocusOrVisibility);
      if (markReadTimerRef.current) clearTimeout(markReadTimerRef.current);
    };
  }, [events]);

  // TODO(reno): Can the router ensure this parameter exists?
  if (!params.id) {
    return <div>Error: channel required</div>;
  }

  const currentReplyTarget =
    replyTargetId === null
      ? null
      : (timeline.messages.find(
          (candidate) => candidate.id === replyTargetId && candidate.deleted_at == null,
        ) ?? null);

  return (
    <div className="flex h-full min-h-0 flex-1 flex-col overflow-hidden bg-background text-foreground">
      <section className="flex-shrink-0 border-b border-border bg-background px-4 py-3 shadow-sm">
        <h1 className="text-lg font-semibold tracking-tight"># {channel?.name ?? params.id}</h1>
      </section>

      <ScreenShareViewer />
      <LocalCameraTile />
      <RemoteCameraTiles />

      <div className="flex min-h-0 min-w-0 flex-1 overflow-hidden">
        <div
          ref={(el) => {
            messagesScrollRef.current = el;
          }}
          className="min-h-0 min-w-0 flex flex-1 flex-col overflow-y-auto overscroll-y-none"
          role="region"
          aria-label="Messages"
          onScroll={handleMessagesScroll}
        >
          <ChannelMessages
            key={`${timeline.channelId}:${timeline.generation}`}
            channelId={timeline.channelId}
            generation={timeline.generation}
            messages={timeline.channelId === validChannelId ? timeline.messages : []}
            loading={timeline.channelId !== validChannelId || timeline.status === "loading"}
            error={timeline.channelId === validChannelId ? timeline.error : null}
            currentUserId={user?.id ?? null}
            onOpenThread={openThread}
            onReplyToMessage={selectInlineReplyTarget}
            onMessageUpdated={applyMessageUpdate}
            onReactionsChange={(messageId, reactions) => {
              const existing = timeline.messages.find((message) => message.id === messageId);
              if (existing) applyMessageUpdate({ ...existing, reactions });
            }}
            mentionUsers={mentionUsers}
            onMentionUsers={primeMentionUsers}
            searchMentionUsers={searchUsers}
          />
          {showNewMessagesBelow ? (
            <div className="sticky bottom-4 z-10 flex justify-center px-4" aria-live="polite">
              <button
                type="button"
                className="rounded-full bg-primary px-4 py-2 text-sm font-semibold text-primary-foreground shadow-lg transition-colors hover:bg-primary/90 focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                aria-label="New messages below. Jump to latest messages"
                onClick={jumpToLatestMessages}
              >
                New messages — Jump to bottom
              </button>
            </div>
          ) : null}
        </div>
        {openThreadRootId !== null &&
          validatedThreadKey === `${validChannelId}:${openThreadRootId}` && (
            <ThreadPanel
              key={validatedThreadKey}
              rootMessageId={openThreadRootId}
              channelId={Number(params.id)}
              currentUserId={user?.id ?? null}
              currentUserName={user?.username ?? null}
              focusComposer={focusComposerRootId === openThreadRootId}
              onComposerFocusConsumed={() => setFocusComposerRootId(null)}
              onClose={closeThread}
              mentionUsers={mentionUsers}
              onMentionUsers={primeMentionUsers}
              searchMentionUsers={searchUsers}
            />
          )}
      </div>

      <section className="flex-shrink-0 p-4 border-t border-border">
        <TypingIndicator
          channelId={Number(params.id)}
          currentUserId={user?.id ?? null}
          events={events}
        />
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void submitMessage();
          }}
        >
          <SelectedPhotoPreviewList
            photos={photoSelection.photos}
            error={photoSelection.error}
            errorId={photoSelectionErrorId}
            disabled={submitting}
            onRemove={photoSelection.removePhoto}
          />
          {currentReplyTarget ? (
            <MessageReferencePreview
              id={replyBannerId}
              reference={currentReplyTarget}
              className="mb-2 flex min-w-0 items-center gap-2 rounded-md border border-primary/20 bg-primary/5 px-3 py-2 text-sm text-foreground"
              authorClass="shrink-0 font-semibold"
              textClass="min-w-0 flex-1 truncate text-muted-foreground"
              authorPrefix="Replying to "
              ariaLabelPrefix="Inline reply target: "
              role="status"
              ariaLive="polite"
            >
              <button
                type="button"
                className="rounded-md px-2 py-1 text-sm font-medium text-primary transition-colors hover:bg-primary/10 focus:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                aria-label={`Dismiss inline reply to message by ${messageDisplayName(
                  currentReplyTarget,
                )}: ${inlineReplyPreviewText(currentReplyTarget)}`}
                onClick={dismissInlineReplyTarget}
              >
                Cancel
              </button>
            </MessageReferencePreview>
          ) : null}
          <div className="flex items-center gap-2">
            <PhotoAttachControl
              onFilesSelected={photoSelection.addFiles}
              disabled={submitting}
              describedBy={photoSelection.error ? photoSelectionErrorId : undefined}
            />
            <MessageInput
              value={message}
              onChange={handleMessageChange}
              ariaLabel="New message"
              placeholder="Send a new message..."
              describedBy={currentReplyTarget ? replyBannerId : undefined}
              mentionUsers={mentionUsers}
              onMentionUsers={primeMentionUsers}
              searchMentionUsers={searchUsers}
              inputRef={(el) => {
                composerRef.current = el;
              }}
            />
            <Button type="submit" size="lg" disabled={submitting || !hasDraftContent}>
              Send
            </Button>
          </div>
        </form>
      </section>
    </div>
  );
}
