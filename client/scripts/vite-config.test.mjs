import { afterEach, describe, expect, it } from "vitest";
import viteConfig from "../vite.config.ts";

const originalSourceMapsValue = process.env.HAMLET_BUILD_SOURCE_MAPS;

afterEach(() => {
  if (originalSourceMapsValue === undefined) {
    delete process.env.HAMLET_BUILD_SOURCE_MAPS;
  } else {
    process.env.HAMLET_BUILD_SOURCE_MAPS = originalSourceMapsValue;
  }
});

describe("Vite deployment source maps", () => {
  it("keeps source maps disabled by default", () => {
    delete process.env.HAMLET_BUILD_SOURCE_MAPS;

    expect(resolveBuildSourcemap()).toBe(false);
  });

  it("enables source maps when HAMLET_BUILD_SOURCE_MAPS is true", () => {
    process.env.HAMLET_BUILD_SOURCE_MAPS = "true";

    expect(resolveBuildSourcemap()).toBe(true);
  });
});

function resolveBuildSourcemap() {
  const config = viteConfig({ mode: "production", command: "build" });
  return config.build?.sourcemap;
}
