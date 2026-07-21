import "./index.css";
import * as Sentry from "@sentry/react";

import { StrictMode, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter, Route, Routes } from "react-router-dom";

import App from "./App";
import { AuthProvider } from "./contexts/auth";
import NotFound from "./errors/404";
import ChannelView from "./pages/channel";
import LoginScreen from "./pages/login";
import ThreadsPage from "./pages/threads";
import { normalizeRouterBasename } from "./router-basename";
import { initializeRendererSentry } from "./sentry";

function ErrorFallback({ error, resetError }: { error: unknown; resetError?: () => void }) {
  return (
    <div className="p-8" role="alert">
      <h2 className="text-lg font-semibold text-destructive">Something went wrong</h2>
      <p className="mt-2 text-sm text-muted-foreground">
        {error instanceof Error ? error.message : String(error)}
      </p>
      {resetError ? (
        <button
          type="button"
          className="mt-4 rounded-md bg-primary px-4 py-2 text-sm text-primary-foreground transition-colors hover:bg-primary/90"
          onClick={resetError}
        >
          Try again
        </button>
      ) : null}
    </div>
  );
}

class RendererErrorBoundary extends Sentry.ErrorBoundary {
  override render(): ReactNode {
    return super.render();
  }
}

const sentryEnabled = initializeRendererSentry();
const root = document.getElementById("root");

if (!(root instanceof HTMLElement)) {
  throw new Error(
    "Root element not found. Did you forget to add it to your index.html? Or maybe the id attribute got misspelled?",
  );
}

const app = (
  <StrictMode>
    <AuthProvider>
      <BrowserRouter basename={normalizeRouterBasename(import.meta.env.BASE_URL)}>
        <Routes>
          <Route element={<App />}>
            <Route index element={null} />
            <Route path="login" element={<LoginScreen />} />
            <Route path="channel/:id" element={<ChannelView />} />
            <Route path="threads" element={<ThreadsPage />} />
            <Route path="*" element={<NotFound />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </AuthProvider>
  </StrictMode>
);

createRoot(root).render(
  sentryEnabled ? (
    <RendererErrorBoundary
      fallback={(fallbackProps) => (
        <ErrorFallback
          error={fallbackProps.error}
          resetError={fallbackProps.resetError ? () => fallbackProps.resetError() : undefined}
        />
      )}
    >
      {app}
    </RendererErrorBoundary>
  ) : (
    app
  ),
);
