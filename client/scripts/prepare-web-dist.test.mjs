import { mkdir, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { prepareWebDist } from "./prepare-web-dist.mjs";

const tempDirs = [];

afterEach(async () => {
  while (tempDirs.length > 0) {
    const dir = tempDirs.pop();
    if (dir !== undefined) await rm(dir, { recursive: true, force: true });
  }
});

describe("web renderer deployment output", () => {
  it("copies index.html to 404.html and writes .nojekyll", async () => {
    const rootDir = await mkTempDir();
    const distDir = path.join(rootDir, "dist");
    const indexHtml = '<div id="root"></div>';

    await mkdir(distDir, { recursive: true });
    await writeFile(path.join(distDir, "index.html"), indexHtml, "utf8");

    await prepareWebDist({ rootDir });

    await expect(readFile(path.join(distDir, "404.html"), "utf8")).resolves.toBe(indexHtml);
    await expect(readFile(path.join(distDir, ".nojekyll"), "utf8")).resolves.toBe("");
  });

  it("fails clearly when the Vite build output is missing index.html", async () => {
    const rootDir = await mkTempDir();

    await expect(prepareWebDist({ rootDir })).rejects.toMatchObject({ code: "ENOENT" });
  });

  it("allows an explicit dist directory for focused tests", async () => {
    const distDir = path.join(await mkTempDir(), "custom-dist");

    await mkdir(distDir, { recursive: true });
    await writeFile(path.join(distDir, "index.html"), "custom", "utf8");

    await prepareWebDist({ distDir });

    await expect(stat(path.join(distDir, ".nojekyll"))).resolves.toMatchObject({ size: 0 });
    await expect(readFile(path.join(distDir, "404.html"), "utf8")).resolves.toBe("custom");
  });
});

async function mkTempDir() {
  const rootDir = await mkdtemp(path.join(os.tmpdir(), "hamlet-web-dist-"));
  tempDirs.push(rootDir);
  return rootDir;
}
