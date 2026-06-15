import fs from "node:fs";
import { access, stat } from "node:fs/promises";
import http, { type IncomingMessage, type Server, type ServerResponse } from "node:http";
import path from "node:path";
import {
  DEFAULT_STATIC_RENDERER_HOST,
  DEFAULT_STATIC_RENDERER_PORT,
  STATIC_RENDERER_ORIGIN,
  resolveConfiguredRendererHost,
  resolveConfiguredRendererPort,
} from "./constants";

const INDEX_FILE = "index.html";
const LOOPBACK_HOSTS = new Set(["127.0.0.1", "localhost", "::1"]);

// Browser security posture for packaged Electron renderer responses: keep app
// code self-hosted while allowing Hamlet's direct browser connections to local
// HTTP/SSE, local WebSocket/LiveKit, HTTPS/WSS services, and existing media or
// iframe embed rendering. Tighten individual source classes as product support
// narrows.
const CONTENT_SECURITY_POLICY_DIRECTIVES = [
  "default-src 'self'",
  "base-uri 'self'",
  "object-src 'none'",
  "script-src 'self'",
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' data: blob: http: https:",
  "font-src 'self' data:",
  "connect-src 'self' http://127.0.0.1:* http://localhost:* ws://127.0.0.1:* ws://localhost:* https: wss:",
  "media-src 'self' data: blob: http: https:",
  "frame-src 'self' http: https:",
  "child-src 'self' http: https:",
  "worker-src 'self' blob:",
  "form-action 'self'",
  "frame-ancestors 'none'",
] as const;

export const STATIC_RENDERER_CONTENT_SECURITY_POLICY =
  CONTENT_SECURITY_POLICY_DIRECTIVES.join("; ");

const SECURITY_HEADERS = {
  "Content-Security-Policy": STATIC_RENDERER_CONTENT_SECURITY_POLICY,
  "X-Content-Type-Options": "nosniff",
  "Referrer-Policy": "strict-origin-when-cross-origin",
  "X-Frame-Options": "DENY",
  "Permissions-Policy": [
    "camera=()",
    "microphone=(self)",
    "speaker-selection=(self)",
    "display-capture=(self)",
    "geolocation=()",
    "midi=()",
    "payment=()",
    "serial=()",
    "usb=()",
    "xr-spatial-tracking=()",
  ].join(", "),
} as const;

