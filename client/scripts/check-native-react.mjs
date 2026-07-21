#!/usr/bin/env node

import { access, readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const FORBIDDEN_PACKAGES = [
  "solid-js",
  "@solidjs/",
  "@solidjs/router",
  "@solidjs/testing-library",
  "@testing-library/solid",
  "@sentry/solid",
  "solid-icons",
  "vite-plugin-solid",
];
const AUDITED_EXTENSIONS = new Set([
  ".js",
  ".jsx",
  ".mjs",
  ".cjs",
  ".ts",
  ".tsx",
  ".json",
  ".yaml",
  ".yml",
]);
const IGNORED_DIRECTORIES = new Set([
  ".git",
  "coverage",
  "dist",
  "dist-electron",
  "node_modules",
  "release",
  "storybook-static",
  "test-results",
]);
const CHECKER_FILE = /^check-native-react(?:\.test)?\.mjs$/;
const COMPATIBILITY_IDENTIFIERS = [
  "useSignalState",
  "useComputedValue",
  "useCallableResource",
  "useAfterRenderEffect",
  "useMountEffect",
  "registerCleanup",
  "useStaticSignalRerender",
  "useStoreState",
  "preserveIdentity",
  "CallableResource",
];
const COMPATIBILITY_JSX_HELPERS = ["If", "List", "Choose", "Case"];
const ACTIVE_DOCUMENTATION = ["README.md", "../AGENTS.md", "../server/CLAUDE.md"];
const ACTIVE_DOCUMENTATION_DIRECTORIES = ["docs", "../docs"];

function lineNumber(text, index) {
  return text.slice(0, index).split("\n").length;
}

function packagePattern(packageName) {
  const escapedName = packageName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const suffix = packageName.endsWith("/")
    ? "[A-Za-z0-9_.-]+"
    : "(?:[/@][A-Za-z0-9_.@/-]*|[\\s,}\"'])";
  return new RegExp(`(?:^|[^A-Za-z0-9_.-])${escapedName}${suffix}`, "gm");
}

function inspectText(relativePath, text, findings) {
  for (const packageName of FORBIDDEN_PACKAGES) {
    for (const match of text.matchAll(packagePattern(packageName))) {
      findings.push(`${relativePath}:${lineNumber(text, match.index)} references ${packageName}`);
    }
  }

  const solidJsxPattern = /jsxImportSource\s*["':= ]+solid-js|types\s*=\s*["']solid-js/g;
  for (const match of text.matchAll(solidJsxPattern)) {
    findings.push(`${relativePath}:${lineNumber(text, match.index)} configures Solid JSX types`);
  }
}

function inspectActiveCode(relativePath, text, findings) {
  const reactStatePattern = /(?:hooks\/react-state|react-state(?:\.[cm]?[jt]sx?)?)/g;
  for (const match of text.matchAll(reactStatePattern)) {
    findings.push(`${relativePath}:${lineNumber(text, match.index)} references react-state`);
  }

  for (const identifier of COMPATIBILITY_IDENTIFIERS) {
    const pattern = new RegExp(`\\b${identifier}\\b`, "g");
    for (const match of text.matchAll(pattern)) {
      findings.push(`${relativePath}:${lineNumber(text, match.index)} uses ${identifier}`);
    }
  }
  for (const helper of COMPATIBILITY_JSX_HELPERS) {
    const pattern = new RegExp(`<${helper}(?:\\s|/?>|\\.)`, "g");
    for (const match of text.matchAll(pattern)) {
      findings.push(`${relativePath}:${lineNumber(text, match.index)} uses <${helper}>`);
    }
  }
}

function inspectActiveDocumentation(relativePath, text, findings) {
  // A migration PRD may retain old-stack prose when it explicitly declares the
  // whole document historical and says that it does not describe active code.
  const preamble = text.split("\n").slice(0, 12).join("\n");
  const explicitlyHistorical =
    /\*\*Historical (?:migration )?document\.\*\*/i.test(preamble) &&
    /(?:do not|does not) describe the active (?:client )?stack/i.test(preamble);
  if (explicitlyHistorical) return;

  const currentSolidPatterns = [
    /\b(?:current|active|existing|today(?:'s)?)\s+(?:Hamlet\s+)?(?:client|renderer|frontend)(?:\s+(?:framework|stack|technology))?\s*(?::|=|-|\bis\b|\buses\b|\bruns\b|\bremains\b|\bis built with\b|\bis written (?:in|with)\b)[^.\n]{0,100}\bSolid(?:JS)?\b/gi,
    /\b(?:Hamlet\s+)?(?:client|renderer|frontend)(?:\s+(?:framework|stack|technology))?\s*(?::|=|-)\s*Solid(?:JS)?\b/gi,
    /\b(?:Hamlet\s+)?(?:client|renderer|frontend)\s+(?:uses|runs|remains|is built with|is written (?:in|with))[^.\n]{0,100}\bSolid(?:JS)?\b/gi,
  ];
  for (const pattern of currentSolidPatterns) {
    for (const match of text.matchAll(pattern)) {
      findings.push(
        `${relativePath}:${lineNumber(text, match.index)} describes the active client as Solid`,
      );
    }
  }
}

async function walkSource(root, directory, findings) {
  const entries = await readdir(directory, { withFileTypes: true });
  for (const entry of entries) {
    if (entry.isDirectory() && IGNORED_DIRECTORIES.has(entry.name)) continue;

    const absolutePath = path.join(directory, entry.name);
    const relativePath = path.relative(root, absolutePath);
    if (entry.isDirectory()) {
      await walkSource(root, absolutePath, findings);
      continue;
    }
    if (!entry.isFile() || !AUDITED_EXTENSIONS.has(path.extname(entry.name))) continue;
    if (path.dirname(relativePath) === "scripts" && CHECKER_FILE.test(entry.name)) continue;

    if (/(?:^|[._-])solid(?:[._-]|$)/i.test(entry.name)) {
      findings.push(`${relativePath} has a Solid-specific filename`);
    }
    const text = await readFile(absolutePath, "utf8");
    inspectText(relativePath, text, findings);
    inspectActiveCode(relativePath, text, findings);
  }
}

async function walkDocumentation(root, directory, findings) {
  let entries;
  try {
    entries = await readdir(directory, { withFileTypes: true });
  } catch (error) {
    if (error?.code === "ENOENT") return;
    throw error;
  }

  for (const entry of entries) {
    if (entry.isDirectory() && IGNORED_DIRECTORIES.has(entry.name)) continue;
    const absolutePath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      await walkDocumentation(root, absolutePath, findings);
    } else if (entry.isFile() && path.extname(entry.name).toLowerCase() === ".md") {
      inspectActiveDocumentation(
        path.relative(root, absolutePath),
        await readFile(absolutePath, "utf8"),
        findings,
      );
    }
  }
}

export async function auditNativeReact(root) {
  const findings = [];
  const compatibilityPath = path.join(root, "src", "hooks", "react-state.tsx");
  try {
    await access(compatibilityPath);
    findings.push("src/hooks/react-state.tsx still exists");
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }

  const packagePath = path.join(root, "package.json");
  const packageText = await readFile(packagePath, "utf8");
  const manifest = JSON.parse(packageText);
  const allDependencies = { ...manifest.dependencies, ...manifest.devDependencies };

  for (const required of ["react", "react-dom"]) {
    if (!allDependencies[required]) findings.push(`package.json is missing ${required}`);
  }
  if (!allDependencies["@vitejs/plugin-react"]) {
    findings.push("package.json is missing @vitejs/plugin-react");
  }
  inspectText("package.json", packageText, findings);

  const lockPath = path.join(root, "pnpm-lock.yaml");
  try {
    inspectText("pnpm-lock.yaml", await readFile(lockPath, "utf8"), findings);
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
    findings.push("pnpm-lock.yaml is missing");
  }

  await walkSource(root, root, findings);

  for (const documentationPath of ACTIVE_DOCUMENTATION) {
    const absolutePath = path.resolve(root, documentationPath);
    try {
      inspectActiveDocumentation(
        path.relative(root, absolutePath),
        await readFile(absolutePath, "utf8"),
        findings,
      );
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
    }
  }
  for (const documentationDirectory of ACTIVE_DOCUMENTATION_DIRECTORIES) {
    await walkDocumentation(root, path.resolve(root, documentationDirectory), findings);
  }
  return [...new Set(findings)].sort();
}

async function main() {
  const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
  const defaultRoot = path.resolve(scriptDirectory, "..");
  const rootArgument = process.argv.indexOf("--root");
  const root =
    rootArgument === -1 ? defaultRoot : path.resolve(process.argv[rootArgument + 1] ?? "");
  if (rootArgument !== -1 && !process.argv[rootArgument + 1]) {
    console.error("check:native-react: --root requires a directory");
    process.exitCode = 2;
    return;
  }

  const findings = await auditNativeReact(root);
  if (findings.length > 0) {
    console.error(
      "check:native-react failed. Remove active Solid dependencies, tooling, and source references:",
    );
    for (const finding of findings) console.error(`  - ${finding}`);
    process.exitCode = 1;
    return;
  }
  console.log(
    "check:native-react passed: the client uses native React tooling with no active Solid artifacts.",
  );
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
