import type { ReactElement } from "react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { render } from "./testing-library";

export function renderWithRouter(ui: () => ReactElement, initialPath = "/") {
  return render(
    <MemoryRouter initialEntries={[initialPath]}>
      <Routes>
        <Route path="*" element={ui()} />
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
