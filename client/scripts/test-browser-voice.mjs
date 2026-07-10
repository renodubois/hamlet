#!/usr/bin/env node
import { spawn } from "node:child_process";
import { mkdirSync, readFileSync } from "node:fs";
import net from "node:net";
import process from "node:process";
import { setTimeout as delay } from "node:timers/promises";

const repoRoot = new URL("../..", import.meta.url);
const clientDir = new URL("..", import.meta.url);
const serverDir = new URL("../../server/", import.meta.url);
const serverTargetDir = new URL("../../server/target/", import.meta.url);
const serverUploadsDir = new URL("../../server/uploads/", import.meta.url);
const serverPrivateUploadsDir = new URL("../../server/private-uploads/", import.meta.url);
loadLocalEnv();
const composeProjectName = process.env.HAMLET_VOICE_COMPOSE_PROJECT ?? "hamlet_voice_e2e";
const keepCompose = process.env.HAMLET_VOICE_KEEP_COMPOSE === "1";
const pnpmBin = process.platform === "win32" ? "pnpm.cmd" : "pnpm";
const serverUrl =
  process.env.HAMLET_SERVER_URL ??
  process.env.VITE_HAMLET_DEFAULT_SERVER_URL ??
  "http://127.0.0.1:3030";
const livekitUrl = process.env.LIVEKIT_URL ?? "ws://127.0.0.1:7880";
const livekitTcp = new URL(livekitUrl);
const livekitHost = livekitTcp.hostname;
const livekitPort = Number(livekitTcp.port || "7880");
let composeStopped = false;

function run(command, args, options = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: options.cwd,
      env: options.env ?? process.env,
      stdio: options.stdio ?? "inherit",
      shell: false,
    });
    child.on("error", reject);
    child.on("exit", (code, signal) => {
      if (code === 0) resolve();
      else reject(new Error(`${command} ${args.join(" ")} failed (${signal ?? code})`));
    });
  });
}

async function waitForHttp(url, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if ([200, 204, 301, 302, 400, 401, 403, 404].includes(response.status)) return;
      lastError = new Error(`${url} returned ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await delay(1_000);
  }
  throw new Error(`Timed out waiting for ${url}: ${String(lastError)}`);
}

async function waitForTcp(host, port, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    try {
      await new Promise((resolve, reject) => {
        const socket = net.connect({ host, port });
        socket.setTimeout(1_000);
        socket.on("connect", () => {
          socket.destroy();
          resolve();
        });
        socket.on("timeout", () => {
          socket.destroy(new Error("timeout"));
        });
        socket.on("error", reject);
      });
      return;
    } catch (error) {
      lastError = error;
    }
    await delay(1_000);
  }
  throw new Error(`Timed out waiting for ${host}:${port}: ${String(lastError)}`);
}

async function compose(args) {
  await run("docker", ["compose", "-p", composeProjectName, ...args], { cwd: serverDir });
}

async function main() {
  console.log(`[voice-e2e] repo root: ${repoRoot.pathname}`);
  console.log(
    `[voice-e2e] starting server + LiveKit with docker compose project ${composeProjectName}`,
  );
  // Docker creates bind-mount placeholders as root when these paths are absent.
  // Pre-create them so follow-up host cargo commands (for example Electron
  // E2E's `cargo run`) stay user-writable.
  [serverTargetDir, serverUploadsDir, serverPrivateUploadsDir].forEach((dir) =>
    mkdirSync(dir, { recursive: true }),
  );
  await compose(["up", "-d", "--build", "--remove-orphans"]);

  try {
    console.log(`[voice-e2e] waiting for Hamlet server on ${serverUrl}`);
    await waitForHttp(`${serverUrl}/channels`, 180_000);
    console.log(`[voice-e2e] waiting for LiveKit on ${livekitHost}:${livekitPort}`);
    await waitForTcp(livekitHost, livekitPort, 60_000);
    console.log("[voice-e2e] running Playwright browser voice test");
    await run(pnpmBin, ["exec", "playwright", "test", "-c", "playwright.voice.config.ts"], {
      cwd: clientDir,
    });
  } finally {
    if (keepCompose) {
      console.log(`[voice-e2e] leaving docker compose project ${composeProjectName} running`);
    } else if (!composeStopped) {
      console.log(`[voice-e2e] stopping docker compose project ${composeProjectName}`);
      await compose(["down", "--remove-orphans"]);
      composeStopped = true;
    }
  }
}

function loadLocalEnv() {
  for (const fileUrl of [
    new URL("../../.hamlet-worktree.env", import.meta.url),
    new URL("../.env", import.meta.url),
    new URL("../.env.local", import.meta.url),
  ]) {
    let contents;
    try {
      contents = readFileSync(fileUrl, "utf8");
    } catch {
      continue;
    }

    for (const line of contents.split(/\r?\n/)) {
      const trimmed = line.trim();
      if (trimmed === "" || trimmed.startsWith("#")) continue;
      const match = /^(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)=(.*)$/.exec(trimmed);
      if (match === null) continue;
      const [, key, rawValue] = match;
      if (process.env[key] !== undefined) continue;
      process.env[key] = unquoteEnvValue(rawValue.trim());
    }
  }
}

function unquoteEnvValue(value) {
  if (value.length < 2) return value;
  const quote = value[0];
  if ((quote !== '"' && quote !== "'") || value.at(-1) !== quote) return value;
  const inner = value.slice(1, -1);
  return quote === '"' ? inner.replaceAll('\\"', '"').replaceAll("\\\\", "\\") : inner;
}

main().catch(async (error) => {
  console.error(error);
  if (!keepCompose && !composeStopped) {
    try {
      await compose(["down", "--remove-orphans"]);
      composeStopped = true;
    } catch (downError) {
      console.error(downError);
    }
  }
  process.exitCode = 1;
});
