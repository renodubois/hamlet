#!/usr/bin/env node
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  ALPHA_PACKAGE_METADATA,
  packagedLaunchEnvironment,
  readLastUnpackedManifest,
  resolvePackagedExecutable,
} from "./package-config.mjs";

const rootDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

try {
  const manifest = await readLastUnpackedManifest(rootDir);
  const executable = await resolvePackagedExecutable(manifest);
  const child = spawn(executable, [], {
    cwd: path.dirname(executable),
    detached: true,
    env: packagedLaunchEnvironment(),
    stdio: "ignore",
  });

  child.unref();
  console.log(`${ALPHA_PACKAGE_METADATA.productName} launched from:`);
  console.log(`  ${executable}`);
} catch (error) {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
}
