import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useResource } from "../hooks/use-resource";
import {
  getMe,
  getServerUrl,
  login as apiLogin,
  register as apiRegister,
  logout as apiLogout,
  setServerUrl,
  type User,
} from "../api";

export type AuthStatus = "loading" | "authenticated" | "anonymous";

export interface AuthContextValue {
  user: User | null;
  status: AuthStatus;
  // `null` is the explicit no-error sentinel in the public auth contract.
  // oxlint-disable-next-line typescript/no-redundant-type-constituents
  error: unknown | null;
  login: (server: string, username: string, password: string) => Promise<string | null>;
  register: (
    server: string,
    username: string,
    password: string,
    email?: string,
  ) => Promise<string | null>;
  logout: () => Promise<void>;
  refresh: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | undefined>(undefined);

export function AuthProvider(props: { children: ReactNode }) {
  const [serverKey, setServerKey] = useState(getServerUrl);
  const attemptRef = useRef(0);
  const [resource, controls] = useResource({
    key: serverKey,
    load: (_server, signal) => getMe(signal),
    keepDataOnRefetch: true,
  });

  const selectServer = useCallback((server: string): string => {
    setServerUrl(server);
    const nextServer = getServerUrl();
    setServerKey(nextServer);
    return nextServer;
  }, []);

  const refresh = useCallback(async (): Promise<void> => {
    await controls.refetch();
  }, [controls]);

  const login = useCallback(
    async (server: string, username: string, password: string): Promise<string | null> => {
      const attempt = ++attemptRef.current;
      const selectedServer = selectServer(server);
      try {
        const res = await apiLogin(username, password);
        if (attempt !== attemptRef.current || selectedServer !== getServerUrl()) return null;
        if (res.ok) {
          await controls.refetch();
          return null;
        }
        if (res.status === 401) return "Invalid username or password";
        return "Login failed";
      } catch {
        if (attempt !== attemptRef.current || selectedServer !== getServerUrl()) return null;
        return "Could not reach server";
      }
    },
    [controls, selectServer],
  );

  const register = useCallback(
    async (
      server: string,
      username: string,
      password: string,
      email?: string,
    ): Promise<string | null> => {
      const attempt = ++attemptRef.current;
      const selectedServer = selectServer(server);
      try {
        const res = await apiRegister(username, password, email);
        if (attempt !== attemptRef.current || selectedServer !== getServerUrl()) return null;
        if (res.ok) {
          await controls.refetch();
          return null;
        }
        if (res.status === 409) return "Username already taken";
        if (res.status === 403) return "Registration is disabled on this server";
        return "Registration failed";
      } catch {
        if (attempt !== attemptRef.current || selectedServer !== getServerUrl()) return null;
        return "Could not reach server";
      }
    },
    [controls, selectServer],
  );

  const logout = useCallback(async (): Promise<void> => {
    ++attemptRef.current;
    controls.invalidate(null);
    try {
      await apiLogout();
    } finally {
      // Keep anonymous state authoritative even when logout fails or another
      // caller starts a refresh while the request is in flight.
      controls.invalidate(null);
    }
  }, [controls]);

  const user = resource.data ?? null;
  const status: AuthStatus =
    resource.data !== undefined
      ? resource.data === null
        ? "anonymous"
        : "authenticated"
      : resource.status === "loading" || resource.status === "idle"
        ? "loading"
        : "anonymous";
  const value = useMemo<AuthContextValue>(
    () => ({ user, status, error: resource.error, login, register, logout, refresh }),
    [login, logout, refresh, register, resource.error, status, user],
  );

  return <AuthContext.Provider value={value}>{props.children}</AuthContext.Provider>;
}

export function useAuth() {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used inside AuthProvider");
  return ctx;
}
