import {
  cloneElement,
  Fragment,
  isValidElement,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type MouseEvent as ReactMouseEvent,
  type ReactElement,
  type ReactNode,
} from "react";
import { resolveServerUrl, type Channel, type CustomEmoji, type MentionUser } from "../api";
import { useOptionalChannels } from "../contexts/channels";
import { useOptionalCustomEmojis } from "../contexts/custom-emojis";
import { parseCustomEmojiMarkers } from "../emoji/custom-emojis";
import { linkifyText } from "../linkify";
import { parseChannelMarkers } from "../mentions/channel-mentions";
import { mentionDisplayName, parseMentionMarkers } from "../mentions/mentions";
import Avatar from "./avatar";

const DEFAULT_TEXT_CLASS = "whitespace-pre-wrap break-words [overflow-wrap:anywhere]";
const DEFAULT_LINK_CLASS = "text-primary hover:underline break-all";
const DEFAULT_MENTION_CLASS =
  "inline rounded bg-primary/10 px-1 py-0 font-medium text-primary align-baseline";
const DEFAULT_CHANNEL_MENTION_CLASS =
  "inline rounded bg-muted px-1 py-0 font-medium text-foreground align-baseline";
const CHANNEL_LINK_CLASS = `${DEFAULT_CHANNEL_MENTION_CLASS} hover:bg-accent hover:underline focus:outline-none focus-visible:ring-2 focus-visible:ring-ring`;
const INTERACTIVE_MENTION_CLASS = `${DEFAULT_MENTION_CLASS} cursor-pointer border-0 hover:bg-primary/20 focus:outline-none focus-visible:ring-2 focus-visible:ring-ring`;
const CURRENT_USER_MENTION_CLASS =
  "inline rounded bg-primary/20 px-1 py-0 font-semibold text-primary align-baseline";
const INTERACTIVE_CURRENT_USER_MENTION_CLASS = `${CURRENT_USER_MENTION_CLASS} cursor-pointer border-0 hover:bg-primary/30 focus:outline-none focus-visible:ring-2 focus-visible:ring-ring`;
const PREVIEW_WIDTH = 256;
const PREVIEW_VERTICAL_GAP = 8;

export type MentionClickHandler = (
  user: MentionUser,
  event: ReactMouseEvent<HTMLButtonElement>,
) => void;

export interface RichTextRenderOptions {
  text: string;
  mentions?: readonly MentionUser[];
  customEmojiById: (id: number) => CustomEmoji | null;
  channels?: readonly Channel[];
  currentUserId?: number | null;
  linkClass?: string;
  onMentionClick?: MentionClickHandler;
}

interface MarkerRange {
  type: "mention" | "custom-emoji" | "channel";
  start: number;
  end: number;
  marker: string;
  id: number;
  storedName?: string;
}

interface MentionPreviewState {
  user: MentionUser;
  anchor: HTMLElement;
}

function mentionUsersById(mentions: readonly MentionUser[]): Map<number, MentionUser> {
  const byId = new Map<number, MentionUser>();
  for (const mention of mentions) {
    if (!byId.has(mention.id)) byId.set(mention.id, mention);
  }
  return byId;
}

function channelsById(channels: readonly Channel[]): Map<number, Channel> {
  const byId = new Map<number, Channel>();
  for (const channel of channels) {
    if (!byId.has(channel.id)) byId.set(channel.id, channel);
  }
  return byId;
}

function mentionLabel(user: MentionUser): string {
  return `@${mentionDisplayName(user)}`;
}

function mentionAccessibleName(user: MentionUser): string {
  return `Mention ${mentionDisplayName(user)} (@${user.username})`;
}

function mentionPreviewLabel(user: MentionUser): string {
  return `Profile preview for ${mentionDisplayName(user)} (@${user.username})`;
}

function textMentionsUser(text: string, userId: number): boolean {
  return parseMentionMarkers(text).some((token) => token.type === "mention" && token.id === userId);
}

function mentionClass(user: MentionUser, currentUserId: number | null | undefined): string {
  return user.id === currentUserId ? CURRENT_USER_MENTION_CLASS : DEFAULT_MENTION_CLASS;
}

function interactiveMentionClass(
  user: MentionUser,
  currentUserId: number | null | undefined,
): string {
  return user.id === currentUserId
    ? INTERACTIVE_CURRENT_USER_MENTION_CLASS
    : INTERACTIVE_MENTION_CLASS;
}

