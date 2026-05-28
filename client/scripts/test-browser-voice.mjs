#!/usr/bin/env node
import { spawn } from "node:child_process";
import net from "node:net";
import process from "node:process";
import { setTimeout as delay } from "node:timers/promises";

const repoRoot = new URL("../..", import.meta.url);
const clientDir = new URL("..", import.meta.url);
const serverDir = new URL("../../server/", import.meta.url);
const composeProjectName = process.env.HAMLET_VOICE_COMPOSE_PROJECT ?? "hamlet_voice_e2e";
const keepCompose = process.env.HAMLET_VOICE_KEEP_COMPOSE === "1";
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
  await compose(["up", "-d", "--build", "--remove-orphans"]);

  try {
    console.log("[voice-e2e] waiting for Hamlet server on 127.0.0.1:3030");
    await waitForHttp("http://127.0.0.1:3030/channels", 180_000);
    console.log("[voice-e2e] waiting for LiveKit on 127.0.0.1:7880");
    await waitForTcp("127.0.0.1", 7880, 60_000);
    console.log("[voice-e2e] running Playwright browser voice test");
    await run("npx", ["playwright", "test", "-c", "playwright.voice.config.ts"], {
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
