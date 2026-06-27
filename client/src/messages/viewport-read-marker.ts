export const DEFAULT_NEAR_BOTTOM_PX = 96;

export interface ViewportReadMarkerState {
  nearBottom: boolean;
  rendererEligible: boolean;
  newMessageBelowViewport: boolean;
  lastVisibleTopLevelMessageId: number | null;
}

export function isNearScrollBottom(
  container: Pick<HTMLElement, "scrollHeight" | "scrollTop" | "clientHeight">,
  thresholdPx = DEFAULT_NEAR_BOTTOM_PX,
): boolean {
  return container.scrollHeight - container.scrollTop - container.clientHeight <= thresholdPx;
}

export function rendererCanMarkRead(doc: Document = document): boolean {
  return doc.visibilityState !== "hidden" && doc.hasFocus();
}

export function lastVisibleTopLevelMessageId(container: HTMLElement): number | null {
  const containerRect = container.getBoundingClientRect();
  let lastVisible: number | null = null;

  for (const row of Array.from(container.querySelectorAll<HTMLElement>("[data-message-id]"))) {
    const rawId = row.dataset.messageId;
    const id = rawId ? Number(rawId) : NaN;
    if (!Number.isSafeInteger(id) || id <= 0) continue;

    const rowRect = row.getBoundingClientRect();
    const intersectsViewport =
      rowRect.bottom >= containerRect.top && rowRect.top <= containerRect.bottom;
    if (intersectsViewport) lastVisible = id;
  }

  return lastVisible;
}

export function hasTopLevelMessageBelowViewport(container: HTMLElement): boolean {
  const containerRect = container.getBoundingClientRect();
  return Array.from(container.querySelectorAll<HTMLElement>("[data-message-id]")).some(
    (row) => row.getBoundingClientRect().top > containerRect.bottom,
  );
}

export function getViewportReadMarkerState(
  container: HTMLElement,
  doc: Document = document,
): ViewportReadMarkerState {
  const nearBottom = isNearScrollBottom(container);
  return {
    nearBottom,
    rendererEligible: rendererCanMarkRead(doc),
    newMessageBelowViewport: !nearBottom && hasTopLevelMessageBelowViewport(container),
    lastVisibleTopLevelMessageId: lastVisibleTopLevelMessageId(container),
  };
}
