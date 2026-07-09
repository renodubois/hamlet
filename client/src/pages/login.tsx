import { useAfterRenderEffect, useSignalState, registerCleanup, If } from "../hooks/react-state";
import { useAuth } from "../contexts/auth";
import { getPublicServerConfig, getServerUrl } from "../api";

export default function LoginScreen() {
  const auth = useAuth();
  const [mode, setMode] = useSignalState<"login" | "register">("login");
  const [server, setServer] = useSignalState(getServerUrl());
  const [username, setUsername] = useSignalState("");
  const [password, setPassword] = useSignalState("");
  const [email, setEmail] = useSignalState("");
  const [error, setError] = useSignalState<string | null>(null);
  const [submitting, setSubmitting] = useSignalState(false);
  const [accountRegistrationEnabled, setAccountRegistrationEnabled] = useSignalState(true);

  const switchMode = (next: "login" | "register") => {
    if (next === "register" && !accountRegistrationEnabled()) return;
    setMode(next);
    setError(null);
  };

  useAfterRenderEffect(() => {
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

    registerCleanup(() => {
      cancelled = true;
    });
  });

  useAfterRenderEffect(() => {
    if (!accountRegistrationEnabled() && mode() === "register") switchMode("login");
  });

  const handleSubmit = async (e: any) => {
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
    <div className="flex h-screen items-center justify-center bg-gray-900">
      <div className="bg-gray-800 rounded-lg p-8 w-96">
        <h1 className="text-gray-100 text-2xl font-bold mb-6">
          {mode() === "login" ? "Sign in" : "Create account"}
        </h1>
        <form onSubmit={handleSubmit} className="flex flex-col gap-4">
          <input
            className="bg-gray-700 text-gray-100 rounded-md p-3 placeholder-gray-400"
            type="text"
            placeholder="Server URL"
            value={server()}
            onInput={(e) => setServer(e.currentTarget.value)}
          />
          <input
            className="bg-gray-700 text-gray-100 rounded-md p-3 placeholder-gray-400"
            type="text"
            placeholder="Username"
            autoComplete="username"
            value={username()}
            onInput={(e) => setUsername(e.currentTarget.value)}
          />
          <input
            className="bg-gray-700 text-gray-100 rounded-md p-3 placeholder-gray-400"
            type="password"
            placeholder="Password"
            autoComplete={mode() === "login" ? "current-password" : "new-password"}
            value={password()}
            onInput={(e) => setPassword(e.currentTarget.value)}
          />
          <If when={mode() === "register" && accountRegistrationEnabled()}>
            <input
              className="bg-gray-700 text-gray-100 rounded-md p-3 placeholder-gray-400"
              type="email"
              placeholder="Email (optional)"
              autoComplete="email"
              value={email()}
              onInput={(e) => setEmail(e.currentTarget.value)}
            />
          </If>
          <If when={error()}>
            <p className="text-red-400 text-sm">{error()}</p>
          </If>
          <button
            className="bg-blue-600 hover:bg-blue-700 text-white rounded-md p-3 font-medium disabled:opacity-50 transition-colors"
            type="submit"
            disabled={submitting()}
          >
            {submitting() ? "Please wait..." : mode() === "login" ? "Sign in" : "Create account"}
          </button>
        </form>
        <If when={import.meta.env.DEV}>
          <div className="mt-4 border-t border-gray-700 pt-4">
            <p className="text-gray-400 text-xs mb-2">Dev shortcuts</p>
            <div className="flex gap-2">
              <button
                className="flex-1 bg-gray-700 hover:bg-gray-600 text-gray-100 rounded-md p-3 font-medium disabled:opacity-50 transition-colors"
                type="button"
                disabled={submitting()}
                onClick={() => devLogin("baipas")}
              >
                Log in as baipas
              </button>
              <button
                className="flex-1 bg-gray-700 hover:bg-gray-600 text-gray-100 rounded-md p-3 font-medium disabled:opacity-50 transition-colors"
                type="button"
                disabled={submitting()}
                onClick={() => devLogin("teo")}
              >
                Log in as teo
              </button>
            </div>
          </div>
        </If>
        <If when={accountRegistrationEnabled()}>
          <p className="text-gray-400 text-sm mt-4 text-center">
            <If
              when={mode() === "login"}
              fallback={
                <>
                  Have an account?{" "}
                  <button
                    className="text-blue-400 hover:underline"
                    onClick={() => switchMode("login")}
                  >
                    Sign in
                  </button>
                </>
              }
            >
              Need an account?{" "}
              <button
                className="text-blue-400 hover:underline"
                onClick={() => switchMode("register")}
              >
                Create one
              </button>
            </If>
          </p>
        </If>
      </div>
    </div>
  );
}
