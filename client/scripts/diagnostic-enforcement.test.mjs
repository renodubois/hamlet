import { spawnSync } from "node:child_process";
import path from "node:path";
import { beforeAll, describe, expect, it } from "vitest";

const SELF_TEST_ENV = "HAMLET_DIAGNOSTIC_ENFORCEMENT_SELF_TEST";
const SELF_TEST_DIAGNOSTIC = "diagnostic emitted from beforeAll";

if (process.env[SELF_TEST_ENV] === "before-all") {
  beforeAll(() => {
    console.warn(SELF_TEST_DIAGNOSTIC);
  });

  describe("diagnostic enforcement subprocess fixture", () => {
    it("would otherwise pass", () => {
      expect(true).toBe(true);
    });
  });
} else {
  describe("diagnostic enforcement", () => {
    it("fails a test process for an uncaptured beforeAll diagnostic", () => {
      const result = spawnSync(
        process.execPath,
        [
          path.join("node_modules", "vitest", "vitest.mjs"),
          "run",
          "scripts/diagnostic-enforcement.test.mjs",
        ],
        {
          cwd: process.cwd(),
          encoding: "utf8",
          env: { ...process.env, [SELF_TEST_ENV]: "before-all" },
          timeout: 30_000,
        },
      );
      const output = `${result.stdout ?? ""}\n${result.stderr ?? ""}`;

      expect(result.error).toBeUndefined();
      expect(result.status).not.toBe(0);
      expect(output).toContain("Unexpected console diagnostics");
      expect(output).toContain(`console.warn: ${SELF_TEST_DIAGNOSTIC}`);
    });
  });
}