function renderCustomEmoji(
  marker: string,
  storedName: string,
  byId: (id: number) => CustomEmoji | null,
  id: number,
): ReactElement {
  const emoji = byId(id);
  if (!emoji) {
    return <span title={`Custom emoji ${marker} is unavailable`}>:{storedName}:</span>;
  }

  const label = `:${emoji.name}:`;
  return (
    <img
      src={resolveServerUrl(emoji.image_url)}
      alt={label}
      title={emoji.deleted_at === null ? label : `${label} (deleted)`}
      className="inline-block h-6 w-6 align-text-bottom object-contain"
    />
  );
}

function collectMarkerRanges(text: string): MarkerRange[] {
  const ranges: MarkerRange[] = [];
  let cursor = 0;
  for (const token of parseMentionMarkers(text)) {
    if (token.type === "text") {
      cursor += token.value.length;
      continue;
    }

    ranges.push({
      type: "mention",
      start: cursor,
      end: cursor + token.marker.length,
      marker: token.marker,
      id: token.id,
    });
    cursor += token.marker.length;
  }

  cursor = 0;
  for (const token of parseCustomEmojiMarkers(text)) {
    if (token.type === "text") {
      cursor += token.value.length;
      continue;
    }

    ranges.push({
      type: "custom-emoji",
      start: cursor,
      end: cursor + token.marker.length,
      marker: token.marker,
      id: token.id,
      storedName: token.storedName,
    });
    cursor += token.marker.length;
  }

  cursor = 0;
  for (const token of parseChannelMarkers(text)) {
    if (token.type === "text") {
      cursor += token.value.length;
      continue;
    }

    ranges.push({
      type: "channel",
      start: cursor,
      end: cursor + token.marker.length,
      marker: token.marker,
      id: token.id,
    });
    cursor += token.marker.length;
  }

  return ranges.sort((a, b) => a.start - b.start || a.end - b.end);
}

function renderPlainTextSegment(
  text: string,
  mentionedUsers: ReadonlyMap<number, MentionUser>,
  knownChannels: ReadonlyMap<number, Channel>,
  options: RichTextRenderOptions,
): ReactNode[] {
  const parts: ReactNode[] = [];
  let cursor = 0;

  for (const range of collectMarkerRanges(text)) {
    if (range.start < cursor) continue;
    if (range.start > cursor) parts.push(text.slice(cursor, range.start));

    if (range.type === "mention") {
      const user = mentionedUsers.get(range.id);
      if (user) {
        parts.push(renderMention(user, options));
      } else {
        parts.push(range.marker);
      }
    } else if (range.type === "channel") {
      const channel = knownChannels.get(range.id);
      parts.push(channel ? renderChannelMention(channel) : range.marker);
    } else {
      parts.push(
        renderCustomEmoji(
          range.marker,
          range.storedName ?? "unknown",
          options.customEmojiById,
          range.id,
        ),
      );
    }

    cursor = range.end;
  }

  if (cursor < text.length) parts.push(text.slice(cursor));
  return parts;
}

function renderMention(user: MentionUser, options: RichTextRenderOptions): ReactElement {
  if (!options.onMentionClick) {
    return (
      <span className={mentionClass(user, options.currentUserId)} title={`@${user.username}`}>
        {mentionLabel(user)}
      </span>
    );
  }

  return (
    <button
      type="button"
      className={interactiveMentionClass(user, options.currentUserId)}
      title={`@${user.username}`}
      aria-label={mentionAccessibleName(user)}
      aria-haspopup="dialog"
      onClick={(event) => options.onMentionClick?.(user, event)}
    >
      {mentionLabel(user)}
    </button>
  );
}

function renderChannelMention(channel: Channel): ReactElement {
  const label = `#${channel.name}`;
  if (channel.type !== "text") {
    return (
      <span className={DEFAULT_CHANNEL_MENTION_CLASS} title={`${label} (${channel.type})`}>
        {label}
      </span>
    );
  }

  return (
    <a className={CHANNEL_LINK_CLASS} href={`/channel/${channel.id}`} title={label}>
      {label}
    </a>
  );
}

function keyedRichTextPart(part: ReactNode, key: string): ReactElement {
  if (isValidElement(part)) return cloneElement(part, { key });
  return <Fragment key={key}>{part}</Fragment>;
}

