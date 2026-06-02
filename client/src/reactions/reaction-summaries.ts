import type { ReactionRequest, ReactionSummary } from "../api";

export type ReactionOperation = "add" | "remove" | "toggle";

function sameReaction(summary: ReactionSummary, reaction: ReactionRequest): boolean {
  if (summary.kind === "native" && reaction.kind === "native") {
    return summary.emoji === reaction.emoji;
  }
  if (summary.kind === "custom" && reaction.kind === "custom") {
    return summary.emoji_id === reaction.emoji_id;
  }
  return false;
}

const MAX_REACTOR_PREVIEW_NAMES = 5;

function withYouFirst(reactors: readonly string[] | undefined): string[] {
  const withoutYou = (reactors ?? []).filter((name) => name !== "You");
  return ["You", ...withoutYou].slice(0, MAX_REACTOR_PREVIEW_NAMES);
}

function withoutYou(reactors: readonly string[] | undefined): string[] | undefined {
  if (reactors === undefined) return undefined;
  return reactors.filter((name) => name !== "You").slice(0, MAX_REACTOR_PREVIEW_NAMES);
}

function newReactionSummary(reaction: ReactionRequest): ReactionSummary {
  if (reaction.kind === "native") {
    return { kind: "native", emoji: reaction.emoji, count: 1, me_reacted: true, reactors: ["You"] };
  }

  return {
    kind: "custom",
    emoji_id: reaction.emoji_id,
    name: reaction.name ?? "custom emoji",
    image_url: reaction.image_url ?? "",
    animated: reaction.animated ?? false,
    deleted_at: null,
    count: 1,
    me_reacted: true,
    reactors: ["You"],
  };
}

function reactorsEqual(
  a: readonly string[] | undefined,
  b: readonly string[] | undefined,
): boolean {
  const left = a ?? [];
  const right = b ?? [];
  return left.length === right.length && left.every((name, index) => name === right[index]);
}

export function reactionSummariesEqual(
  a: readonly ReactionSummary[],
  b: readonly ReactionSummary[],
): boolean {
  return (
    a.length === b.length &&
    a.every((summary, index) => {
      const other = b[index];
      if (other === undefined || summary.kind !== other.kind) return false;
      if (summary.count !== other.count || summary.me_reacted !== other.me_reacted) return false;
      if (!reactorsEqual(summary.reactors, other.reactors)) return false;
      if (summary.kind === "native" && other.kind === "native") {
        return summary.emoji === other.emoji;
      }
      if (summary.kind === "custom" && other.kind === "custom") {
        return (
          summary.emoji_id === other.emoji_id &&
          summary.name === other.name &&
          summary.image_url === other.image_url &&
          summary.animated === other.animated &&
          (summary.deleted_at ?? null) === (other.deleted_at ?? null)
        );
      }
      return false;
    })
  );
}

export function mergeReactionUpdateForViewer(
  current: readonly ReactionSummary[],
  incoming: readonly ReactionSummary[],
  eventUserId: number,
  currentUserId: number | null,
): ReactionSummary[] {
  if (currentUserId !== null && eventUserId === currentUserId) {
    return incoming.map((summary) => ({ ...summary }));
  }

  return incoming.map((summary) => {
    const previous = current.find((candidate) => sameReaction(candidate, summary));
    return { ...summary, me_reacted: previous?.me_reacted ?? false };
  });
}

export function applyOptimisticReaction(
  summaries: readonly ReactionSummary[],
  reaction: ReactionRequest,
  operation: ReactionOperation,
): ReactionSummary[] {
  const existing = summaries.find((summary) => sameReaction(summary, reaction));
  const shouldAdd = operation === "add" || (operation === "toggle" && !existing?.me_reacted);

  if (shouldAdd) {
    if (!existing) {
      return [...summaries, newReactionSummary(reaction)];
    }
    if (existing.me_reacted) return summaries.map((summary) => ({ ...summary }));
    return summaries.map((summary) =>
      sameReaction(summary, reaction)
        ? {
            ...summary,
            count: summary.count + 1,
            me_reacted: true,
            reactors: withYouFirst(summary.reactors),
          }
        : { ...summary },
    );
  }

  if (!existing || !existing.me_reacted) return summaries.map((summary) => ({ ...summary }));

  return summaries
    .map((summary) =>
      sameReaction(summary, reaction)
        ? {
            ...summary,
            count: Math.max(0, summary.count - 1),
            me_reacted: false,
            reactors: withoutYou(summary.reactors),
          }
        : { ...summary },
    )
    .filter((summary) => summary.count > 0);
}
