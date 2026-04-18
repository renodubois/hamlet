import { createContext, createResource, useContext, type JSX, type Resource } from "solid-js";
import {
  getMe,
  login as apiLogin,
  register as apiRegister,
  logout as apiLogout,
  setServerUrl,
  type User,
} from "./api";

interface AuthContextValue {
  user: Resource<User | null>;
  login: (server: string, username: string, password: string) => Promise<string | null>;
  register: (
    server: string,
    username: string,
    password: string,
    email?: string,
  ) => Promise<string | null>;
  logout: () => Promise<void>;
  refresh: () => void;
}

const AuthContext = createContext<AuthContextValue>();

export function AuthProvider(props: { children: JSX.Element }) {
  const [user, { refetch }] = createResource(getMe);

  const login = async (
    server: string,
    username: string,
    password: string,
  ): Promise<string | null> => {
    setServerUrl(server);
    try {
      const res = await apiLogin(username, password);
      if (res.ok) {
        void refetch();
        return null;
      }
      if (res.status === 401) return "Invalid username or password";
      return "Login failed";
    } catch {
      return "Could not reach server";
    }
  };

  const register = async (
    server: string,
    username: string,
    password: string,
    email?: string,
  ): Promise<string | null> => {
    setServerUrl(server);
    try {
      const res = await apiRegister(username, password, email);
      if (res.ok) {
        void refetch();
        return null;
      }
      if (res.status === 409) return "Username already taken";
      return "Registration failed";
    } catch {
      return "Could not reach server";
    }
  };

  const logout = async () => {
    try {
      await apiLogout();
    } finally {
      void refetch();
    }
  };

  const refresh = () => {
    void refetch();
  };

  return (
    <AuthContext.Provider value={{ user, login, register, logout, refresh }}>
      {props.children}
    </AuthContext.Provider>
  );
}

export function useAuth() {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used inside AuthProvider");
  return ctx;
}
