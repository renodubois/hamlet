import { A } from "@solidjs/router";
import { createResource, For, Show } from "solid-js";
import { listParticipatedThreads, messageDisplayName, type Message } from "../api";

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

function ReplyPreview(props: { reply: Message }) {
  return (
    <li class="rounded-md bg-gray-50 px-3 py-2">
      <p class="text-xs font-semibold text-gray-600">{messageDisplayName(props.reply)}</p>
      <p class="mt-1 whitespace-pre-wrap break-words text-sm text-gray-800">{props.reply.text}</p>
    </li>
  );
}

export default function ThreadsView() {
  const [threads] = createResource(listParticipatedThreads);

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
                <article class="rounded-lg border border-gray-200 bg-white p-4 shadow-sm">
                  <div class="flex flex-wrap items-center justify-between gap-2">
                    <p class="text-sm font-semibold text-blue-700"># {thread.channel.name}</p>
                    <p class="text-xs text-gray-500">
                      {replyCountText()} •{" "}
                      <time dateTime={formatDateTime(thread.last_reply_created_at)}>
                        Last reply {formatTimestamp(thread.last_reply_created_at)}
                      </time>
                    </p>
                  </div>

                  <div class="mt-3 border-l-4 border-gray-200 pl-3">
                    <Show
                      when={!thread.root.deleted_at}
                      fallback={<p class="italic text-gray-500">Original message deleted</p>}
                    >
                      <p class="text-sm font-semibold text-gray-700">
                        {messageDisplayName(thread.root)}
                      </p>
                      <p class="mt-1 whitespace-pre-wrap break-words text-gray-900">
                        {thread.root.text}
                      </p>
                    </Show>
                  </div>

                  <Show when={thread.recent_replies.length > 0}>
                    <ol class="mt-3 space-y-2" aria-label="Recent replies">
                      <For each={thread.recent_replies}>
                        {(reply) => <ReplyPreview reply={reply} />}
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
