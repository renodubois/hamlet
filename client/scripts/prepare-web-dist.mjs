import { copyFile, mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const defaultRootDir = path.resolve(scriptDir, "..");

export async function prepareWebDist({ rootDir = defaultRootDir, distDir } = {}) {
  const outputDir = distDir ?? path.join(rootDir, "dist");
  const indexPath = path.join(outputDir, "index.html");

  await mkdir(outputDir, { recursive: true });
  await copyFile(indexPath, path.join(outputDir, "404.html"));
  await writeFile(path.join(outputDir, ".nojekyll"), "", "utf8");
}

if (
  process.argv[1] !== undefined &&
  fileURLToPath(import.meta.url) === path.resolve(process.argv[1])
) {
  await prepareWebDist();
}
