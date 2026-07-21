import { useId } from "react";
import { useSignalState } from "../hooks/react-state";
import { getServerUrl, type ReactionSummary } from "../api";

function resolveImageUrl(url: string): string {
  if (url.startsWith("http://") || url.startsWith("https://")) return url;
  return `${getServerUrl()}${url}`;
}

function isDeletedCustomReaction(reaction: ReactionSummary): boolean {
  return reaction.kind === "custom" && reaction.deleted_at != null;
}

function reactionDisplayLabel(reaction: ReactionSummary): string {
  const label = reaction.kind === "native" ? reaction.emoji : `:${reaction.name}:`;
  return isDeletedCustomReaction(reaction) ? `${label} (deleted)` : label;
}

function reactionCountLabel(count: number): string {
  return count === 1 ? "1 reaction" : `${count} reactions`;
}

function reactionLabel(reaction: ReactionSummary): string {
  const action = reaction.me_reacted
    ? "Remove your reaction"
    : isDeletedCustomReaction(reaction)
      ? "No longer available"
      : "Add your reaction";
  const animatedPrefix = reaction.kind === "custom" && reaction.animated ? "Animated " : "";
  return `${animatedPrefix}${reactionDisplayLabel(reaction)} ${reactionCountLabel(
    reaction.count,
  )}. ${action}`;
}

function reactionPreviewText(reaction: ReactionSummary): string {
  const countLabel = reactionCountLabel(reaction.count);
  const reactors = reaction.reactors ?? [];
  if (reactors.length === 0) return countLabel;

  const remaining = Math.max(0, reaction.count - reactors.length);
  const names =
    remaining > 0 ? `${reactors.join(", ")} and ${remaining} more` : reactors.join(", ");
  return `${countLabel}: ${names}`;
}

function reactionKey(reaction: ReactionSummary): string {
  return reaction.kind === "native" ? `native:${reaction.emoji}` : `custom:${reaction.emoji_id}`;
}

export default function ReactionRow(props: {
  reactions: readonly ReactionSummary[];
  onToggle: (reaction: ReactionSummary) => void;
}) {
  const rowId = useId();
  const [activePreviewKey, setActivePreviewKey] = useSignalState<string | null>(null);
  const visibleReactions = props.reactions.filter((reaction) => reaction.count > 0);

  return visibleReactions.length > 0 ? (
    <div className="mt-1 flex flex-wrap gap-1" aria-label="Message reactions">
      {visibleReactions.map((reaction) => {
        const key = reactionKey(reaction);
        const previewId = `${rowId}-reaction-preview-${encodeURIComponent(key)}`;
        const previewText = reactionPreviewText(reaction);
        const canToggle = !isDeletedCustomReaction(reaction) || reaction.me_reacted;

        return (
          <span key={key} className="relative inline-flex">
            <button
              type="button"
              aria-pressed={reaction.me_reacted}
              aria-label={reactionLabel(reaction)}
              aria-describedby={previewId}
              title={previewText}
              disabled={!canToggle}
              className={`inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-sm transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-ring ${
                reaction.me_reacted
                  ? "border-primary/30 bg-primary/10 font-medium text-primary shadow-sm"
                  : canToggle
                    ? "border-border bg-muted text-muted-foreground hover:border-primary/20 hover:bg-accent"
                    : "cursor-not-allowed border-border bg-muted/50 text-muted-foreground"
              }`}
              onClick={() => {
                if (canToggle) props.onToggle(reaction);
              }}
              onFocus={() => setActivePreviewKey(key)}
              onBlur={() => setActivePreviewKey(null)}
              onMouseEnter={() => setActivePreviewKey(key)}
              onMouseLeave={(event) => {
                if (document.activeElement !== event.currentTarget) setActivePreviewKey(null);
              }}
            >
              {reaction.me_reacted ? (
                <span aria-hidden="true" className="text-xs leading-none">
                  ✓
                </span>
              ) : null}
              {reaction.kind === "custom" ? (
                <>
                  <img
                    src={resolveImageUrl(reaction.image_url)}
                    alt=""
                    title={reactionDisplayLabel(reaction)}
                    className="h-5 w-5 object-contain"
                    aria-hidden="true"
                  />
                  <span className="sr-only">{reactionDisplayLabel(reaction)}</span>
                  {reaction.animated ? (
                    <>
                      <span
                        className="rounded bg-purple-700 px-0.5 text-[8px] font-bold uppercase leading-3 text-white"
                        title="Animated custom emoji"
                        aria-hidden="true"
                      >
                        A
                      </span>
                      <span className="sr-only">Animated custom emoji</span>
                    </>
                  ) : null}
                </>
              ) : (
                <span aria-hidden="true">{reactionDisplayLabel(reaction)}</span>
              )}
              <span>{reaction.count}</span>
            </button>
            <span id={previewId} className="sr-only">
              {previewText}
            </span>
            {activePreviewKey() === key ? (
              <span
                role="tooltip"
                className="absolute bottom-full left-1/2 z-20 mb-1 w-max max-w-xs -translate-x-1/2 rounded-md bg-foreground px-2 py-1 text-xs font-medium text-background shadow-lg"
              >
                {previewText}
              </span>
            ) : null}
          </span>
        );
      })}
    </div>
  ) : null;
}
