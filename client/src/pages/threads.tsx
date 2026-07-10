import { Link } from "react-router-dom";
import { useCallableResource, List, If } from "../hooks/react-state";
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
    ? "border-primary"
    : mentionedCurrentUser
      ? "border-primary/50"
      : defaultBorderClass;
  const stateClass = mentionedCurrentUser
    ? "bg-primary/10 ring-1 ring-inset ring-primary/20"
    : authoredByCurrentUser
      ? "bg-primary/5"
      : defaultStateClass;

  return `${borderClass} ${stateClass}`;
}

function PreviewAttachments(props: { message: Message }) {
  return (
    <If when={!isDeletedMessage(props.message) && props.message.attachments.length > 0}>
      <AttachmentGrid
        attachments={props.message.attachments}
        authorName={messageDisplayName(props.message)}
      />
    </If>
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
      className={`rounded-md border-l-4 px-3 py-2 ${previewMessageStateClass(
        props.reply,
        props.currentUserId,
        "border-transparent",
        "bg-muted",
      )}`}
    >
      <If
        when={!isDeletedMessage(props.reply)}
        fallback={<p className="italic text-muted-foreground">Reply deleted</p>}
      >
        <p className="text-xs font-semibold text-muted-foreground">
          {messageDisplayName(props.reply)}
        </p>
        <MessageText
          text={props.reply.text}
          mentions={props.reply.mentions ?? []}
          currentUserId={props.currentUserId}
          className="mt-1 whitespace-pre-wrap break-words text-sm text-foreground"
        />
        <PreviewAttachments message={props.reply} />
      </If>
    </li>
  );
}

export default function ThreadsView() {
  const { user } = useAuth();
  const currentUserId = () => user()?.id ?? null;
  const [threads] = useCallableResource(listParticipatedThreads);

  const previewMentionsCurrentUser = (thread: {
    root: Message;
    recent_replies: readonly Message[];
  }) =>
    messageMentionsCurrentUser(thread.root, currentUserId()) ||
    thread.recent_replies.some((reply) => messageMentionsCurrentUser(reply, currentUserId()));

  return (
    <section className="flex h-full flex-col bg-background text-foreground">
      <header className="flex-shrink-0 border-b border-border bg-muted p-4 text-foreground">
        <h1 className="text-2xl font-bold">Threads</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Conversations you have participated in.
        </p>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto p-4">
        <If when={threads.loading}>
          <p className="text-muted-foreground">Loading threads...</p>
        </If>

        <If when={threads.error}>
          <p role="alert" className="text-destructive">
            Error loading threads: {String(threads.error)}
          </p>
        </If>

        <If when={threads()?.length === 0}>
          <p className="text-muted-foreground">No participated threads yet.</p>
        </If>

        <div className="space-y-4">
          <List each={threads() ?? []}>
            {(thread) => {
              const replyCountText = () =>
                `${thread.reply_count} ${thread.reply_count === 1 ? "reply" : "replies"}`;
              const threadHref = () => `/channel/${thread.channel.id}?thread=${thread.root.id}`;
              return (
                <article
                  data-mentioned-current-user={
                    previewMentionsCurrentUser(thread) ? "true" : undefined
                  }
                  className={`rounded-lg border p-4 shadow-sm transition-colors ${
                    previewMentionsCurrentUser(thread)
                      ? "border-primary/50 bg-primary/5 ring-1 ring-inset ring-primary/20"
                      : "border-border bg-card"
                  }`}
                >
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <p className="text-sm font-semibold text-primary"># {thread.channel.name}</p>
                    <p className="text-xs text-muted-foreground">
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
                    className={`mt-3 rounded-r-md border-l-4 py-2 pl-3 pr-2 ${previewMessageStateClass(
                      thread.root,
                      currentUserId(),
                      "border-border",
                      "",
                    )}`}
                  >
                    <If
                      when={!isDeletedMessage(thread.root)}
                      fallback={
                        <p className="italic text-muted-foreground">Original message deleted</p>
                      }
                    >
                      <p className="text-sm font-semibold text-muted-foreground">
                        {messageDisplayName(thread.root)}
                      </p>
                      <MessageText
                        text={thread.root.text}
                        mentions={thread.root.mentions ?? []}
                        currentUserId={currentUserId()}
                        className="mt-1 whitespace-pre-wrap break-words text-foreground"
                      />
                      <PreviewAttachments message={thread.root} />
                    </If>
                  </div>

                  <If when={thread.recent_replies.length > 0}>
                    <ol className="mt-3 space-y-2" aria-label="Recent replies">
                      <List each={thread.recent_replies}>
                        {(reply) => <ReplyPreview reply={reply} currentUserId={currentUserId()} />}
                      </List>
                    </ol>
                  </If>

                  <Link
                    to={threadHref()}
                    className="mt-3 inline-flex text-sm font-medium text-primary transition-colors hover:text-primary/80"
                    aria-label={`Open full thread in # ${thread.channel.name}`}
                  >
                    View all replies
                  </Link>
                </article>
              );
            }}
          </List>
        </div>
      </div>
    </section>
  );
}
