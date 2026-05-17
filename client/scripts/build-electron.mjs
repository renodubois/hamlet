import { build } from "esbuild";
import { rm } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const rootDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const outdir = path.join(rootDir, "dist-electron");

await rm(outdir, { recursive: true, force: true });

await build({
  absWorkingDir: rootDir,
  entryPoints: ["electron/main.ts", "electron/preload.ts"],
  bundle: true,
  platform: "node",
  target: "node22",
  format: "cjs",
  outdir: "dist-electron",
  outExtension: { ".js": ".cjs" },
  sourcemap: true,
  sourcesContent: false,
  external: ["electron"],
  logLevel: "info",
});
