import { describe, expect, test, vi } from "vitest";
import {
  getViewportReadMarkerState,
  isNearScrollBottom,
  lastVisibleTopLevelMessageId,
  rendererCanMarkRead,
} from "./viewport-read-marker";

function setRect(element: Element, rect: Partial<DOMRect>) {
  const full = {
    top: 0,
    bottom: 0,
    left: 0,
    right: 0,
    width: 0,
    height: 0,
    x: 0,
    y: 0,
    toJSON: () => ({}),
    ...rect,
  } as DOMRect;
  vi.spyOn(element, "getBoundingClientRect").mockReturnValue(full);
}

describe("viewport read marker helpers", () => {
  test("detects near-bottom state with a threshold", () => {
    expect(isNearScrollBottom({ scrollHeight: 1000, scrollTop: 850, clientHeight: 100 })).toBe(
      true,
    );
    expect(isNearScrollBottom({ scrollHeight: 1000, scrollTop: 700, clientHeight: 100 })).toBe(
      false,
    );
  });

  test("requires focused visible renderer eligibility", () => {
    const doc = { visibilityState: "visible", hasFocus: () => true } as Document;
    expect(rendererCanMarkRead(doc)).toBe(true);

    const hidden = { visibilityState: "hidden", hasFocus: () => true } as Document;
    expect(rendererCanMarkRead(hidden)).toBe(false);

    const blurred = { visibilityState: "visible", hasFocus: () => false } as Document;
    expect(rendererCanMarkRead(blurred)).toBe(false);
  });

  test("returns the last visible top-level message id", () => {
    const container = document.createElement("div");
    setRect(container, { top: 0, bottom: 100 });
    for (const [id, top, bottom] of [
      [10, -50, -10],
      [20, 10, 30],
      [30, 70, 90],
      [40, 120, 150],
    ]) {
      const row = document.createElement("div");
      row.dataset.messageId = String(id);
      setRect(row, { top, bottom });
      container.append(row);
    }

    expect(lastVisibleTopLevelMessageId(container)).toBe(30);
  });

  test("reports new messages below viewport when scrolled up", () => {
    const container = document.createElement("div");
    Object.defineProperties(container, {
      scrollHeight: { value: 1000, configurable: true },
      scrollTop: { value: 100, configurable: true },
      clientHeight: { value: 100, configurable: true },
    });
    setRect(container, { top: 0, bottom: 100 });
    const visible = document.createElement("div");
    visible.dataset.messageId = "1";
    setRect(visible, { top: 20, bottom: 40 });
    const below = document.createElement("div");
    below.dataset.messageId = "2";
    setRect(below, { top: 120, bottom: 140 });
    container.append(visible, below);

    const state = getViewportReadMarkerState(container, {
      visibilityState: "visible",
      hasFocus: () => true,
    } as Document);

    expect(state.nearBottom).toBe(false);
    expect(state.rendererEligible).toBe(true);
    expect(state.newMessageBelowViewport).toBe(true);
    expect(state.lastVisibleTopLevelMessageId).toBe(1);
  });
});
