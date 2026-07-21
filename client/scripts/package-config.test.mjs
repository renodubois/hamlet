import { mkdir, mkdtemp, rm, unlink, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  ALPHA_PACKAGE_METADATA,
  DEFERRED_DISTRIBUTION_FEATURES,
  PACKAGER_IGNORE_PATTERNS,
  assertPackagePrerequisites,
  createPackagerOptions,
  createLastUnpackedManifest,
  finalPackageDirectory,
  formatDeferredDistributionMessage,
  packagedLaunchEnvironment,
  packageTargetFromEnvironment,
  readLastUnpackedManifest,
  writeLastUnpackedManifest,
} from "./package-config.mjs";

const tempDirs = [];

afterEach(async () => {
  while (tempDirs.length > 0) {
    const dir = tempDirs.pop();
    if (dir !== undefined) await rm(dir, { recursive: true, force: true });
  }
});

describe("Electron alpha package metadata", () => {
  it("uses a distinct alpha identity and platform metadata", () => {
    const options = createPackagerOptions({
      rootDir: "/repo/client",
      appVersion: "0.1.0",
      platform: "darwin",
      arch: "arm64",
    });

    expect(ALPHA_PACKAGE_METADATA.productName).toBe("Hamlet Electron Alpha");
    expect(ALPHA_PACKAGE_METADATA.executableName).toBe("hamlet-electron-alpha");
    expect(ALPHA_PACKAGE_METADATA.appBundleId).toBe("com.renodubois.hamlet.electron.alpha");
    expect(ALPHA_PACKAGE_METADATA.appBundleId).not.toBe("com.renodubois.hamlet");
    expect(options).toMatchObject({
      name: "Hamlet Electron Alpha",
      appBundleId: "com.renodubois.hamlet.electron.alpha",
      helperBundleId: "com.renodubois.hamlet.electron.alpha.helper",
      appCategoryType: "public.app-category.social-networking",
      appVersion: "0.1.0",
      buildVersion: "0.1.0",
      platform: "darwin",
      arch: "arm64",
      asar: false,
      prune: false,
      quiet: true,
    });
    expect(options.executableName).toBeUndefined();
    expect(options.icon).toBe(path.join("/repo/client", "packaging", "icons", "icon.icns"));
    expect(
      createPackagerOptions({
        rootDir: "/repo/client",
        appVersion: "0.1.0",
        platform: "linux",
        arch: "x64",
      }).executableName,
    ).toBe("hamlet-electron-alpha");
    expect(options.usageDescription.Microphone).toContain("voice channels");
    expect(options.usageDescription.Camera).toContain("explicit local previews");
    expect(options.usageDescription.Camera).toContain("voice/video camera features");
    expect(options.win32metadata.ProductName).toBe("Hamlet Electron Alpha");
    expect(options.darwinDarkModeSupport).toBe(false);
  });

  it("leaves signing, notarization, installers, auto-update, and public distribution deferred", () => {
    const options = createPackagerOptions({
      rootDir: "/repo/client",
      appVersion: "0.1.0",
      platform: "win32",
      arch: "x64",
    });
    const deferredMessage = formatDeferredDistributionMessage("package:full");

    expect(options.osxSign).toBeUndefined();
    expect(options.osxNotarize).toBeUndefined();
    expect(options.windowsSign).toBeUndefined();
    expect(DEFERRED_DISTRIBUTION_FEATURES).toEqual([
      "code signing",
      "macOS notarization",
      "installers",
      "auto-update",
      "public distribution",
    ]);
    expect(deferredMessage).toContain("intentionally deferred");
    expect(deferredMessage).toContain("pnpm run package:unpacked");
    for (const feature of DEFERRED_DISTRIBUTION_FEATURES) {
      expect(deferredMessage).toContain(feature);
    }
  });
});

describe("Electron alpha package inputs", () => {
  it("requires renderer, main, preload, and icon outputs before packaging", async () => {
    const rootDir = await createPackageFixture();

    await expect(assertPackagePrerequisites(rootDir)).resolves.toBeUndefined();

    await unlink(path.join(rootDir, "dist-electron", "preload.cjs"));
    await expect(assertPackagePrerequisites(rootDir)).rejects.toThrow("dist-electron/preload.cjs");
  });

  it("keeps the package copy scoped to runtime outputs and packaging assets", () => {
    expect(isIgnored("src/App.tsx")).toBe(true);
    expect(isIgnored("electron/main.ts")).toBe(true);
    expect(isIgnored("scripts/package-unpacked.mjs")).toBe(true);
    expect(isIgnored("e2e/smoke.spec.ts")).toBe(true);
    expect(isIgnored("node_modules/electron/index.js")).toBe(true);
    expect(isIgnored("index.html")).toBe(true);
    expect(isIgnored("README.md")).toBe(true);
    expect(isIgnored("oxlint.config.json")).toBe(true);
    expect(isIgnored("playwright.config.ts")).toBe(true);
    expect(isIgnored("playwright.electron.config.ts")).toBe(true);
    expect(isIgnored("playwright.package.config.ts")).toBe(true);
    expect(isIgnored("playwright.env.ts")).toBe(true);

    expect(isIgnored("package.json")).toBe(false);
    expect(isIgnored("dist/index.html")).toBe(false);
    expect(isIgnored("dist/assets/app.js")).toBe(false);
    expect(isIgnored("dist-electron/main.cjs")).toBe(false);
    expect(isIgnored("packaging/icons/icon.png")).toBe(false);
  });
});

