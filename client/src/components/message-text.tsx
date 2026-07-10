import { cloneElement, Fragment, isValidElement } from "react";

import {
  If,
  useAfterRenderEffect,
  useComputedValue,
  useSignalState,
  useStableDomId,
  registerCleanup,
  useMountEffect,
  type JSX,
} from "../hooks/react-state";
import { resolveServerUrl, type CustomEmoji, type MentionUser } from "../api";
import { useOptionalCustomEmojis } from "../contexts/custom-emojis";
import { parseCustomEmojiMarkers } from "../emoji/custom-emojis";
import { linkifyText } from "../linkify";
import { mentionDisplayName, parseMentionMarkers } from "../mentions/mentions";
import Avatar from "./avatar";

const DEFAULT_TEXT_CLASS = "whitespace-pre-wrap break-words [overflow-wrap:anywhere]";
const DEFAULT_LINK_CLASS = "text-blue-700 hover:underline break-all";
const DEFAULT_MENTION_CLASS =
  "inline rounded bg-blue-100 px-1 py-0 font-medium text-blue-800 align-baseline";
const INTERACTIVE_MENTION_CLASS = `${DEFAULT_MENTION_CLASS} cursor-pointer border-0 hover:bg-blue-200 focus:outline-none focus:ring-2 focus:ring-blue-400`;
const CURRENT_USER_MENTION_CLASS =
  "inline rounded bg-yellow-100 px-1 py-0 font-semibold text-yellow-900 align-baseline";
const INTERACTIVE_CURRENT_USER_MENTION_CLASS = `${CURRENT_USER_MENTION_CLASS} cursor-pointer border-0 hover:bg-yellow-200 focus:outline-none focus:ring-2 focus:ring-yellow-400`;
const PREVIEW_WIDTH = 256;
const PREVIEW_VERTICAL_GAP = 8;

export type MentionClickHandler = (user: MentionUser, event: any) => void;

export interface RichTextRenderOptions {
  text: string;
  mentions?: readonly MentionUser[];
  customEmojiById: (id: number) => CustomEmoji | null;
  currentUserId?: number | null;
  linkClass?: string;
  onMentionClick?: MentionClickHandler;
}

interface MarkerRange {
  type: "mention" | "custom-emoji";
  start: number;
  end: number;
  marker: string;
  id: number;
  storedName?: string;
}

interface MentionPreviewState {
  userId: number;
  anchor: HTMLElement;
}

function mentionUsersById(mentions: readonly MentionUser[]): Map<number, MentionUser> {
  const byId = new Map<number, MentionUser>();
  for (const mention of mentions) {
    if (!byId.has(mention.id)) byId.set(mention.id, mention);
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
): JSX.Element {
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

  return ranges.sort((a, b) => a.start - b.start || a.end - b.end);
}

function renderPlainTextSegment(
  text: string,
  mentionedUsers: ReadonlyMap<number, MentionUser>,
  options: RichTextRenderOptions,
): JSX.Element[] {
  const parts: JSX.Element[] = [];
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

function renderMention(user: MentionUser, options: RichTextRenderOptions): JSX.Element {
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
      onClick={(event) => options.onMentionClick?.(user, event.nativeEvent ?? event)}
    >
      {mentionLabel(user)}
    </button>
  );
}

function keyedRichTextPart(part: JSX.Element, key: string): JSX.Element {
  if (isValidElement(part)) return cloneElement(part, { key });
  return <Fragment key={key}>{part}</Fragment>;
}

export function renderRichText(options: RichTextRenderOptions): JSX.Element[] {
  const mentionedUsers = mentionUsersById(options.mentions ?? []);
  const linkClass = options.linkClass ?? DEFAULT_LINK_CLASS;
  const parts: JSX.Element[] = [];
  let partIndex = 0;
  const pushPart = (part: JSX.Element) => {
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
      for (const part of renderPlainTextSegment(token.value, mentionedUsers, options)) {
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
): JSX.Element[] {
  return renderRichText({ text, mentions, customEmojiById: byId });
}

function previewPosition(anchor: HTMLElement): JSX.CSSProperties {
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
  class?: string;
  className?: string;
  linkClass?: string;
  onMentionClick?: MentionClickHandler;
}) {
  const customEmojis = useOptionalCustomEmojis();
  const customEmojiById = (id: number) => customEmojis?.byId(id) ?? null;
  const previewId = useStableDomId();
  const [preview, setPreview] = useSignalState<MentionPreviewState | null>(null);
  const mentionedUsers = useComputedValue(() => mentionUsersById(props.mentions ?? []));
  const previewUser = useComputedValue(() => {
    const state = preview();
    if (!state) return null;
    return mentionedUsers().get(state.userId) ?? null;
  });
  let popoverRef: HTMLDivElement | null | undefined;

  const closePreview = () => setPreview(null);

  const handleMentionClick: MentionClickHandler = (user, event) => {
    event.stopPropagation();
    const anchor = (event.currentTarget ?? event.target) as HTMLElement | null;
    if (anchor) setPreview({ userId: user.id, anchor });
    props.onMentionClick?.(user, event.nativeEvent ?? event);
  };

  useAfterRenderEffect(() => {
    const state = preview();
    if (!state) return;
    if (!mentionedUsers().has(state.userId) || !textMentionsUser(props.text, state.userId)) {
      closePreview();
    }
  });

  useMountEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      const state = preview();
      if (!state) return;

      const target = event.target;
      if (!(target instanceof Node)) return;
      if (state.anchor.contains(target) || popoverRef?.contains(target)) return;
      closePreview();
    };

    const onKeyDown = (event: any) => {
      if (event.key === "Escape") closePreview();
    };

    const onScroll = () => closePreview();

    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("keydown", onKeyDown);
    window.addEventListener("scroll", onScroll, true);
    registerCleanup(() => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("scroll", onScroll, true);
    });
  });

  return (
    <div className={props.className ?? props.class ?? DEFAULT_TEXT_CLASS}>
      {renderRichText({
        text: props.text,
        mentions: props.mentions ?? [],
        customEmojiById,
        currentUserId: props.currentUserId,
        linkClass: props.linkClass,
        onMentionClick: handleMentionClick,
      })}
      <If when={preview()}>
        {(state) => (
          <If when={previewUser()}>
            {(user) => {
              const display = () => mentionDisplayName(user());
              return (
                <div
                  id={previewId}
                  ref={(el) => {
                    popoverRef = el;
                  }}
                  role="dialog"
                  aria-label={mentionPreviewLabel(user())}
                  className="fixed z-50 rounded-lg border border-gray-200 bg-white p-3 text-gray-900 shadow-lg"
                  style={previewPosition(state().anchor)}
                >
                  <div className="flex items-center gap-3">
                    <Avatar url={user().avatar_url} username={display()} size={48} />
                    <div className="min-w-0">
                      <div className="truncate text-sm font-semibold text-gray-950">
                        {display()}
                      </div>
                      <div className="truncate text-sm text-gray-600">@{user().username}</div>
                    </div>
                  </div>
                </div>
              );
            }}
          </If>
        )}
      </If>
    </div>
  );
}
