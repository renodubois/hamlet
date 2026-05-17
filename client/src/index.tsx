/* @refresh reload */
import "solid-devtools";
import "./index.css";

import { ErrorBoundary } from "solid-js";
import { render } from "solid-js/web";

import App from "./App";
import { Router } from "@solidjs/router";
import { routes } from "./routes";
import { AuthProvider } from "./contexts/auth";

const root = document.getElementById("root");

if (!(root instanceof HTMLElement)) {
  throw new Error(
    "Root element not found. Did you forget to add it to your index.html? Or maybe the id attribute got misspelled?",
  );
}

render(
  () => (
    <ErrorBoundary
      fallback={(err) => (
        <div class="p-8" role="alert">
          <h2 class="text-lg font-semibold text-red-700">Something went wrong</h2>
          <p class="mt-2 text-sm text-gray-700">
            {err instanceof Error ? err.message : String(err)}
          </p>
        </div>
      )}
    >
      <AuthProvider>
        <Router root={App}>{routes}</Router>
      </AuthProvider>
    </ErrorBoundary>
  ),
  root,
);
