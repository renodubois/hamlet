import { Fragment, type HTMLAttributes, type ReactNode } from "react";
import { messageDisplayName, resolveServerUrl, type CustomEmoji } from "../api";
import { useOptionalCustomEmojis } from "../contexts/custom-emojis";
import { parseCustomEmojiMarkers, type CustomEmojiMarkerToken } from "../emoji/custom-emojis";

const MAX_PREVIEW_CHARS = 120;
const DEFAULT_ROOT_CLASS =
  "mb-1 flex min-w-0 items-center gap-1 border-l-2 border-primary/30 pl-2 text-sm text-muted-foreground";
const DEFAULT_AUTHOR_CLASS = "shrink-0 font-medium text-muted-foreground";
const DEFAULT_TEXT_CLASS = "min-w-0 truncate";

export interface MessageReferencePreviewSource {
  id: number;
  user_id?: number;
  channel_id?: number;
  created_at?: number;
  deleted_at?: number | null;
  text?: string;
  attachment_count?: number;
  username?: string | null;
  display_name?: string | null;
  avatar_url?: string | null;
  attachments?: readonly unknown[];
}

interface PreviewTextToken {
  kind: "text";
  value: string;
}

interface PreviewCustomEmojiToken {
  kind: "custom-emoji";
  marker: CustomEmojiMarkerToken;
  label: string;
  emoji: CustomEmoji | null;
}

type PreviewRenderToken = PreviewTextToken | PreviewCustomEmojiToken;

export interface MessageReferencePreviewProps {
  id?: string;
  reference?: MessageReferencePreviewSource | null;
  targetId?: number | null;
  className?: string;
  authorClass?: string;
  textClass?: string;
  authorPrefix?: string;
  ariaLabelPrefix?: string;
  role?: HTMLAttributes<HTMLDivElement>["role"];
  ariaLive?: "off" | "polite" | "assertive";
  onActivate?: () => void;
  children?: ReactNode;
}

function referenceAuthorName(reference: MessageReferencePreviewSource | null | undefined): string {
  if (!reference || !reference.username) return "Unknown message";
  return messageDisplayName({
    username: reference.username,
    display_name: reference.display_name ?? null,
  });
}

export function messageReferenceAttachmentPreviewText(count: number): string {
  return count === 1 ? "Attachment" : `${count} attachments`;
}

function attachmentCount(reference: MessageReferencePreviewSource): number {
  if (typeof reference.attachment_count === "number" && reference.attachment_count > 0) {
    return reference.attachment_count;
  }
  return reference.attachments?.length ?? 0;
}

function normalizedText(reference: MessageReferencePreviewSource): string {
  return (reference.text ?? "").replace(/\s+/g, " ").trim();
}

function customEmojiLabel(marker: CustomEmojiMarkerToken, emoji: CustomEmoji | null): string {
  return `:${emoji?.name ?? marker.storedName}:`;
}

function truncateTokens(tokens: PreviewRenderToken[]): PreviewRenderToken[] {
  const total = tokens.reduce((sum, token) => {
    return sum + (token.kind === "text" ? token.value.length : token.label.length);
  }, 0);
  if (total <= MAX_PREVIEW_CHARS) return tokens;

  const out: PreviewRenderToken[] = [];
  let remaining = MAX_PREVIEW_CHARS - 1;

  for (const token of tokens) {
    if (remaining <= 0) break;

    const length = token.kind === "text" ? token.value.length : token.label.length;
    if (length <= remaining) {
      out.push(token);
      remaining -= length;
      continue;
    }

    if (token.kind === "text" && remaining > 0) {
      out.push({ kind: "text", value: token.value.slice(0, remaining) });
    }
    break;
  }

  out.push({ kind: "text", value: "…" });
  return out;
}

function textTokens(text: string, byId: (id: number) => CustomEmoji | null): PreviewRenderToken[] {
  const tokens: PreviewRenderToken[] = parseCustomEmojiMarkers(text).map((token) => {
    if (token.type === "text") return { kind: "text", value: token.value };
    const emoji = byId(token.id);
    return {
      kind: "custom-emoji",
      marker: token,
      label: customEmojiLabel(token, emoji),
      emoji,
    };
  });
  return truncateTokens(tokens);
}

