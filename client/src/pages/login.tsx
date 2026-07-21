import { useEffect, useState, type FormEvent } from "react";
import { useAuth } from "../contexts/auth";
import { getPublicServerConfig, getServerUrl } from "../api";
import { Button } from "../components/ui/button";
import { Card, CardContent, CardHeader } from "../components/ui/card";
import { Input } from "../components/ui/input";
import { Separator } from "../components/ui/separator";

export default function LoginScreen() {
  const auth = useAuth();
  const [mode, setMode] = useState<"login" | "register">("login");
  const [server, setServer] = useState(getServerUrl);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [email, setEmail] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [accountRegistrationEnabled, setAccountRegistrationEnabled] = useState(true);
  const normalizedServer = server.trim().replace(/\/+$/, "");

  const switchMode = (next: "login" | "register") => {
    if (next === "register" && !accountRegistrationEnabled) return;
    setMode(next);
    setError(null);
  };

  useEffect(() => {
    let current = true;
    if (!normalizedServer) {
      setAccountRegistrationEnabled(true);
      return () => {
        current = false;
      };
    }

    void getPublicServerConfig(normalizedServer)
      .then((config) => {
        if (current) setAccountRegistrationEnabled(config.account_registration_enabled);
      })
      .catch(() => {
        if (current) setAccountRegistrationEnabled(true);
      });

    return () => {
      current = false;
    };
  }, [normalizedServer]);

  useEffect(() => {
    if (!accountRegistrationEnabled) {
      setMode((current) => (current === "register" ? "login" : current));
    }
  }, [accountRegistrationEnabled]);

  const handleSubmit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    setError(null);
    if (mode === "register" && !accountRegistrationEnabled) {
      setError("Registration is disabled on this server");
      return;
    }

    setSubmitting(true);
    const err =
      mode === "login"
        ? await auth.login(server, username, password)
        : await auth.register(server, username, password, email || undefined);
    setSubmitting(false);
    if (err) setError(err);
  };

  const devLogin = async (devUsername: string) => {
    setError(null);
    setSubmitting(true);
    const err = await auth.login(server, devUsername, "password");
    setSubmitting(false);
    if (err) setError(err);
  };

  return (
    <div className="flex h-screen items-center justify-center bg-sidebar p-4">
      <Card className="w-96">
        <CardHeader>
          <h1 data-slot="card-title" className="text-2xl leading-snug font-bold tracking-tight">
            {mode === "login" ? "Sign in" : "Create account"}
          </h1>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSubmit} className="flex flex-col gap-4">
            <Input
              type="text"
              placeholder="Server URL"
              value={server}
              onChange={(e) => setServer(e.currentTarget.value)}
            />
            <Input
              type="text"
              placeholder="Username"
              autoComplete="username"
              value={username}
              onChange={(e) => setUsername(e.currentTarget.value)}
            />
            <Input
              type="password"
              placeholder="Password"
              autoComplete={mode === "login" ? "current-password" : "new-password"}
              value={password}
              onChange={(e) => setPassword(e.currentTarget.value)}
            />
            {mode === "register" && accountRegistrationEnabled ? (
              <Input
                type="email"
                placeholder="Email (optional)"
                autoComplete="email"
                value={email}
                onChange={(e) => setEmail(e.currentTarget.value)}
              />
            ) : null}
            {error ? <p className="text-destructive text-sm">{error}</p> : null}
            <Button type="submit" size="lg" disabled={submitting}>
              {submitting ? "Please wait..." : mode === "login" ? "Sign in" : "Create account"}
            </Button>
          </form>
          {import.meta.env.DEV ? (
            <div className="mt-4">
              <Separator className="mb-4" />
              <p className="text-muted-foreground text-xs mb-2">Dev shortcuts</p>
              <div className="flex gap-2">
                <Button
                  className="flex-1"
                  variant="secondary"
                  type="button"
                  disabled={submitting}
                  onClick={() => devLogin("baipas")}
                >
                  Log in as baipas
                </Button>
                <Button
                  className="flex-1"
                  variant="secondary"
                  type="button"
                  disabled={submitting}
                  onClick={() => devLogin("teo")}
                >
                  Log in as teo
                </Button>
              </div>
            </div>
          ) : null}
          {accountRegistrationEnabled ? (
            <p className="text-muted-foreground text-sm mt-4 text-center">
              {mode === "login" ? (
                <>
                  Need an account?{" "}
                  <button
                    className="text-primary hover:underline"
                    onClick={() => switchMode("register")}
                  >
                    Create one
                  </button>
                </>
              ) : (
                <>
                  Have an account?{" "}
                  <button
                    className="text-primary hover:underline"
                    onClick={() => switchMode("login")}
                  >
                    Sign in
                  </button>
                </>
              )}
            </p>
          ) : null}
        </CardContent>
      </Card>
    </div>
  );
}
