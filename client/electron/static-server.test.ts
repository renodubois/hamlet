import { afterEach, describe, expect, it } from "vitest";
import { mkdtemp, rm, writeFile, mkdir } from "node:fs/promises";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import {
  STATIC_RENDERER_CONTENT_SECURITY_POLICY,
  StaticRendererStartupError,
  shouldServeStaticRenderer,
  startStaticRendererServer,
  staticRendererContentType,
  type StaticRendererServer,
} from "./static-server";

interface RawHttpResponse {
  status: number;
  headers: Record<string, string>;
  body: string;
}

const tempDirs: string[] = [];
const servers: StaticRendererServer[] = [];

afterEach(async () => {
  while (servers.length > 0) {
    const server = servers.pop();
    if (server !== undefined) await server.close();
  }

  while (tempDirs.length > 0) {
    const dir = tempDirs.pop();
    if (dir !== undefined) await rm(dir, { recursive: true, force: true });
  }
});

describe("static renderer server", () => {
  it("serves built renderer assets from a loopback HTTP origin", async () => {
    const rootDir = await createRendererFixture();
    const server = await startTestServer(rootDir);

    expect(server.origin).toBe(`http://127.0.0.1:${server.port}`);
    const response = await request(server, "/assets/app.js");

    expect(response.status).toBe(200);
    expect(response.headers["content-type"]).toBe("text/javascript; charset=utf-8");
    expect(response.body).toContain("renderer loaded");
  });

  it("falls back to index.html for deep SPA routes", async () => {
    const rootDir = await createRendererFixture();
    const server = await startTestServer(rootDir);

    const response = await request(server, "/channel/12345");

    expect(response.status).toBe(200);
    expect(response.headers["content-type"]).toBe("text/html; charset=utf-8");
    expect(response.body).toContain('<div id="root"></div>');
  });

  it("does not use the SPA fallback for missing asset paths", async () => {
    const rootDir = await createRendererFixture();
    const server = await startTestServer(rootDir);

    const response = await request(server, "/assets/missing.js");

    expect(response.status).toBe(404);
    expect(response.headers["content-type"]).toBe("text/plain; charset=utf-8");
    expect(response.body).toBe("Not Found\n");
  });

  it("sends the packaged renderer security headers", async () => {
    const rootDir = await createRendererFixture();
    const server = await startTestServer(rootDir);

    const response = await request(server, "/");

    expect(response.headers["content-security-policy"]).toBe(
      STATIC_RENDERER_CONTENT_SECURITY_POLICY,
    );
    expect(response.headers["content-security-policy"]).toContain(
      "connect-src 'self' http://127.0.0.1:* http://localhost:* ws://127.0.0.1:* ws://localhost:* https: wss:",
    );
    expect(response.headers["content-security-policy"]).toContain("frame-src 'self' http: https:");
    expect(response.headers["content-security-policy"]).toContain("object-src 'none'");
    expect(response.headers["x-content-type-options"]).toBe("nosniff");
    expect(response.headers["referrer-policy"]).toBe("strict-origin-when-cross-origin");
    expect(response.headers["x-frame-options"]).toBe("DENY");
    expect(response.headers["permissions-policy"]).toContain("microphone=(self)");
  });

  it("rejects path traversal before serving or falling back", async () => {
    const rootDir = await createRendererFixture();
    const server = await startTestServer(rootDir);

    await expect(request(server, "/%2e%2e/package.json")).resolves.toMatchObject({
      status: 403,
      body: "Forbidden\n",
    });
    await expect(request(server, "/assets/%2e%2e/%2e%2e/package.json")).resolves.toMatchObject({
      status: 403,
      body: "Forbidden\n",
    });
  });

  it("maps common renderer asset MIME types", async () => {
    const rootDir = await createRendererFixture();
    const server = await startTestServer(rootDir);

    const cases = [
      ["/", "text/html; charset=utf-8"],
      ["/assets/app.css", "text/css; charset=utf-8"],
      ["/assets/logo.svg", "image/svg+xml"],
      ["/assets/font.woff2", "font/woff2"],
      ["/assets/data.bin", "application/octet-stream"],
    ] as const;

    for (const [pathname, contentType] of cases) {
      const response = await request(server, pathname);
      expect(response.status).toBe(200);
      expect(response.headers["content-type"]).toBe(contentType);
    }

    expect(staticRendererContentType("bundle.unknown")).toBe("application/octet-stream");
  });

  it("surfaces missing renderer build startup errors", async () => {
    const rootDir = await createTempDir();

    await expect(startStaticRendererServer({ rootDir, port: 0 })).rejects.toMatchObject({
      name: "StaticRendererStartupError",
      code: "MISSING_RENDERER_DIST",
    });
  });

  it("surfaces unsafe bind host startup errors", async () => {
    const rootDir = await createRendererFixture();

    await expect(
      startStaticRendererServer({ rootDir, host: "0.0.0.0", port: 0 }),
    ).rejects.toMatchObject({
      name: "StaticRendererStartupError",
      code: "UNSAFE_HOST",
    });
  });

  it("surfaces port conflict startup errors", async () => {
    const rootDir = await createRendererFixture();
    const first = await startTestServer(rootDir);

    await expect(
      startStaticRendererServer({ rootDir, host: first.host, port: first.port }),
    ).rejects.toMatchObject({
      name: "StaticRendererStartupError",
      code: "EADDRINUSE",
    });
  });

  it("only starts the packaged static server when no renderer URL override is present", () => {
    expect(shouldServeStaticRenderer({})).toBe(true);
    expect(shouldServeStaticRenderer({ HAMLET_RENDERER_URL: "" })).toBe(true);
    expect(shouldServeStaticRenderer({ HAMLET_RENDERER_URL: "http://127.0.0.1:1422" })).toBe(false);
  });

  it("uses a typed startup error for clear launch failures", () => {
    const error = new StaticRendererStartupError("boom", { code: "TEST" });

    expect(error.name).toBe("StaticRendererStartupError");
    expect(error.code).toBe("TEST");
  });
});

