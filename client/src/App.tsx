import React, { Suspense, useEffect, type ReactNode } from "react";
import { Navigate, Outlet, useLocation, useNavigate } from "react-router-dom";
import ChannelSidebar from "./components/channel-sidebar";
import { Button } from "./components/ui/button";
import { useAuth } from "./contexts/auth";
import { ChannelsProvider, useChannels } from "./contexts/channels";
import { CustomEmojisProvider } from "./contexts/custom-emojis";
import { EventsProvider } from "./contexts/events";
import { ReadStatesProvider } from "./contexts/read-states";
import { type User } from "./api";
import { VoiceChatProvider } from "./contexts/voice-chat";

function ErrorPanel(props: { error: unknown; reset?: () => void; title?: string }) {
  const message = props.error instanceof Error ? props.error.message : String(props.error);
  return (
    <div className="max-w-lg p-8" role="alert">
      <h2 className="text-lg font-semibold text-destructive">
        {props.title ?? "Something went wrong"}
      </h2>
      <p className="mt-2 text-sm text-muted-foreground">{message}</p>
      {props.reset ? (
        <Button type="button" className="mt-4" onClick={() => props.reset?.()}>
          Try again
        </Button>
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
    return (
      <div key={this.state.key} className="flex h-full min-h-0 flex-1 flex-col">
        {this.props.children}
      </div>
    );
  }
}

function AppShell(props: { user: User }) {
  const auth = useAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const { channels } = useChannels();

  useEffect(() => {
    if (location.pathname !== "/") return;
    const first = channels.find((channel) => channel.type === "text");
    if (first) void navigate(`/channel/${first.id}`, { replace: true });
  }, [channels, location.pathname, navigate]);

  return (
    <div className="flex h-screen min-h-0 overflow-hidden">
      <aside className="flex min-h-0 w-60 flex-shrink-0 flex-col border-r border-sidebar-border bg-sidebar text-sidebar-foreground">
        <ChannelSidebar user={props.user} onLogout={auth.logout} onAvatarChange={auth.refresh} />
      </aside>
      <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
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
  const location = useLocation();

  if (auth.status === "loading") return null;
  if (auth.status === "anonymous") {
    return location.pathname === "/login" ? (
      <Suspense fallback={null}>
        <Outlet />
      </Suspense>
    ) : (
      <Navigate to="/login" replace />
    );
  }

  const currentUser = auth.user;
  if (!currentUser) return null;
  if (location.pathname === "/login") return <Navigate to="/" replace />;

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
