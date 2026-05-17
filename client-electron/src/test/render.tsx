import type { JSX, Component } from "solid-js";
import { render } from "@solidjs/testing-library";
import { MemoryRouter, Route, createMemoryHistory } from "@solidjs/router";

export function renderWithRouter(ui: () => JSX.Element, initialPath = "/") {
  const history = createMemoryHistory();
  history.set({ value: initialPath });
  return render(() => (
    <MemoryRouter history={history}>
      <Route path="*" component={ui as Component} />
    </MemoryRouter>
  ));
}

export function assertExists<T>(value: T | null | undefined, label = "value"): T {
  if (value === null || value === undefined) {
    throw new Error(`expected ${label} to exist`);
  }
  return value;
}