const MIME_TYPES = new Map<string, string>([
  [".html", "text/html; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".mjs", "text/javascript; charset=utf-8"],
  [".css", "text/css; charset=utf-8"],
  [".json", "application/json; charset=utf-8"],
  [".map", "application/json; charset=utf-8"],
  [".svg", "image/svg+xml"],
  [".png", "image/png"],
  [".jpg", "image/jpeg"],
  [".jpeg", "image/jpeg"],
  [".gif", "image/gif"],
  [".webp", "image/webp"],
  [".ico", "image/x-icon"],
  [".wasm", "application/wasm"],
  [".woff", "font/woff"],
  [".woff2", "font/woff2"],
  [".ttf", "font/ttf"],
  [".otf", "font/otf"],
  [".txt", "text/plain; charset=utf-8"],
]);

export interface StaticRendererEnvironment {
  HAMLET_RENDERER_URL?: string;
  HAMLET_RENDERER_HOST?: string;
  HAMLET_RENDERER_PORT?: string;
}

export interface StaticRendererServerOptions {
  rootDir: string;
  host?: string;
  port?: number;
}

export interface StaticRendererServer {
  readonly origin: string;
  readonly host: string;
  readonly port: number;
  readonly rootDir: string;
  close(): Promise<void>;
}

export class StaticRendererStartupError extends Error {
  readonly code?: string;

  constructor(message: string, options: { code?: string; cause?: unknown } = {}) {
    super(message, { cause: options.cause });
    this.name = "StaticRendererStartupError";
    this.code = options.code;
  }
}

export function shouldServeStaticRenderer(env: StaticRendererEnvironment = process.env): boolean {
  return env.HAMLET_RENDERER_URL === undefined || env.HAMLET_RENDERER_URL.trim() === "";
}

export function resolveRendererDistPath(appPath: string): string {
  return path.join(appPath, "dist");
}

export function staticRendererSecurityHeaders(): Record<string, string> {
  return { ...SECURITY_HEADERS };
}

export function staticRendererContentType(filePath: string): string {
  return MIME_TYPES.get(path.extname(filePath).toLowerCase()) ?? "application/octet-stream";
}

export async function startStaticRendererServer(
  options: StaticRendererServerOptions,
): Promise<StaticRendererServer> {
  const host = options.host ?? resolveConfiguredRendererHost();
  const port = options.port ?? resolveConfiguredRendererPort();
  const rootDir = path.resolve(options.rootDir);

  assertLoopbackHost(host);
  await assertRendererRoot(rootDir);

  const server = http.createServer((request, response) => {
    void serveRequest(rootDir, request, response);
  });

  server.on("clientError", (_error, socket) => {
    socket.end("HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n");
  });

  await listen(server, host, port).catch((cause: unknown) => {
    throw listenStartupError(host, port, cause);
  });

  const address = server.address();
  if (typeof address === "string" || address === null) {
    await closeServer(server);
    throw new StaticRendererStartupError(
      `Static renderer server did not bind to a TCP loopback address.`,
      { code: "INVALID_BIND_ADDRESS" },
    );
  }

  let closed = false;
  const actualPort = address.port;

  return {
    origin: originFor(host, actualPort),
    host,
    port: actualPort,
    rootDir,
    async close() {
      if (closed) return;
      closed = true;
      await closeServer(server);
    },
  };
}

async function serveRequest(
  rootDir: string,
  request: IncomingMessage,
  response: ServerResponse,
): Promise<void> {
  try {
    const method = request.method ?? "GET";
    if (method !== "GET" && method !== "HEAD") {
      sendText(response, method, 405, "Method Not Allowed\n", { Allow: "GET, HEAD" });
      return;
    }

    const pathname = parseSafePathname(request.url ?? "/");
    if (!pathname.ok) {
      sendText(response, method, pathname.status, `${pathname.message}\n`);
      return;
    }

    const filePath = await resolveStaticFile(rootDir, pathname.value);
    if (filePath === null) {
      sendText(response, method, 404, "Not Found\n");
      return;
    }

    await sendFile(response, method, filePath);
  } catch (cause) {
    if (!response.headersSent) {
      sendText(response, request.method ?? "GET", 500, "Internal Server Error\n");
      return;
    }
    response.destroy(cause instanceof Error ? cause : undefined);
  }
}

async function resolveStaticFile(rootDir: string, pathname: string): Promise<string | null> {
  const directPathname = pathname === "/" ? `/${INDEX_FILE}` : pathname;
  const directFile = resolveInsideRoot(rootDir, directPathname);
  const directStat = await maybeStatFile(directFile);
  if (directStat === "file") return directFile;

  if (shouldServeSpaFallback(pathname)) {
    const fallbackFile = path.join(rootDir, INDEX_FILE);
    const fallbackStat = await maybeStatFile(fallbackFile);
    if (fallbackStat === "file") return fallbackFile;
  }

  return null;
}

function shouldServeSpaFallback(pathname: string): boolean {
  if (pathname === "/") return false;
  if (pathname.endsWith("/")) return true;
  return path.posix.extname(pathname) === "";
}

function parseSafePathname(
  requestUrl: string,
): { ok: true; value: string } | { ok: false; status: 400 | 403; message: string } {
  const rawPathname = extractRawPathname(requestUrl);
  if (rawPathname === null) {
    return { ok: false, status: 400, message: "Bad Request" };
  }

  if (containsPathTraversal(rawPathname)) {
    return { ok: false, status: 403, message: "Forbidden" };
  }

  let decodedPathname: string;
  try {
    decodedPathname = decodeURIComponent(rawPathname);
  } catch {
    return { ok: false, status: 400, message: "Bad Request" };
  }

  if (
    decodedPathname.includes("\0") ||
    decodedPathname.includes("\\") ||
    containsPathTraversal(decodedPathname)
  ) {
    return { ok: false, status: 403, message: "Forbidden" };
  }

  return { ok: true, value: decodedPathname };
}

function extractRawPathname(requestUrl: string): string | null {
  if (requestUrl === "" || requestUrl === "*") return null;

  const absoluteUrl = parseAbsoluteRequestUrl(requestUrl);
  if (absoluteUrl !== null) return absoluteUrl.pathname;

  const queryIndex = requestUrl.indexOf("?");
  const rawPathname = queryIndex === -1 ? requestUrl : requestUrl.slice(0, queryIndex);
  if (!rawPathname.startsWith("/")) return null;
  return rawPathname;
}

function parseAbsoluteRequestUrl(requestUrl: string): URL | null {
  if (!/^[a-zA-Z][a-zA-Z\d+.-]*:\/\//.test(requestUrl)) return null;
  try {
    const parsed = new URL(requestUrl);
    if (parsed.protocol !== "http:") return null;
    return parsed;
  } catch {
    return null;
  }
}

function containsPathTraversal(pathname: string): boolean {
  let candidate = pathname;

  for (let i = 0; i < 3; i += 1) {
    const normalizedSeparators = candidate.replaceAll("\\", "/");
    if (normalizedSeparators.split("/").some((segment) => segment === "..")) return true;

    try {
      const decoded = decodeURIComponent(candidate);
      if (decoded === candidate) return false;
      candidate = decoded;
    } catch {
      return false;
    }
  }

  return false;
}

function resolveInsideRoot(rootDir: string, pathname: string): string {
  const relativePath = pathname.replace(/^\/+/, "");
  const absolutePath = path.resolve(rootDir, relativePath);
  const relativeToRoot = path.relative(rootDir, absolutePath);

  if (
    relativeToRoot === "" ||
    (!relativeToRoot.startsWith("..") && !path.isAbsolute(relativeToRoot))
  ) {
    return absolutePath;
  }

  throw new StaticRendererStartupError("Resolved renderer path escaped the static root.", {
    code: "PATH_TRAVERSAL",
  });
}

async function maybeStatFile(filePath: string): Promise<"file" | "directory" | null> {
  try {
    const fileStat = await stat(filePath);
    if (fileStat.isFile()) return "file";
    if (fileStat.isDirectory()) return "directory";
    return null;
  } catch (cause) {
    if (isNodeErrorCode(cause, "ENOENT") || isNodeErrorCode(cause, "ENOTDIR")) return null;
    throw cause;
  }
}

async function sendFile(response: ServerResponse, method: string, filePath: string): Promise<void> {
  const fileStat = await stat(filePath);
  const headers = {
    ...staticRendererSecurityHeaders(),
    "Content-Type": staticRendererContentType(filePath),
    "Content-Length": String(fileStat.size),
  };

  response.writeHead(200, headers);
  if (method === "HEAD") {
    response.end();
    return;
  }

  await new Promise<void>((resolve, reject) => {
    const stream = fs.createReadStream(filePath);
    stream.once("error", reject);
    response.once("error", reject);
    response.once("finish", resolve);
    stream.pipe(response);
  });
}

function sendText(
  response: ServerResponse,
  method: string,
  status: number,
  body: string,
  extraHeaders: Record<string, string> = {},
): void {
  const payload = Buffer.from(body);
  response.writeHead(status, {
    ...staticRendererSecurityHeaders(),
    ...extraHeaders,
    "Content-Type": "text/plain; charset=utf-8",
    "Content-Length": String(method === "HEAD" ? 0 : payload.byteLength),
  });

  if (method === "HEAD") {
    response.end();
    return;
  }

  response.end(payload);
}

async function assertRendererRoot(rootDir: string): Promise<void> {
  try {
    const rootStat = await stat(rootDir);
    if (!rootStat.isDirectory()) {
      throw new StaticRendererStartupError(`Renderer build path is not a directory: ${rootDir}`, {
        code: "MISSING_RENDERER_DIST",
      });
    }

    const indexFile = path.join(rootDir, INDEX_FILE);
    const indexStat = await stat(indexFile);
    if (!indexStat.isFile()) {
      throw new StaticRendererStartupError(`Renderer entrypoint is not a file: ${indexFile}`, {
        code: "MISSING_RENDERER_DIST",
      });
    }

    await access(indexFile, fs.constants.R_OK);
  } catch (cause) {
    if (cause instanceof StaticRendererStartupError) throw cause;
    throw new StaticRendererStartupError(
      `Renderer build is missing or unreadable at ${rootDir}. Run npm run build:renderer before launching production Electron.`,
      { code: "MISSING_RENDERER_DIST", cause },
    );
  }
}

function assertLoopbackHost(host: string): void {
  if (LOOPBACK_HOSTS.has(host)) return;
  throw new StaticRendererStartupError(
    `Refusing to bind static renderer server to non-loopback host "${host}".`,
    { code: "UNSAFE_HOST" },
  );
}

function listen(server: Server, host: string, port: number): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    const onError = (error: Error) => {
      server.off("listening", onListening);
      reject(error);
    };
    const onListening = () => {
      server.off("error", onError);
      resolve();
    };

    server.once("error", onError);
    server.once("listening", onListening);
    server.listen(port, host);
  });
}

