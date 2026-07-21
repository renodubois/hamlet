import { StrictMode, type ReactElement } from "react";
import { render as rtlRender, type RenderOptions } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { render } from "./testing-library";

/** Legacy static-signal-aware router helper. Do not add new consumers. */
export function renderWithRouter(ui: () => ReactElement, initialPath = "/") {
  return render(
    <MemoryRouter initialEntries={[initialPath]}>
      <Routes>
        <Route path="*" element={ui()} />
      </Routes>
    </MemoryRouter>,
  );
}

/** Native React render path for migrated tests; intentionally has no signal bridge. */
export function renderNative(ui: ReactElement, options?: RenderOptions) {
  return rtlRender(<StrictMode>{ui}</StrictMode>, options);
}

export function renderWithRouterNative(ui: ReactElement, initialPath = "/") {
  return renderNative(
    <MemoryRouter initialEntries={[initialPath]}>
      <Routes>
        <Route path="*" element={ui} />
      </Routes>
    </MemoryRouter>,
  );
}

export function assertExists<T>(value: T | null | undefined, label = "value"): T {
  if (value === null || value === undefined) {
    throw new Error(`expected ${label} to exist`);
  }
  return value;
}