async function createRendererFixture(): Promise<string> {
  const rootDir = await createTempDir();
  await mkdir(path.join(rootDir, "assets"));
  await writeFile(
    path.join(rootDir, "index.html"),
    '<!doctype html><html><head><title>Hamlet</title></head><body><div id="root"></div><script type="module" src="/assets/app.js"></script></body></html>',
  );
  await writeFile(path.join(rootDir, "assets", "app.js"), 'console.log("renderer loaded");');
  await writeFile(path.join(rootDir, "assets", "app.css"), "body { color: #fff; }");
  await writeFile(path.join(rootDir, "assets", "logo.svg"), '<svg role="img"></svg>');
  await writeFile(path.join(rootDir, "assets", "font.woff2"), "fake font");
  await writeFile(path.join(rootDir, "assets", "data.bin"), "binary");
  return rootDir;
}

async function createTempDir(): Promise<string> {
  const rootDir = await mkdtemp(path.join(os.tmpdir(), "hamlet-static-renderer-"));
  tempDirs.push(rootDir);
  return rootDir;
}

async function startTestServer(rootDir: string): Promise<StaticRendererServer> {
  const server = await startStaticRendererServer({ rootDir, port: 0 });
  servers.push(server);
  return server;
}

function request(server: StaticRendererServer, pathname: string): Promise<RawHttpResponse> {
  return new Promise<RawHttpResponse>((resolve, reject) => {
    const chunks: Buffer[] = [];
    const socket = net.connect(server.port, server.host, () => {
      socket.write(
        `GET ${pathname} HTTP/1.1\r\nHost: ${server.host}:${server.port}\r\nConnection: close\r\n\r\n`,
      );
    });

    socket.on("data", (chunk) => {
      chunks.push(chunk);
    });
    socket.on("error", reject);
    socket.on("end", () => {
      try {
        resolve(parseRawHttpResponse(Buffer.concat(chunks).toString("utf8")));
      } catch (error) {
        reject(error);
      }
    });
  });
}

function parseRawHttpResponse(raw: string): RawHttpResponse {
  const separator = "\r\n\r\n";
  const separatorIndex = raw.indexOf(separator);
  if (separatorIndex === -1) throw new Error("Response did not include an HTTP header block.");

  const headerBlock = raw.slice(0, separatorIndex);
  const body = raw.slice(separatorIndex + separator.length);
  const headerLines = headerBlock.split("\r\n");
  const statusLine = headerLines.shift();
  if (statusLine === undefined) throw new Error("Response did not include a status line.");

  const statusMatch = /^HTTP\/\d\.\d (\d{3})/.exec(statusLine);
  if (statusMatch === null) throw new Error(`Invalid HTTP status line: ${statusLine}`);
  const status = Number(statusMatch[1]);
  const headers: Record<string, string> = {};

  for (const line of headerLines) {
    const colon = line.indexOf(":");
    if (colon === -1) continue;
    headers[line.slice(0, colon).toLowerCase()] = line.slice(colon + 1).trim();
  }

  return { status, headers, body };
}
