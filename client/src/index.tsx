/* @refresh reload */
import "solid-devtools";
import "./index.css";
import * as Sentry from "@sentry/solid";

import { ErrorBoundary } from "solid-js";
import { render } from "solid-js/web";

import App from "./App";
import { Router } from "@solidjs/router";
import { routes } from "./routes";
import { AuthProvider } from "./contexts/auth";

Sentry.init({
  dsn: "https://60c6e628bcadb37f23e2356423103d85@o4511678358880256.ingest.us.sentry.io/4511678668734464",
  dataCollection: {
    // To disable sending user data and HTTP bodies, uncomment the lines below. For more info visit:
    // https://docs.sentry.io/platforms/javascript/guides/solid/configuration/options/#dataCollection
    userInfo: false,
    // httpBodies: []
  },
});

const SentryErrorBoundary = Sentry.withSentryErrorBoundary(ErrorBoundary);

const root = document.getElementById("root");

if (!(root instanceof HTMLElement)) {
  throw new Error(
    "Root element not found. Did you forget to add it to your index.html? Or maybe the id attribute got misspelled?",
  );
}

render(
  () => (
    <SentryErrorBoundary
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
    </SentryErrorBoundary>
  ),
  root,
);