function plainTextFromTokens(tokens: readonly PreviewRenderToken[]): string {
  return tokens.map((token) => (token.kind === "text" ? token.value : token.label)).join("");
}

export function messageReferencePreviewText(
  reference: MessageReferencePreviewSource | null | undefined,
): string {
  if (!reference) return "Original message unavailable";
  if (reference.deleted_at != null) return "Original message deleted";

  const text = normalizedText(reference);
  if (text.length > 0) {
    const tokens = textTokens(text, () => null);
    return plainTextFromTokens(tokens);
  }

  const count = attachmentCount(reference);
  if (count > 0) return messageReferenceAttachmentPreviewText(count);
  return "No text";
}

export function messageReferencePreviewAriaLabel(
  reference: MessageReferencePreviewSource | null | undefined,
  targetId?: number | null,
): string {
  if (!reference) {
    return targetId == null
      ? "Replying to unavailable message"
      : `Replying to unavailable message ${targetId}`;
  }

  const author = referenceAuthorName(reference);
  if (reference.deleted_at != null) return `Replying to deleted message by ${author}`;
  return `Replying to ${author}: ${messageReferencePreviewText(reference)}`;
}

function renderCustomEmojiToken(token: PreviewCustomEmojiToken): ReactNode {
  if (!token.emoji) {
    return <span title={`Custom emoji ${token.marker.marker} is unavailable`}>{token.label}</span>;
  }

  return (
    <img
      src={resolveServerUrl(token.emoji.image_url)}
      alt={token.label}
      title={token.emoji.deleted_at === null ? token.label : `${token.label} (deleted)`}
      className="inline-block h-4 w-4 align-text-bottom object-contain"
    />
  );
}

function keyedTokens(tokens: readonly PreviewRenderToken[]) {
  const occurrences = new Map<string, number>();
  return tokens.map((token) => {
    const base = token.kind === "text" ? `text:${token.value}` : `custom:${token.marker.marker}`;
    const occurrence = occurrences.get(base) ?? 0;
    occurrences.set(base, occurrence + 1);
    return { token, key: occurrence === 0 ? base : `${base}:${occurrence}` };
  });
}

function PreviewText(props: { reference: MessageReferencePreviewSource }) {
  const customEmojis = useOptionalCustomEmojis();
  const customEmojiById = (id: number) => customEmojis?.byId(id) ?? null;
  const text = normalizedText(props.reference);

  if (props.reference.deleted_at != null || text.length === 0) {
    return messageReferencePreviewText(props.reference);
  }

  return keyedTokens(textTokens(text, customEmojiById)).map(({ token, key }) => (
    <Fragment key={key}>
      {token.kind === "text" ? token.value : renderCustomEmojiToken(token)}
    </Fragment>
  ));
}

export default function MessageReferencePreview(props: MessageReferencePreviewProps) {
  const baseClass = props.className ?? DEFAULT_ROOT_CLASS;
  const rootClass = props.onActivate
    ? `${baseClass} cursor-pointer rounded-sm text-left transition-colors hover:bg-primary/10 focus:outline-none focus-visible:ring-2 focus-visible:ring-ring`
    : baseClass;
  const authorClass = props.authorClass ?? DEFAULT_AUTHOR_CLASS;
  const textClass = props.textClass ?? DEFAULT_TEXT_CLASS;
  const ariaLabel = `${props.ariaLabelPrefix ?? ""}${messageReferencePreviewAriaLabel(
    props.reference,
    props.targetId,
  )}`;
  const content = (
    <>
      <span className={authorClass}>{`${props.authorPrefix ?? ""}${referenceAuthorName(
        props.reference,
      )}`}</span>
      <span aria-hidden="true" className="shrink-0 text-muted-foreground/70">
        —
      </span>
      <span className={textClass}>
        {props.reference ? (
          <PreviewText reference={props.reference} />
        ) : (
          "Original message unavailable"
        )}
      </span>
      {props.children}
    </>
  );

  return props.onActivate ? (
    <button
      id={props.id}
      type="button"
      className={rootClass}
      aria-live={props.ariaLive}
      aria-label={ariaLabel}
      onClick={props.onActivate}
    >
      {content}
    </button>
  ) : (
    <div
      id={props.id}
      className={rootClass}
      role={props.role}
      aria-live={props.ariaLive}
      aria-label={ariaLabel}
    >
      {content}
    </div>
  );
}
