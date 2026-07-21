import { StrictMode, type ReactElement } from "react";
import { render as rtlRender, type RenderOptions } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
/** Render a test tree under React Strict Mode. */
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
