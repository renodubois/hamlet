import { spawn } from "node:child_process";
import path from "node:path";
import process from "node:process";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";

const rootDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const rendererUrl = process.env.HAMLET_RENDERER_URL ?? "http://127.0.0.1:1422";
const npmBin = process.platform === "win32" ? "npm.cmd" : "npm";
const electronBin = path.join(
  rootDir,
  "node_modules",
  ".bin",
  process.platform === "win32" ? "electron.cmd" : "electron",
);
const children = new Set();

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.once(signal, () => {
    stopChildren();
    process.kill(process.pid, signal);
  });
}

await run(npmBin, ["run", "electron:build"]);

const vite = spawnChild(npmBin, ["run", "dev"], {
  env: { ...process.env, BROWSER: "none" },
});

try {
  await Promise.race([
    waitForRenderer(rendererUrl),
    waitForExit(vite).then((code) => {
      throw new Error(
        `Vite dev server exited before Electron started (code ${formatExitCode(code)}).`,
      );
    }),
  ]);

  const electron = spawnChild(electronBin, ["."], {
    env: { ...process.env, HAMLET_RENDERER_URL: rendererUrl },
  });
  process.exitCode = (await waitForExit(electron)) ?? 0;
} catch (error) {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
} finally {
  stopChildren();
}

function spawnChild(command, args, options = {}) {
  const child = spawn(command, args, {
    cwd: rootDir,
    stdio: "inherit",
    ...options,
  });
  children.add(child);
  child.once("exit", () => children.delete(child));
  return child;
}

function run(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawnChild(command, args);
    child.once("error", reject);
    child.once("exit", (code) => {
      if (code === 0) {
        resolve();
      } else {
        reject(
          new Error(`${command} ${args.join(" ")} failed with exit code ${formatExitCode(code)}.`),
        );
      }
    });
  });
}

function waitForExit(child) {
  return new Promise((resolve) => {
    child.once("exit", (code) => resolve(code));
  });
}

function formatExitCode(code) {
  return typeof code === "number" ? String(code) : "unknown";
}

async function waitForRenderer(url) {
  const deadline = Date.now() + 30_000;

  while (Date.now() < deadline) {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 1_000);
    try {
      const response = await fetch(url, { signal: controller.signal });
      if (response.ok) return;
    } catch {
      // The dev server is still starting.
    } finally {
      clearTimeout(timeout);
    }
    await delay(250);
  }

  throw new Error(`Timed out waiting for renderer dev server at ${url}.`);
}

function stopChildren() {
  for (const child of children) {
    if (!child.killed) child.kill();
  }
}
