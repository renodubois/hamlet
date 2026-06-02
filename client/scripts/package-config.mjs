import { access, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";

export const ALPHA_PACKAGE_METADATA = Object.freeze({
  productName: "Hamlet Electron Alpha",
  executableName: "hamlet-electron-alpha",
  appBundleId: "com.renodubois.hamlet.electron.alpha",
  helperBundleId: "com.renodubois.hamlet.electron.alpha.helper",
  appCategoryType: "public.app-category.social-networking",
  copyright: "Copyright © 2026 Hamlet contributors",
  rendererOrigin: "http://127.0.0.1:1422",
  usageDescription: Object.freeze({
    Microphone: "Hamlet Electron Alpha uses the microphone for voice channels.",
    Camera:
      "Hamlet Electron Alpha does not capture camera video during the alpha; this string keeps macOS media prompts explicit if Chromium requests media access.",
  }),
  win32metadata: Object.freeze({
    CompanyName: "Hamlet",
    FileDescription: "Hamlet Electron Alpha local desktop client",
    ProductName: "Hamlet Electron Alpha",
    InternalName: "hamlet-electron-alpha",
    OriginalFilename: "hamlet-electron-alpha.exe",
    "requested-execution-level": "asInvoker",
  }),
});

export const DEFERRED_DISTRIBUTION_FEATURES = Object.freeze([
  "code signing",
  "macOS notarization",
  "installers",
  "auto-update",
  "public distribution",
]);

export const RELEASE_DIR_NAME = "release";
export const LAST_UNPACKED_MANIFEST = "last-unpacked-package.json";

export const REQUIRED_PACKAGE_INPUTS = Object.freeze([
  "package.json",
  "dist/index.html",
  "dist-electron/main.cjs",
  "dist-electron/preload.cjs",
  "packaging/icons/icon.icns",
  "packaging/icons/icon.ico",
  "packaging/icons/icon.png",
]);

export const PACKAGER_IGNORE_PATTERNS = Object.freeze([
  /^\/node_modules($|\/)/,
  /^\/src($|\/)/,
  /^\/electron($|\/)/,
  /^\/scripts($|\/)/,
  /^\/e2e($|\/)/,
  /^\/coverage($|\/)/,
  /^\/test-results($|\/)/,
  /^\/playwright-report($|\/)/,
  /^\/index\.html$/,
  /^\/README\.md$/,
  /^\/\.eslintignore$/,
  /^\/\.gitignore$/,
  /^\/\.oxfmtrc\.json$/,
  /^\/\.size-limit\.json$/,
  /^\/oxlint\.config\.ts$/,
  /^\/playwright(?:\.[^.]+)?\.config\.ts$/,
  /^\/playwright\.env\.ts$/,
  /^\/postcss\.config\.js$/,
  /^\/tsconfig(?:\.[^.]+)?\.json$/,
  /^\/vite\.config\.ts$/,
  /^\/vitest\.config\.ts$/,
]);

const SUPPORTED_PLATFORMS = new Set(["darwin", "linux", "win32"]);
const SUPPORTED_ARCHITECTURES = new Set(["arm64", "armv7l", "ia32", "x64"]);

export async function readPackageVersion(rootDir) {
  const packageJson = JSON.parse(await readFile(path.join(rootDir, "package.json"), "utf8"));
  if (typeof packageJson.version !== "string" || packageJson.version.trim() === "") {
    throw new Error("package.json must define a non-empty version before packaging.");
  }
  return packageJson.version;
}

export function packageTargetFromEnvironment(env = process.env) {
  return {
    platform: normalizePlatform(env.HAMLET_ELECTRON_PACKAGE_PLATFORM ?? process.platform),
    arch: normalizeArchitecture(env.HAMLET_ELECTRON_PACKAGE_ARCH ?? process.arch),
  };
}

export function createPackagerOptions({ rootDir, appVersion, platform, arch }) {
  const releaseDir = packageReleaseDir(rootDir);
  const targetPlatform = normalizePlatform(platform);
  const targetArch = normalizeArchitecture(arch);

  return {
    dir: rootDir,
    out: releaseDir,
    name: ALPHA_PACKAGE_METADATA.productName,
    ...(targetPlatform === "darwin"
      ? {}
      : { executableName: ALPHA_PACKAGE_METADATA.executableName }),
    appBundleId: ALPHA_PACKAGE_METADATA.appBundleId,
    helperBundleId: ALPHA_PACKAGE_METADATA.helperBundleId,
    appCategoryType: ALPHA_PACKAGE_METADATA.appCategoryType,
    appCopyright: ALPHA_PACKAGE_METADATA.copyright,
    appVersion,
    buildVersion: appVersion,
    platform: targetPlatform,
    arch: targetArch,
    icon: packageIconPath(rootDir, targetPlatform),
    usageDescription: { ...ALPHA_PACKAGE_METADATA.usageDescription },
    win32metadata: { ...ALPHA_PACKAGE_METADATA.win32metadata },
    darwinDarkModeSupport: false,
    overwrite: true,
    asar: false,
    prune: false,
    junk: true,
    quiet: true,
    ignore: [...PACKAGER_IGNORE_PATTERNS],
  };
}

export async function assertPackagePrerequisites(rootDir) {
  const missing = [];

  for (const relativePath of REQUIRED_PACKAGE_INPUTS) {
    const absolutePath = path.join(rootDir, relativePath);
    try {
      await access(absolutePath);
    } catch {
      missing.push(relativePath);
    }
  }

  if (missing.length > 0) {
    throw new Error(
      [
        "Cannot build the Hamlet Electron Alpha unpacked package because required inputs are missing:",
        ...missing.map((relativePath) => `  - ${relativePath}`),
        "Run npm run build before packaging and keep packaging/icons populated.",
      ].join("\n"),
    );
  }
}

export function packageIconPath(rootDir, platform) {
  const targetPlatform = normalizePlatform(platform);
  if (targetPlatform === "darwin") return path.join(rootDir, "packaging", "icons", "icon.icns");
  if (targetPlatform === "win32") return path.join(rootDir, "packaging", "icons", "icon.ico");
  return path.join(rootDir, "packaging", "icons", "icon.png");
}

export function packageReleaseDir(rootDir) {
  return path.join(rootDir, RELEASE_DIR_NAME);
}

export function lastUnpackedManifestPath(rootDir) {
  return path.join(packageReleaseDir(rootDir), LAST_UNPACKED_MANIFEST);
}

export function finalPackageDirectory(rootDir, platform, arch) {
  return path.join(
    packageReleaseDir(rootDir),
    `${ALPHA_PACKAGE_METADATA.productName}-${normalizePlatform(platform)}-${normalizeArchitecture(arch)}`,
  );
}

export async function writeLastUnpackedManifest({
  rootDir,
  packagePath,
  platform,
  arch,
  appVersion,
}) {
  const manifest = createLastUnpackedManifest({ packagePath, platform, arch, appVersion });
  await mkdir(packageReleaseDir(rootDir), { recursive: true });
  await writeFile(lastUnpackedManifestPath(rootDir), `${JSON.stringify(manifest, null, 2)}\n`);
  return manifest;
}

export async function readLastUnpackedManifest(rootDir) {
  return JSON.parse(await readFile(lastUnpackedManifestPath(rootDir), "utf8"));
}

export function createLastUnpackedManifest({ packagePath, platform, arch, appVersion }) {
  const targetPlatform = normalizePlatform(platform);
  const targetArch = normalizeArchitecture(arch);

  return {
    productName: ALPHA_PACKAGE_METADATA.productName,
    executableName: ALPHA_PACKAGE_METADATA.executableName,
    appBundleId: ALPHA_PACKAGE_METADATA.appBundleId,
    appVersion,
    rendererOrigin: ALPHA_PACKAGE_METADATA.rendererOrigin,
    platform: targetPlatform,
    arch: targetArch,
    packagePath,
    executableCandidates: packagedExecutableCandidates(packagePath, targetPlatform),
    deferredDistributionFeatures: [...DEFERRED_DISTRIBUTION_FEATURES],
  };
}

export function packagedExecutableCandidates(packagePath, platform) {
  const targetPlatform = normalizePlatform(platform);
  const executableName = ALPHA_PACKAGE_METADATA.executableName;
  const productName = ALPHA_PACKAGE_METADATA.productName;

  if (targetPlatform === "darwin") {
    return [
      path.join(packagePath, `${productName}.app`, "Contents", "MacOS", productName),
      path.join(packagePath, `${productName}.app`, "Contents", "MacOS", executableName),
    ];
  }

  if (targetPlatform === "win32") {
    return [
      path.join(packagePath, `${executableName}.exe`),
      path.join(packagePath, `${productName}.exe`),
    ];
  }

  return [path.join(packagePath, executableName), path.join(packagePath, productName)];
}

export async function resolvePackagedExecutable(manifest) {
  const candidates = Array.isArray(manifest.executableCandidates)
    ? manifest.executableCandidates
    : packagedExecutableCandidates(manifest.packagePath, manifest.platform);

  for (const candidate of candidates) {
    try {
      await access(candidate);
      return candidate;
    } catch {
      // Try the next platform-specific executable candidate.
    }
  }

  throw new Error(
    [
      `Could not find the unpacked ${ALPHA_PACKAGE_METADATA.productName} executable.`,
      `Package path: ${manifest.packagePath ?? "unknown"}`,
      "Run npm run package:unpacked before launching the packaged app.",
    ].join("\n"),
  );
}

export function packagedLaunchEnvironment(env = process.env) {
  const launchEnv = { ...env };
  delete launchEnv.HAMLET_RENDERER_URL;
  return launchEnv;
}

export function formatDeferredDistributionMessage(commandName = "package:full") {
  return [
    `${commandName} is intentionally deferred for the Electron alpha.`,
    `Use npm run package:unpacked for local unpacked packages that can be launched side by side.`,
    `Deferred release work: ${DEFERRED_DISTRIBUTION_FEATURES.join(", ")}.`,
  ].join("\n");
}

function normalizePlatform(platform) {
  if (typeof platform !== "string" || !SUPPORTED_PLATFORMS.has(platform)) {
    throw new Error(
      `Unsupported Electron package platform "${String(platform)}". Expected one of: ${[
        ...SUPPORTED_PLATFORMS,
      ].join(", ")}.`,
    );
  }
  return platform;
}

function normalizeArchitecture(arch) {
  const normalized = arch === "arm" ? "armv7l" : arch;
  if (typeof normalized !== "string" || !SUPPORTED_ARCHITECTURES.has(normalized)) {
    throw new Error(
      `Unsupported Electron package architecture "${String(arch)}". Expected one of: ${[
        ...SUPPORTED_ARCHITECTURES,
      ].join(", ")}.`,
    );
  }
  return normalized;
}
