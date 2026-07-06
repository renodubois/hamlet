const DEFAULT_SERVER = import.meta.env.VITE_HAMLET_DEFAULT_SERVER_URL ?? "http://127.0.0.1:3030";

const CSRF_COOKIE = "hamlet_csrf";
const CSRF_HEADER = "X-Hamlet-CSRF";
const UNSAFE_METHODS = new Set(["POST", "PUT", "PATCH", "DELETE"]);
const CSRF_EXEMPT_PATHS = new Set(["/login", "/register", "/logout"]);

type CachedCsrfToken = {
  serverUrl: string;
  token: string;
};

let cachedCsrfToken: CachedCsrfToken | undefined;
let csrfTokenRequest: Promise<string> | undefined;

export function getServerUrl(): string {
  return localStorage.getItem("hamlet.serverUrl") ?? DEFAULT_SERVER;
}

export function resolveServerUrl(pathOrUrl: string): string {
  if (/^https?:\/\//i.test(pathOrUrl)) return pathOrUrl;

  const serverUrl = getServerUrl();
  const base = serverUrl.endsWith("/") ? serverUrl : `${serverUrl}/`;
  try {
    return new URL(pathOrUrl, base).toString();
  } catch {
    return `${serverUrl.replace(/\/+$/, "")}/${pathOrUrl.replace(/^\/+/, "")}`;
  }
}

export function setServerUrl(url: string): void {
  clearCachedCsrfToken();
  localStorage.setItem("hamlet.serverUrl", url);
}

export function clearCachedCsrfToken(): void {
  cachedCsrfToken = undefined;
  csrfTokenRequest = undefined;
}

export async function apiFetch(path: string, init?: RequestInit): Promise<Response> {
  const serverUrl = getServerUrl();
  const requestInit: RequestInit = { credentials: "include", ...init };

  if (shouldAttachCsrf(path, requestInit.method)) {
    const token = await getCsrfToken(serverUrl);
    requestInit.headers = withHeader(requestInit.headers, CSRF_HEADER, token);
  }

  const response = await fetch(`${serverUrl}${path}`, requestInit);
  if (isAuthBoundary(path, requestInit.method)) clearCachedCsrfToken();
  return response;
}

function shouldAttachCsrf(path: string, method: string | undefined): boolean {
  const normalizedMethod = (method ?? "GET").toUpperCase();
  if (!UNSAFE_METHODS.has(normalizedMethod)) return false;

  return !CSRF_EXEMPT_PATHS.has(pathnameFor(path));
}

function isAuthBoundary(path: string, method: string | undefined): boolean {
  const normalizedMethod = (method ?? "GET").toUpperCase();
  return normalizedMethod === "POST" && CSRF_EXEMPT_PATHS.has(pathnameFor(path));
}

async function getCsrfToken(serverUrl: string): Promise<string> {
  if (cachedCsrfToken?.serverUrl === serverUrl) return cachedCsrfToken.token;

  const cookieToken = csrfTokenFromDocumentCookie();
  if (cookieToken) {
    cachedCsrfToken = { serverUrl, token: cookieToken };
    return cookieToken;
  }

  csrfTokenRequest ??= fetchCsrfToken(serverUrl).finally(() => {
    csrfTokenRequest = undefined;
  });

  return csrfTokenRequest;
}

async function fetchCsrfToken(serverUrl: string): Promise<string> {
  const res = await fetch(`${serverUrl}/csrf`, { credentials: "include" });
  if (!res.ok) {
    throw new Error(`CSRF token request failed: ${res.status}`);
  }

  const data: unknown = await res.json();
  if (!isCsrfResponse(data)) {
    throw new Error("CSRF token response did not include a token");
  }

  cachedCsrfToken = { serverUrl, token: data.token };
  return data.token;
}

function isCsrfResponse(value: unknown): value is { token: string } {
  return (
    typeof value === "object" &&
    value !== null &&
    "token" in value &&
    typeof (value as { token?: unknown }).token === "string" &&
    (value as { token: string }).token.length > 0
  );
}

function csrfTokenFromDocumentCookie(): string | undefined {
  if (typeof document === "undefined") return undefined;

  return document.cookie
    .split(";")
    .map((cookie) => cookie.trim())
    .find((cookie) => cookie.startsWith(`${CSRF_COOKIE}=`))
    ?.slice(CSRF_COOKIE.length + 1);
}

function withHeader(headers: HeadersInit | undefined, name: string, value: string): HeadersInit {
  if (headers instanceof Headers) {
    const next = new Headers(headers);
    next.set(name, value);
    return next;
  }

  if (Array.isArray(headers)) {
    return [...headers.filter(([key]) => key.toLowerCase() !== name.toLowerCase()), [name, value]];
  }

  return { ...headers, [name]: value };
}

function pathnameFor(path: string): string {
  try {
    return new URL(path, "http://hamlet.local").pathname;
  } catch {
    return path.split("?")[0] ?? path;
  }
}
