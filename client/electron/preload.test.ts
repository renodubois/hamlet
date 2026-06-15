import { readFile } from "node:fs/promises";
import path from "node:path";
import { describe, expect, it } from "vitest";

describe("Electron preload boundary", () => {
  it("does not expose raw IPC, desktop capture, or filesystem capabilities", async () => {
    const source = await readFile(path.join(process.cwd(), "electron", "preload.ts"), "utf8");

    expect(source).not.toMatch(/from\s+["']electron["']|require\(["']electron["']\)/);
    expect(source).not.toMatch(/from\s+["']node:fs|require\(["']node:fs/);
    expect(source).not.toMatch(/contextBridge\./);
  });
});