function listenStartupError(
  host: string,
  port: number,
  cause: unknown,
): StaticRendererStartupError {
  const code = nodeErrorCode(cause);
  if (code === "EADDRINUSE") {
    return new StaticRendererStartupError(
      `Static renderer server could not start because ${host}:${port} is already in use. Close the other Hamlet Electron instance or free the port and relaunch.`,
      { code, cause },
    );
  }

  return new StaticRendererStartupError(
    `Static renderer server could not start on ${host}:${port}.`,
    { code, cause },
  );
}

function closeServer(server: Server): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    server.close((error) => {
      if (error !== undefined) {
        reject(error);
        return;
      }
      resolve();
    });
  });
}

function originFor(host: string, port: number): string {
  if (host === DEFAULT_STATIC_RENDERER_HOST && port === DEFAULT_STATIC_RENDERER_PORT) {
    return STATIC_RENDERER_ORIGIN;
  }
  return `http://${hostForOrigin(host)}:${port}`;
}

function hostForOrigin(host: string): string {
  return host.includes(":") ? `[${host}]` : host;
}

function nodeErrorCode(cause: unknown): string | undefined {
  if (typeof cause !== "object" || cause === null || !("code" in cause)) return undefined;
  const code = (cause as { code: unknown }).code;
  return typeof code === "string" ? code : undefined;
}

function isNodeErrorCode(cause: unknown, code: string): boolean {
  return nodeErrorCode(cause) === code;
}
