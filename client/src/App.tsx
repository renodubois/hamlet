import React, { Suspense, type ReactNode } from "react";
import { Outlet, useLocation, useNavigate } from "react-router-dom";
import ChannelSidebar from "./components/channel-sidebar";
import { useAuth } from "./contexts/auth";
import { ChannelsProvider, useChannels } from "./contexts/channels";
import { CustomEmojisProvider } from "./contexts/custom-emojis";
import { EventsProvider } from "./contexts/events";
import { ReadStatesProvider } from "./contexts/read-states";
import { type User } from "./api";
import { VoiceChatProvider } from "./contexts/voice-chat";
import { useAfterRenderEffect } from "./hooks/react-state";

function ErrorPanel(props: { error: unknown; reset?: () => void; title?: string }) {
  const message = props.error instanceof Error ? props.error.message : String(props.error);
  return (
    <div className="max-w-lg p-8" role="alert">
      <h2 className="text-lg font-semibold text-red-700">
        {props.title ?? "Something went wrong"}
      </h2>
      <p className="mt-2 text-sm text-gray-700">{message}</p>
      {props.reset ? (
        <button
          type="button"
          className="mt-4 rounded-md bg-blue-600 px-4 py-2 text-sm text-white hover:bg-blue-700"
          onClick={() => props.reset?.()}
        >
          Try again
        </button>
      ) : null}
    </div>
  );
}

class RouteErrorBoundary extends React.Component<
  { children: ReactNode },
  { error: unknown; key: number }
> {
  state = { error: null as unknown, key: 0 };
  static getDerivedStateFromError(error: unknown) {
    return { error };
  }
  reset = () => this.setState((state) => ({ error: null, key: state.key + 1 }));
  render() {
    if (this.state.error) return <ErrorPanel error={this.state.error} reset={this.reset} />;
    return <div key={this.state.key}>{this.props.children}</div>;
  }
}

function AppShell(props: { user: User }) {
  const auth = useAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const { channels } = useChannels();

  useAfterRenderEffect(() => {
    const ch = channels();
    if (location.pathname === "/" && ch && ch.length > 0) {
      const first = ch.find((c) => c.type === "text");
      if (first) void navigate(`/channel/${first.id}`, { replace: true });
    }
  });

  return (
    <div className="flex h-screen">
      <aside className="flex w-60 flex-shrink-0 flex-col bg-gray-800 text-gray-100">
        <ChannelSidebar user={props.user} onLogout={auth.logout} onAvatarChange={auth.refresh} />
      </aside>
      <main className="flex min-w-0 flex-1 flex-col">
        <RouteErrorBoundary>
          <Suspense fallback={null}>
            <Outlet />
          </Suspense>
        </RouteErrorBoundary>
      </main>
    </div>
  );
}

export default function App() {
  const auth = useAuth();
  const navigate = useNavigate();
  const location = useLocation();

  useAfterRenderEffect(() => {
    const u = auth.user();
    if (u === undefined) return;
    const onLogin = location.pathname === "/login";
    if (!u && !onLogin) void navigate("/login", { replace: true });
    else if (u && onLogin) void navigate("/", { replace: true });
  });

  const currentUser = auth.user();
  if (currentUser === undefined) return null;
  if (!currentUser) {
    return location.pathname === "/login" ? (
      <Suspense fallback={null}>
        <Outlet />
      </Suspense>
    ) : null;
  }

  return (
    <EventsProvider>
      <CustomEmojisProvider>
        <ChannelsProvider>
          <ReadStatesProvider>
            <VoiceChatProvider>
              <AppShell user={currentUser} />
            </VoiceChatProvider>
          </ReadStatesProvider>
        </ChannelsProvider>
      </CustomEmojisProvider>
    </EventsProvider>
  );
}
