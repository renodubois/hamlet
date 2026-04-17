import { createEffect, createResource, type JSX, Suspense, type Component, Show } from "solid-js";
import { useLocation, useNavigate } from "@solidjs/router";
import ChannelSidebar from "./components/channel_sidebar";
import { useAuth } from "./auth_context";
import { ChannelsProvider } from "./channels_context";
import LoginScreen from "./pages/login";
import { listChannels, type User } from "./api";

const App: Component<{ children: JSX.Element }> = (props) => {
  const auth = useAuth();
  const navigate = useNavigate();
  const location = useLocation();

  const [channels] = createResource(
    () => auth.user() || null,
    () => listChannels(),
  );

  createEffect(() => {
    if (!auth.user()) return;
    const ch = channels();
    if (location.pathname === "/" && ch && ch.length > 0) {
      navigate(`/channel/${ch[0].id}`, { replace: true });
    }
  });

  return (
    <Show when={auth.user() !== undefined} fallback={null}>
      <Show when={auth.user()} fallback={<LoginScreen />}>
        <div class="flex h-screen">
          <aside class="w-60 bg-gray-800 text-gray-100 flex-shrink-0 flex flex-col">
            <ChannelSidebar channels={channels} user={auth.user() as User} onLogout={auth.logout} />
          </aside>
          <main class="flex-1 flex flex-col min-w-0">
            <ChannelsProvider channels={channels}>
              <Suspense>{props.children}</Suspense>
            </ChannelsProvider>
          </main>
        </div>
      </Show>
    </Show>
  );
};

export default App;
