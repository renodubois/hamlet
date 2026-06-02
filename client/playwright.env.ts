import { readFileSync } from "node:fs";
import path from "node:path";

export function loadHamletEnv(env: NodeJS.ProcessEnv = process.env): NodeJS.ProcessEnv {
  const clientDir = process.cwd();
  const repoRoot = path.resolve(clientDir, "..");

  for (const filePath of [
    path.join(repoRoot, ".hamlet-worktree.env"),
    path.join(clientDir, ".env"),
    path.join(clientDir, ".env.local"),
  ]) {
    loadEnvFile(filePath, env);
  }

  return env;
}

function loadEnvFile(filePath: string, env: NodeJS.ProcessEnv): void {
  let contents: string;
  try {
    contents = readFileSync(filePath, "utf8");
  } catch {
    return;
  }

  for (const line of contents.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (trimmed === "" || trimmed.startsWith("#")) continue;
    const match = /^(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)=(.*)$/.exec(trimmed);
    if (match === null) continue;
    const [, key, rawValue] = match;
    if (env[key] !== undefined) continue;
    env[key] = unquoteEnvValue(rawValue.trim());
  }
}

function unquoteEnvValue(value: string): string {
  if (value.length < 2) return value;
  const quote = value[0];
  if ((quote !== '"' && quote !== "'") || value.at(-1) !== quote) return value;
  const inner = value.slice(1, -1);
  return quote === '"' ? inner.replaceAll('\\"', '"').replaceAll("\\\\", "\\") : inner;
}

export const hamletEnv = loadHamletEnv();
