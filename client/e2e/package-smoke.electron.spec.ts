import { expect, test } from "@playwright/test";
import { access, readFile } from "node:fs/promises";
import path from "node:path";
import {
  clientElectronRoot,
  firstHamletWindow,
  launchPackagedElectronApp,
} from "./electron-helpers";
import { rendererOriginPattern } from "./test-config";

interface LastUnpackedManifest {
  packagePath?: string;
  executableCandidates?: string[];
}

test("launches the last unpacked package on the configured renderer origin", async () => {
  const manifest = await readLastUnpackedManifest();
  const executablePath = await resolvePackagedExecutable(manifest);
  const launched = await launchPackagedElectronApp(executablePath);

  try {
    const page = await firstHamletWindow(launched.app);
    await expect(page).toHaveURL(rendererOriginPattern());
    await expect(
      page
        .getByRole("heading", { name: /sign in/i })
        .or(page.getByRole("navigation", { name: /channels/i })),
    ).toBeVisible({ timeout: 30_000 });
  } finally {
    await launched.close();
  }
});

async function readLastUnpackedManifest(): Promise<LastUnpackedManifest> {
  const manifestPath = path.join(clientElectronRoot, "release", "last-unpacked-package.json");
  return JSON.parse(await readFile(manifestPath, "utf8")) as LastUnpackedManifest;
}

async function resolvePackagedExecutable(manifest: LastUnpackedManifest): Promise<string> {
  const candidates = manifest.executableCandidates ?? [];
  for (const candidate of candidates) {
    try {
      await access(candidate);
      return candidate;
    } catch {
      // Try the next platform-specific executable path from the package manifest.
    }
  }

  throw new Error(
    [
      "Could not find the unpacked Hamlet Electron Alpha executable.",
      `Package path: ${manifest.packagePath ?? "unknown"}`,
      "Run npm run package:unpacked before running package smoke coverage.",
    ].join("\n"),
  );
}