describe("Electron alpha package target and launch manifest", () => {
  it("normalizes local package targets", () => {
    expect(
      packageTargetFromEnvironment({
        HAMLET_ELECTRON_PACKAGE_PLATFORM: "linux",
        HAMLET_ELECTRON_PACKAGE_ARCH: "arm",
      }),
    ).toEqual({ platform: "linux", arch: "armv7l" });

    expect(() =>
      packageTargetFromEnvironment({
        HAMLET_ELECTRON_PACKAGE_PLATFORM: "freebsd",
        HAMLET_ELECTRON_PACKAGE_ARCH: "x64",
      }),
    ).toThrow("Unsupported Electron package platform");
  });

  it("records the last unpacked package and strips dev renderer overrides when launching it", async () => {
    const rootDir = await createPackageFixture();
    const packagePath = finalPackageDirectory(rootDir, "darwin", "arm64");

    const manifest = await writeLastUnpackedManifest({
      rootDir,
      packagePath,
      platform: "darwin",
      arch: "arm64",
      appVersion: "0.1.0",
    });

    await expect(readLastUnpackedManifest(rootDir)).resolves.toEqual(manifest);
    expect(manifest).toMatchObject({
      productName: "Hamlet Electron Alpha",
      appBundleId: "com.renodubois.hamlet.electron.alpha",
      appVersion: "0.1.0",
      rendererOrigin: "http://127.0.0.1:1422",
      platform: "darwin",
      arch: "arm64",
      packagePath,
    });
    expect(manifest.executableCandidates).toEqual([
      path.join(
        packagePath,
        "Hamlet Electron Alpha.app",
        "Contents",
        "MacOS",
        "Hamlet Electron Alpha",
      ),
      path.join(
        packagePath,
        "Hamlet Electron Alpha.app",
        "Contents",
        "MacOS",
        "hamlet-electron-alpha",
      ),
    ]);
    expect(
      packagedLaunchEnvironment({ HAMLET_RENDERER_URL: "http://127.0.0.1:1422", KEEP: "yes" }),
    ).toEqual({
      KEEP: "yes",
    });
  });

  it("creates platform-specific executable candidates", () => {
    expect(
      createLastUnpackedManifest({
        packagePath: "/pkg",
        platform: "linux",
        arch: "x64",
        appVersion: "0.1.0",
      }).executableCandidates,
    ).toEqual([
      path.join("/pkg", "hamlet-electron-alpha"),
      path.join("/pkg", "Hamlet Electron Alpha"),
    ]);

    expect(
      createLastUnpackedManifest({
        packagePath: "C:\\pkg",
        platform: "win32",
        arch: "x64",
        appVersion: "0.1.0",
      }).executableCandidates,
    ).toEqual([
      path.join("C:\\pkg", "hamlet-electron-alpha.exe"),
      path.join("C:\\pkg", "Hamlet Electron Alpha.exe"),
    ]);
  });
});

async function createPackageFixture() {
  const rootDir = await mkTempDir();
  await Promise.all([
    mkdir(path.join(rootDir, "dist"), { recursive: true }),
    mkdir(path.join(rootDir, "dist-electron"), { recursive: true }),
    mkdir(path.join(rootDir, "packaging", "icons"), { recursive: true }),
  ]);
  await Promise.all([
    writeFile(path.join(rootDir, "package.json"), JSON.stringify({ version: "0.1.0" })),
    writeFile(path.join(rootDir, "dist", "index.html"), '<div id="root"></div>'),
    writeFile(path.join(rootDir, "dist-electron", "main.cjs"), "module.exports = {};"),
    writeFile(path.join(rootDir, "dist-electron", "preload.cjs"), "module.exports = {};"),
    writeFile(path.join(rootDir, "packaging", "icons", "icon.icns"), "icns"),
    writeFile(path.join(rootDir, "packaging", "icons", "icon.ico"), "ico"),
    writeFile(path.join(rootDir, "packaging", "icons", "icon.png"), "png"),
  ]);
  return rootDir;
}

async function mkTempDir() {
  const rootDir = await mkdtemp(path.join(os.tmpdir(), "hamlet-electron-package-"));
  tempDirs.push(rootDir);
  return rootDir;
}

function isIgnored(relativePath) {
  const packagePath = `/${relativePath}`;
  return PACKAGER_IGNORE_PATTERNS.some((pattern) => pattern.test(packagePath));
}
