import { afterEach, describe, expect, it } from "vitest";
import viteConfig from "../vite.config.ts";

const originalSourceMapsValue = process.env.HAMLET_BUILD_SOURCE_MAPS;
const originalBasePathValue = process.env.VITE_HAMLET_BASE_PATH;

afterEach(() => {
  restoreEnvironmentVariable("HAMLET_BUILD_SOURCE_MAPS", originalSourceMapsValue);
  restoreEnvironmentVariable("VITE_HAMLET_BASE_PATH", originalBasePathValue);
});

describe("Vite deployment source maps", () => {
  it("keeps source maps disabled by default", () => {
    delete process.env.HAMLET_BUILD_SOURCE_MAPS;

    expect(resolveConfig().build?.sourcemap).toBe(false);
  });

  it("enables source maps when HAMLET_BUILD_SOURCE_MAPS is true", () => {
    process.env.HAMLET_BUILD_SOURCE_MAPS = "true";

    expect(resolveConfig().build?.sourcemap).toBe(true);
  });
});

describe("Vite deployment base path", () => {
  it.each([undefined, "", "/"])("uses the site root for %j", (basePath) => {
    setOptionalEnvironmentVariable("VITE_HAMLET_BASE_PATH", basePath);

    expect(resolveConfig().base).toBe("/");
  });

  it.each([
    ["hamlet", "/hamlet/"],
    ["/hamlet", "/hamlet/"],
    ["/hamlet/", "/hamlet/"],
    ["  /org/hamlet///  ", "/org/hamlet/"],
  ])("normalizes a project-site base path %j", (basePath, expected) => {
    process.env.VITE_HAMLET_BASE_PATH = basePath;

    expect(resolveConfig().base).toBe(expected);
  });
});

function resolveConfig() {
  return viteConfig({ mode: "production", command: "build" });
}

function restoreEnvironmentVariable(key, value) {
  setOptionalEnvironmentVariable(key, value);
}

function setOptionalEnvironmentVariable(key, value) {
  if (value === undefined) {
    delete process.env[key];
  } else {
    process.env[key] = value;
  }
}
