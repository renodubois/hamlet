import { createEffect, createSignal, onCleanup, Show } from "solid-js";
import { useAuth } from "../contexts/auth";
import { getPublicServerConfig, getServerUrl } from "../api";

export default function LoginScreen() {
  const auth = useAuth();
  const [mode, setMode] = createSignal<"login" | "register">("login");
  const [server, setServer] = createSignal(getServerUrl());
  const [username, setUsername] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [email, setEmail] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [submitting, setSubmitting] = createSignal(false);
  const [accountRegistrationEnabled, setAccountRegistrationEnabled] = createSignal(true);

  const switchMode = (next: "login" | "register") => {
    if (next === "register" && !accountRegistrationEnabled()) return;
    setMode(next);
    setError(null);
  };

  createEffect(() => {
    const currentServer = server().trim();
    let cancelled = false;
    if (!currentServer) {
      setAccountRegistrationEnabled(true);
      return;
    }

    getPublicServerConfig(currentServer)
      .then((config) => {
        if (!cancelled) setAccountRegistrationEnabled(config.account_registration_enabled);
      })
      .catch(() => {
        if (!cancelled) setAccountRegistrationEnabled(true);
      });

    onCleanup(() => {
      cancelled = true;
    });
  });

  createEffect(() => {
    if (!accountRegistrationEnabled() && mode() === "register") switchMode("login");
  });

  const handleSubmit = async (e: SubmitEvent) => {
    e.preventDefault();
    setError(null);
    if (mode() === "register" && !accountRegistrationEnabled()) {
      setError("Registration is disabled on this server");
      return;
    }

    setSubmitting(true);
    const err =
      mode() === "login"
        ? await auth.login(server(), username(), password())
        : await auth.register(server(), username(), password(), email() || undefined);
    setSubmitting(false);
    if (err) setError(err);
  };

  const devLogin = async (devUsername: string) => {
    setError(null);
    setSubmitting(true);
    const err = await auth.login(server(), devUsername, "password");
    setSubmitting(false);
    if (err) setError(err);
  };

  return (
    <div class="flex h-screen items-center justify-center bg-gray-900">
      <div class="bg-gray-800 rounded-lg p-8 w-96">
        <h1 class="text-gray-100 text-2xl font-bold mb-6">
          {mode() === "login" ? "Sign in" : "Create account"}
        </h1>
        <form onSubmit={handleSubmit} class="flex flex-col gap-4">
          <input
            class="bg-gray-700 text-gray-100 rounded-md p-3 placeholder-gray-400"
            type="text"
            placeholder="Server URL"
            value={server()}
            onInput={(e) => setServer(e.currentTarget.value)}
          />
          <input
            class="bg-gray-700 text-gray-100 rounded-md p-3 placeholder-gray-400"
            type="text"
            placeholder="Username"
            autocomplete="username"
            value={username()}
            onInput={(e) => setUsername(e.currentTarget.value)}
          />
          <input
            class="bg-gray-700 text-gray-100 rounded-md p-3 placeholder-gray-400"
            type="password"
            placeholder="Password"
            autocomplete={mode() === "login" ? "current-password" : "new-password"}
            value={password()}
            onInput={(e) => setPassword(e.currentTarget.value)}
          />
          <Show when={mode() === "register" && accountRegistrationEnabled()}>
            <input
              class="bg-gray-700 text-gray-100 rounded-md p-3 placeholder-gray-400"
              type="email"
              placeholder="Email (optional)"
              autocomplete="email"
              value={email()}
              onInput={(e) => setEmail(e.currentTarget.value)}
            />
          </Show>
          <Show when={error()}>
            <p class="text-red-400 text-sm">{error()}</p>
          </Show>
          <button
            class="bg-blue-600 hover:bg-blue-700 text-white rounded-md p-3 font-medium disabled:opacity-50 transition-colors"
            type="submit"
            disabled={submitting()}
          >
            {submitting() ? "Please wait..." : mode() === "login" ? "Sign in" : "Create account"}
          </button>
        </form>
        <Show when={import.meta.env.DEV}>
          <div class="mt-4 border-t border-gray-700 pt-4">
            <p class="text-gray-400 text-xs mb-2">Dev shortcuts</p>
            <div class="flex gap-2">
              <button
                class="flex-1 bg-gray-700 hover:bg-gray-600 text-gray-100 rounded-md p-3 font-medium disabled:opacity-50 transition-colors"
                type="button"
                disabled={submitting()}
                onClick={() => devLogin("baipas")}
              >
                Log in as baipas
              </button>
              <button
                class="flex-1 bg-gray-700 hover:bg-gray-600 text-gray-100 rounded-md p-3 font-medium disabled:opacity-50 transition-colors"
                type="button"
                disabled={submitting()}
                onClick={() => devLogin("teo")}
              >
                Log in as teo
              </button>
            </div>
          </div>
        </Show>
        <Show when={accountRegistrationEnabled()}>
          <p class="text-gray-400 text-sm mt-4 text-center">
            <Show
              when={mode() === "login"}
              fallback={
                <>
                  Have an account?{" "}
                  <button class="text-blue-400 hover:underline" onClick={() => switchMode("login")}>
                    Sign in
                  </button>
                </>
              }
            >
              Need an account?{" "}
              <button class="text-blue-400 hover:underline" onClick={() => switchMode("register")}>
                Create one
              </button>
            </Show>
          </p>
        </Show>
      </div>
    </div>
  );
}
