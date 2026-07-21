import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { auditNativeReact } from "./check-native-react.mjs";

const tempDirectories = [];

async function fixture({
  dependencies = {},
  source = "export const value = 1;",
  lock = "lockfileVersion: '9.0'\n",
} = {}) {
  const root = await mkdtemp(path.join(os.tmpdir(), "hamlet-native-react-"));
  tempDirectories.push(root);
  await mkdir(path.join(root, "src"));
  await writeFile(
    path.join(root, "package.json"),
    JSON.stringify({
      dependencies: { react: "19.0.0", "react-dom": "19.0.0", ...dependencies },
      devDependencies: { "@vitejs/plugin-react": "6.0.0" },
    }),
  );
  await writeFile(path.join(root, "pnpm-lock.yaml"), lock);
  await writeFile(path.join(root, "src", "app.tsx"), source);
  return root;
}

afterEach(async () => {
  await Promise.all(
    tempDirectories.splice(0).map((directory) => rm(directory, { recursive: true, force: true })),
  );
});

describe("native React audit", () => {
  it("accepts a React-only client and explicitly historical Solid documentation", async () => {
    const root = await fixture();
    await writeFile(
      path.join(root, "README.md"),
      "Historical migration note: the renderer was implemented with SolidJS before React.\n",
    );
    expect(await auditNativeReact(root)).toEqual([]);
  });

  it("rejects the compatibility file, imports, identifiers, and JSX helpers", async () => {
    const identifiers = [
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
    const helpers = ["If", "List", "Choose", "Case"];
    const root = await fixture({
      source: [
        'import "./hooks/react-state";',
        ...identifiers.map((identifier) => `void ${identifier};`),
        ...helpers.map((helper, index) =>
          index === 0
            ? `const ${helper}Node = <${helper}/>;`
            : `const ${helper}Node = <${helper}>value</${helper}>;`,
        ),
      ].join("\n"),
    });
    await mkdir(path.join(root, "src", "hooks"));
    await writeFile(path.join(root, "src", "hooks", "react-state.tsx"), "export {};\n");

    const findings = await auditNativeReact(root);
    expect(findings).toContain("src/hooks/react-state.tsx still exists");
    expect(findings.some((finding) => finding.includes("references react-state"))).toBe(true);
    for (const identifier of identifiers) {
      expect(findings.some((finding) => finding.includes(`uses ${identifier}`))).toBe(true);
    }
    for (const helper of helpers) {
      expect(findings.some((finding) => finding.includes(`uses <${helper}>`))).toBe(true);
    }
  });

  it("rejects compatibility code outside src", async () => {
    const root = await fixture();
    for (const [directory, filename, source] of [
      ["electron", "main.ts", "void useSignalState;\n"],
      ["e2e", "smoke.spec.ts", "const node = <Choose />;\n"],
      ["scripts", "build.mjs", "void registerCleanup;\n"],
    ]) {
      await mkdir(path.join(root, directory));
      await writeFile(path.join(root, directory, filename), source);
    }
    await writeFile(path.join(root, "vite.config.ts"), "void useComputedValue;\n");

    const findings = await auditNativeReact(root);
    for (const expectedPath of [
      "electron/main.ts",
      "e2e/smoke.spec.ts",
      "scripts/build.mjs",
      "vite.config.ts",
    ]) {
      expect(findings.some((finding) => finding.startsWith(expectedPath))).toBe(true);
    }
  });

  it("rejects strong current-stack Solid phrasing throughout active docs", async () => {
    const root = await fixture();
    await mkdir(path.join(root, "docs", "plans"), { recursive: true });
    const examples = [
      ["README.md", "The current renderer uses SolidJS today.\n"],
      ["docs/architecture.md", "Frontend framework: SolidJS\n"],
      ["docs/operations.md", "Hamlet client stack = SolidJS\n"],
      ["docs/plans/current.md", "The active frontend is built with SolidJS.\n"],
    ];
    for (const [filename, text] of examples) {
      await mkdir(path.dirname(path.join(root, filename)), { recursive: true });
      await writeFile(path.join(root, filename), text);
    }

    const findings = await auditNativeReact(root);
    for (const [filename] of examples) {
      expect(
        findings.some(
          (finding) =>
            finding.startsWith(`${filename}:1`) &&
            finding.endsWith("describes the active client as Solid"),
        ),
      ).toBe(true);
    }
  });

  it("allows explicitly historical migration PRD text", async () => {
    const root = await fixture();
    await mkdir(path.join(root, "docs"));
    await writeFile(
      path.join(root, "docs", "migration-prd.md"),
      [
        "# Migration PRD",
        "",
        "> **Historical migration document.** This does not describe the active client stack.",
        "",
        "Frontend framework: SolidJS",
      ].join("\n"),
    );

    expect(await auditNativeReact(root)).toEqual([]);
  });

  it("reports forbidden dependencies and source imports", async () => {
    const solid = ["solid", "js"].join("-");
    const root = await fixture({
      dependencies: { [solid]: "1.9.0" },
      source: `import { createSignal } from "${solid}";`,
    });

    const findings = await auditNativeReact(root);
    expect(findings.some((finding) => finding.includes(`package.json:1 references ${solid}`))).toBe(
      true,
    );
    expect(findings.some((finding) => finding.includes(`src/app.tsx:1 references ${solid}`))).toBe(
      true,
    );
  });

  it("audits the lockfile and required React tooling", async () => {
    const root = await fixture({
      lock: `packages:\n  ${["vite-plugin", "solid"].join("-")}@2.11.0: {}\n`,
    });
    const manifest = JSON.parse(
      await (await import("node:fs/promises")).readFile(path.join(root, "package.json"), "utf8"),
    );
    delete manifest.devDependencies["@vitejs/plugin-react"];
    await writeFile(path.join(root, "package.json"), JSON.stringify(manifest));

    const findings = await auditNativeReact(root);
    expect(findings).toContain("package.json is missing @vitejs/plugin-react");
    expect(findings.some((finding) => finding.startsWith("pnpm-lock.yaml:2 references"))).toBe(
      true,
    );
  });
});