export function renderRichText(options: RichTextRenderOptions): ReactNode[] {
  const mentionedUsers = mentionUsersById(options.mentions ?? []);
  const knownChannels = channelsById(options.channels ?? []);
  const linkClass = options.linkClass ?? DEFAULT_LINK_CLASS;
  const parts: ReactNode[] = [];
  let partIndex = 0;
  const pushPart = (part: ReactNode) => {
    parts.push(keyedRichTextPart(part, `part-${partIndex}`));
    partIndex += 1;
  };

  for (const token of linkifyText(options.text)) {
    if (token.type === "link") {
      pushPart(
        <a href={token.url} target="_blank" rel="noopener noreferrer" className={linkClass}>
          {token.url}
        </a>,
      );
    } else {
      for (const part of renderPlainTextSegment(
        token.value,
        mentionedUsers,
        knownChannels,
        options,
      )) {
        pushPart(part);
      }
    }
  }

  return parts;
}

export function renderTextWithMentionsAndCustomEmojis(
  text: string,
  mentions: readonly MentionUser[],
  byId: (id: number) => CustomEmoji | null,
): ReactNode[] {
  return renderRichText({ text, mentions, customEmojiById: byId });
}

function previewPosition(anchor: HTMLElement): CSSProperties {
  const rect = anchor.getBoundingClientRect();
  const viewportWidth = window.innerWidth > 0 ? window.innerWidth : 1024;
  const viewportHeight = window.innerHeight > 0 ? window.innerHeight : 768;
  const left = Math.max(8, Math.min(rect.left, viewportWidth - PREVIEW_WIDTH - 8));
  const top = Math.max(8, Math.min(rect.bottom + PREVIEW_VERTICAL_GAP, viewportHeight - 160));

  return {
    left: `${left}px`,
    top: `${top}px`,
    width: `${PREVIEW_WIDTH}px`,
  };
}

export default function MessageText(props: {
  text: string;
  mentions?: readonly MentionUser[];
  currentUserId?: number | null;
  className?: string;
  linkClass?: string;
  onMentionClick?: MentionClickHandler;
  channels?: readonly Channel[];
}) {
  const customEmojis = useOptionalCustomEmojis();
  const optionalChannels = useOptionalChannels();
  const customEmojiById = (id: number) => customEmojis?.byId(id) ?? null;
  const channels = props.channels ?? optionalChannels?.channels() ?? [];
  const previewId = useId();
  const [preview, setPreview] = useState<MentionPreviewState | null>(null);
  const previewRef = useRef<MentionPreviewState | null>(preview);
  previewRef.current = preview;
  const popoverRef = useRef<HTMLDivElement | null>(null);
  const previewOpenedAtRef = useRef(0);
  const mentionedUsers = useMemo(() => mentionUsersById(props.mentions ?? []), [props.mentions]);
  const previewUser = preview ? (mentionedUsers.get(preview.user.id) ?? preview.user) : null;

  const closePreview = () => setPreview(null);

  const handleMentionClick: MentionClickHandler = (user, event) => {
    previewOpenedAtRef.current = performance.now();
    event.stopPropagation();
    const anchor = event.currentTarget;
    setPreview({ user, anchor });
    props.onMentionClick?.(user, event);
  };

  useEffect(() => {
    if (!preview) return;
    if (!textMentionsUser(props.text, preview.user.id)) closePreview();
  });

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      const state = previewRef.current;
      if (!state) return;

      const target = event.target;
      if (!(target instanceof Node)) return;
      if (state.anchor.contains(target) || popoverRef.current?.contains(target)) return;
      closePreview();
    };

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") closePreview();
    };

    const onScroll = (event: Event) => {
      if (event.isTrusted && performance.now() - previewOpenedAtRef.current < 250) return;
      closePreview();
    };

    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("keydown", onKeyDown);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("scroll", onScroll, true);
    };
  }, []);

  return (
    <div className={props.className ?? DEFAULT_TEXT_CLASS}>
      {renderRichText({
        text: props.text,
        mentions: props.mentions ?? [],
        customEmojiById,
        channels,
        currentUserId: props.currentUserId,
        linkClass: props.linkClass,
        onMentionClick: handleMentionClick,
      })}
      {preview && previewUser ? (
        <div
          id={previewId}
          ref={(el) => {
            popoverRef.current = el;
          }}
          role="dialog"
          aria-label={mentionPreviewLabel(previewUser)}
          className="fixed z-50 rounded-lg border border-border bg-popover p-3 text-popover-foreground shadow-lg"
          style={previewPosition(preview.anchor)}
        >
          <div className="flex items-center gap-3">
            <Avatar
              url={previewUser.avatar_url}
              username={mentionDisplayName(previewUser)}
              size={48}
            />
            <div className="min-w-0">
              <div className="truncate text-sm font-semibold text-foreground">
                {mentionDisplayName(previewUser)}
              </div>
              <div className="truncate text-sm text-muted-foreground">@{previewUser.username}</div>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
