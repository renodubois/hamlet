import { useState } from "react";
import { act, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import type { ReactionSummary } from "../api";
import { renderNative } from "../test/render";
import ReactionRow from "./reaction-row";

const heart: ReactionSummary = {
  kind: "native",
  emoji: "❤️",
  count: 2,
  me_reacted: false,
  reactors: ["Alice", "Bob"],
};
const fire: ReactionSummary = {
  kind: "native",
  emoji: "🔥",
  count: 1,
  me_reacted: false,
  reactors: ["Casey"],
};

describe("<ReactionRow>", () => {
  test("keeps focus and tooltip state with a reaction across reorder and removal", () => {
    const onToggle = vi.fn();
    let setReactions: (reactions: readonly ReactionSummary[]) => void = () => undefined;

    function Harness() {
      const [reactions, setReactionState] = useState<readonly ReactionSummary[]>([fire, heart]);
      setReactions = setReactionState;
      return <ReactionRow reactions={reactions} onToggle={onToggle} />;
    }

    renderNative(<Harness />);

    const heartButton = screen.getByRole("button", { name: /^❤️/ });
    act(() => heartButton.focus());
    expect(heartButton).toHaveFocus();
    expect(screen.getByRole("tooltip")).toHaveTextContent("2 reactions: Alice, Bob");

    act(() => setReactions([heart, fire]));

    expect(screen.getByRole("button", { name: /^❤️/ })).toBe(heartButton);
    expect(heartButton).toHaveFocus();
    expect(screen.getByRole("tooltip")).toHaveTextContent("2 reactions: Alice, Bob");

    act(() => setReactions([heart]));

    expect(screen.getByRole("button", { name: /^❤️/ })).toBe(heartButton);
    expect(heartButton).toHaveFocus();
    expect(screen.getByRole("tooltip")).toHaveTextContent("2 reactions: Alice, Bob");

    act(() => setReactions([fire]));

    expect(screen.queryByRole("tooltip")).toBeNull();
  });
});
