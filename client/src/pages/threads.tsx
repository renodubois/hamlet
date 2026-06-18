import { A } from "@solidjs/router";
import { createResource, For, Show } from "solid-js";
import { listParticipatedThreads, messageDisplayName, type Message } from "../api";
import AttachmentGrid from "../components/attachment-grid";
import MessageText from "../components/message-text";
import { useAuth } from "../contexts/auth";
import { messageMentionsCurrentUser } from "../mentions/mentions";

function formatTimestamp(timestampMicros: number): string {
  return new Date(Math.floor(timestampMicros / 1000)).toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function formatDateTime(timestampMicros: number): string {
  return new Date(Math.floor(timestampMicros / 1000)).toISOString();
}

function isDeletedMessage(message: Message): boolean {
  return message.deleted_at != null;
}

function isAuthoredByCurrentUser(message: Message, currentUserId: number | null): boolean {
  return !isDeletedMessage(message) && currentUserId !== null && message.user_id === currentUserId;
}

function previewMessageStateClass(
  message: Message,
  currentUserId: number | null,
  defaultBorderClass: string,
  defaultStateClass: string,
): string {
  const authoredByCurrentUser = isAuthoredByCurrentUser(message, currentUserId);
  const mentionedCurrentUser = messageMentionsCurrentUser(message, currentUserId);
  const borderClass = authoredByCurrentUser
    ? "border-blue-400"
    : mentionedCurrentUser
      ? "border-yellow-300"
      : defaultBorderClass;
  const stateClass = mentionedCurrentUser
    ? "bg-yellow-50 ring-1 ring-inset ring-yellow-300"
    : authoredByCurrentUser
      ? "bg-blue-50/50"
      : defaultStateClass;

  return `${borderClass} ${stateClass}`;
}

function PreviewAttachments(props: { message: Message }) {
  return (
    <Show when={!isDeletedMessage(props.message) && props.message.attachments.length > 0}>
      <AttachmentGrid
        attachments={props.message.attachments}
        authorName={messageDisplayName(props.message)}
      />
    </Show>
  );
}

function ReplyPreview(props: { reply: Message; currentUserId: number | null }) {
  const authoredByCurrentUser = () => isAuthoredByCurrentUser(props.reply, props.currentUserId);
  const mentionedCurrentUser = () => messageMentionsCurrentUser(props.reply, props.currentUserId);

  return (
    <li
      data-message-id={String(props.reply.id)}
      data-authored-by-current-user={authoredByCurrentUser() ? "true" : undefined}
      data-mentioned-current-user={mentionedCurrentUser() ? "true" : undefined}
      class={`rounded-md border-l-4 px-3 py-2 ${previewMessageStateClass(
        props.reply,
        props.currentUserId,
        "border-transparent",
        "bg-gray-50",
      )}`}
    >
      <Show
        when={!isDeletedMessage(props.reply)}
        fallback={<p class="italic text-gray-500">Reply deleted</p>}
      >
        <p class="text-xs font-semibold text-gray-600">{messageDisplayName(props.reply)}</p>
        <MessageText
          text={props.reply.text}
          mentions={props.reply.mentions ?? []}
          currentUserId={props.currentUserId}
          class="mt-1 whitespace-pre-wrap break-words text-sm text-gray-800"
        />
        <PreviewAttachments message={props.reply} />
      </Show>
    </li>
  );
}

export default function ThreadsView() {
  const { user } = useAuth();
  const currentUserId = () => user()?.id ?? null;
  const [threads] = createResource(listParticipatedThreads);

  const previewMentionsCurrentUser = (thread: {
    root: Message;
    recent_replies: readonly Message[];
  }) =>
    messageMentionsCurrentUser(thread.root, currentUserId()) ||
    thread.recent_replies.some((reply) => messageMentionsCurrentUser(reply, currentUserId()));

  return (
    <section class="flex h-full flex-col bg-white text-gray-900">
      <header class="flex-shrink-0 border-b border-gray-200 bg-gray-100 p-4 text-gray-700">
        <h1 class="text-2xl font-bold">Threads</h1>
        <p class="mt-1 text-sm text-gray-600">Conversations you have participated in.</p>
      </header>

      <div class="min-h-0 flex-1 overflow-y-auto p-4">
        <Show when={threads.loading}>
          <p class="text-gray-600">Loading threads...</p>
        </Show>

        <Show when={threads.error}>
          <p role="alert" class="text-red-700">
            Error loading threads: {String(threads.error)}
          </p>
        </Show>

        <Show when={threads()?.length === 0}>
          <p class="text-gray-600">No participated threads yet.</p>
        </Show>

        <div class="space-y-4">
          <For each={threads() ?? []}>
            {(thread) => {
              const replyCountText = () =>
                `${thread.reply_count} ${thread.reply_count === 1 ? "reply" : "replies"}`;
              const threadHref = () => `/channel/${thread.channel.id}?thread=${thread.root.id}`;
              return (
                <article
                  data-mentioned-current-user={
                    previewMentionsCurrentUser(thread) ? "true" : undefined
                  }
                  class={`rounded-lg border p-4 shadow-sm transition-colors ${
                    previewMentionsCurrentUser(thread)
                      ? "border-yellow-300 bg-yellow-50/40 ring-1 ring-inset ring-yellow-200"
                      : "border-gray-200 bg-white"
                  }`}
                >
                  <div class="flex flex-wrap items-center justify-between gap-2">
                    <p class="text-sm font-semibold text-blue-700"># {thread.channel.name}</p>
                    <p class="text-xs text-gray-500">
                      {replyCountText()} •{" "}
                      <time dateTime={formatDateTime(thread.last_reply_created_at)}>
                        Last reply {formatTimestamp(thread.last_reply_created_at)}
                      </time>
                    </p>
                  </div>

                  <div
                    data-message-id={String(thread.root.id)}
                    data-authored-by-current-user={
                      isAuthoredByCurrentUser(thread.root, currentUserId()) ? "true" : undefined
                    }
                    data-mentioned-current-user={
                      messageMentionsCurrentUser(thread.root, currentUserId()) ? "true" : undefined
                    }
                    class={`mt-3 rounded-r-md border-l-4 py-2 pl-3 pr-2 ${previewMessageStateClass(
                      thread.root,
                      currentUserId(),
                      "border-gray-200",
                      "",
                    )}`}
                  >
                    <Show
                      when={!isDeletedMessage(thread.root)}
                      fallback={<p class="italic text-gray-500">Original message deleted</p>}
                    >
                      <p class="text-sm font-semibold text-gray-700">
                        {messageDisplayName(thread.root)}
                      </p>
                      <MessageText
                        text={thread.root.text}
                        mentions={thread.root.mentions ?? []}
                        currentUserId={currentUserId()}
                        class="mt-1 whitespace-pre-wrap break-words text-gray-900"
                      />
                      <PreviewAttachments message={thread.root} />
                    </Show>
                  </div>

                  <Show when={thread.recent_replies.length > 0}>
                    <ol class="mt-3 space-y-2" aria-label="Recent replies">
                      <For each={thread.recent_replies}>
                        {(reply) => <ReplyPreview reply={reply} currentUserId={currentUserId()} />}
                      </For>
                    </ol>
                  </Show>

                  <A
                    href={threadHref()}
                    class="mt-3 inline-flex text-sm font-medium text-blue-700 hover:text-blue-900"
                    aria-label={`Open full thread in # ${thread.channel.name}`}
                  >
                    View all replies
                  </A>
                </article>
              );
            }}
          </For>
        </div>
      </div>
    </section>
  );
}
