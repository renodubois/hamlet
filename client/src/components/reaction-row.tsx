import { List, If, useSignalState, useStableDomId } from "../hooks/react-state";
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
  const rowId = useStableDomId();
  const [activePreviewKey, setActivePreviewKey] = useSignalState<string | null>(null);
  const visibleReactions = () => props.reactions.filter((reaction) => reaction.count > 0);

  return (
    <If when={visibleReactions().length > 0}>
      <div className="mt-1 flex flex-wrap gap-1" aria-label="Message reactions">
        <List each={visibleReactions()}>
          {(reaction, index) => {
            const key = () => reactionKey(reaction);
            const previewId = () => `${rowId}-reaction-preview-${index()}`;
            const previewText = () => reactionPreviewText(reaction);
            const canToggle = () => !isDeletedCustomReaction(reaction) || reaction.me_reacted;

            return (
              <span className="relative inline-flex">
                <button
                  type="button"
                  aria-pressed={reaction.me_reacted}
                  aria-label={reactionLabel(reaction)}
                  aria-describedby={previewId()}
                  title={previewText()}
                  disabled={!canToggle()}
                  className={`inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-400 ${
                    reaction.me_reacted
                      ? "border-blue-300 bg-blue-50 font-medium text-blue-700 shadow-sm"
                      : canToggle()
                        ? "border-gray-200 bg-gray-100 text-gray-700 hover:border-blue-200 hover:bg-gray-200"
                        : "cursor-not-allowed border-gray-200 bg-gray-50 text-gray-500"
                  }`}
                  onClick={() => {
                    if (canToggle()) props.onToggle(reaction);
                  }}
                  onFocus={() => setActivePreviewKey(key())}
                  onBlur={() => setActivePreviewKey(null)}
                  onMouseEnter={() => setActivePreviewKey(key())}
                  onMouseLeave={(event) => {
                    if (document.activeElement !== event.currentTarget) setActivePreviewKey(null);
                  }}
                >
                  <If when={reaction.me_reacted}>
                    <span aria-hidden="true" className="text-xs leading-none">
                      ✓
                    </span>
                  </If>
                  <If
                    when={reaction.kind === "custom" ? reaction : null}
                    fallback={<span aria-hidden="true">{reactionDisplayLabel(reaction)}</span>}
                  >
                    {(customReaction) => (
                      <>
                        <img
                          src={resolveImageUrl(customReaction().image_url)}
                          alt=""
                          title={reactionDisplayLabel(customReaction())}
                          className="h-5 w-5 object-contain"
                          aria-hidden="true"
                        />
                        <span className="sr-only">{reactionDisplayLabel(customReaction())}</span>
                        <If when={customReaction().animated}>
                          <span
                            className="rounded bg-purple-700 px-0.5 text-[8px] font-bold uppercase leading-3 text-white"
                            title="Animated custom emoji"
                            aria-hidden="true"
                          >
                            A
                          </span>
                          <span className="sr-only">Animated custom emoji</span>
                        </If>
                      </>
                    )}
                  </If>
                  <span>{reaction.count}</span>
                </button>
                <span id={previewId()} className="sr-only">
                  {previewText()}
                </span>
                <If when={activePreviewKey() === key()}>
                  <span
                    role="tooltip"
                    className="absolute bottom-full left-1/2 z-20 mb-1 w-max max-w-xs -translate-x-1/2 rounded-md bg-gray-900 px-2 py-1 text-xs font-medium text-white shadow-lg"
                  >
                    {previewText()}
                  </span>
                </If>
              </span>
            );
          }}
        </List>
      </div>
    </If>
  );
}
