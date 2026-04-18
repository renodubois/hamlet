import { createEffect, type JSX, Suspense, type Component, Show } from "solid-js";
import { useLocation, useNavigate } from "@solidjs/router";
import ChannelSidebar from "./components/channel_sidebar";
import { useAuth } from "./auth_context";
import { ChannelsProvider, useChannels } from "./channels_context";
import { EventsProvider } from "./events_context";
import LoginScreen from "./pages/login";
import { type User } from "./api";

const AppShell: Component<{ children: JSX.Element; user: User }> = (props) => {
  const auth = useAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const { channels } = useChannels();

  createEffect(() => {
    const ch = channels();
    if (location.pathname === "/" && ch && ch.length > 0) {
      // Voice channels don't have a message view — pick the first text channel.
      const first = ch.find((c) => c.type === "text");
      if (first) navigate(`/channel/${first.id}`, { replace: true });
    }
  });

  return (
    <div class="flex h-screen">
      <aside class="w-60 bg-gray-800 text-gray-100 flex-shrink-0 flex flex-col">
        <ChannelSidebar user={props.user} onLogout={auth.logout} onAvatarChange={auth.refresh} />
      </aside>
      <main class="flex-1 flex flex-col min-w-0">
        <Suspense>{props.children}</Suspense>
      </main>
    </div>
  );
};

const App: Component<{ children: JSX.Element }> = (props) => {
  const auth = useAuth();

  return (
    <Show when={auth.user() !== undefined} fallback={null}>
      <Show when={auth.user()} fallback={<LoginScreen />}>
        <EventsProvider>
          <ChannelsProvider>
            <AppShell user={auth.user() as User}>{props.children}</AppShell>
          </ChannelsProvider>
        </EventsProvider>
      </Show>
    </Show>
  );
};

export default App;
