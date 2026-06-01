import { children, createEffect, ErrorBoundary, Suspense, type Component, Show } from "solid-js";
import { useLocation, useNavigate, type RouteSectionProps } from "@solidjs/router";
import ChannelSidebar from "./components/channel-sidebar";
import { useAuth } from "./contexts/auth";
import { ChannelsProvider, useChannels } from "./contexts/channels";
import { CustomEmojisProvider } from "./contexts/custom-emojis";
import { EventsProvider } from "./contexts/events";
import { type User } from "./api";
import { VoiceChatProvider } from "./contexts/voice-chat";

function ErrorPanel(props: { error: unknown; reset?: () => void; title?: string }) {
  const message = () => (props.error instanceof Error ? props.error.message : String(props.error));
  return (
    <div class="p-8 max-w-lg" role="alert">
      <h2 class="text-lg font-semibold text-red-700">{props.title ?? "Something went wrong"}</h2>
      <p class="mt-2 text-sm text-gray-700">{message()}</p>
      <Show when={props.reset}>
        <button
          type="button"
          class="mt-4 bg-blue-600 hover:bg-blue-700 text-white rounded-md px-4 py-2 text-sm"
          onClick={() => props.reset?.()}
        >
          Try again
        </button>
      </Show>
    </div>
  );
}

const AppShell: Component<{ children?: RouteSectionProps["children"]; user: User }> = (props) => {
  const auth = useAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const { channels } = useChannels();
  const outlet = children(() => props.children);

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
        <ErrorBoundary fallback={(err, reset) => <ErrorPanel error={err} reset={reset} />}>
          <Suspense>{outlet()}</Suspense>
        </ErrorBoundary>
      </main>
    </div>
  );
};

const App: Component<RouteSectionProps> = (props) => {
  const auth = useAuth();
  const navigate = useNavigate();
  const location = useLocation();

  // Route guard: send unauthenticated users to /login, and bounce signed-in
  // users away from /login. Both happen as a side effect of auth state
  // changes; the form on /login itself only needs to call auth.login().
  createEffect(() => {
    const u = auth.user();
    // Resource still loading on first paint — wait for the resolved value.
    if (u === undefined) return;
    const onLogin = location.pathname === "/login";
    if (!u && !onLogin) navigate("/login", { replace: true });
    else if (u && onLogin) navigate("/", { replace: true });
  });

  return (
    <Show when={auth.user() !== undefined} fallback={null}>
      <Show
        when={auth.user()}
        fallback={
          // Unauthenticated. Render the matched route (login). Any non-login
          // path passes through here briefly while the effect above redirects.
          <Show when={location.pathname === "/login"}>
            <Suspense>{props.children}</Suspense>
          </Show>
        }
      >
        <EventsProvider>
          <CustomEmojisProvider>
            <ChannelsProvider>
              <VoiceChatProvider>
                <AppShell user={auth.user() as User}>{props.children}</AppShell>
              </VoiceChatProvider>
            </ChannelsProvider>
          </CustomEmojisProvider>
        </EventsProvider>
      </Show>
    </Show>
  );
};

export default App;
