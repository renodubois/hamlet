import { describe, expect, it, vi } from "vitest";
import {
  initializeRendererSentry,
  normalizeSentryDsn,
  RENDERER_SENTRY_PRIVACY_OPTIONS,
} from "./sentry";

describe("renderer Sentry initialization", () => {
  it("normalizes blank DSNs as disabled", () => {
    expect(normalizeSentryDsn(undefined)).toBeUndefined();
    expect(normalizeSentryDsn("")).toBeUndefined();
    expect(normalizeSentryDsn("   ")).toBeUndefined();
    expect(normalizeSentryDsn("  https://example.invalid/1  ")).toBe("https://example.invalid/1");
  });

  it("does not initialize Sentry when the DSN is missing or blank", () => {
    const sentry = { init: vi.fn() };

    expect(initializeRendererSentry({ dsn: undefined, sentry })).toBe(false);
    expect(initializeRendererSentry({ dsn: "", sentry })).toBe(false);
    expect(initializeRendererSentry({ dsn: "   ", sentry })).toBe(false);

    expect(sentry.init).not.toHaveBeenCalled();
  });

  it("initializes Sentry with the configured DSN and conservative privacy options", () => {
    const sentry = { init: vi.fn() };

    expect(initializeRendererSentry({ dsn: "  https://public@example.invalid/1  ", sentry })).toBe(
      true,
    );

    expect(sentry.init).toHaveBeenCalledTimes(1);
    expect(sentry.init).toHaveBeenCalledWith({
      dsn: "https://public@example.invalid/1",
      ...RENDERER_SENTRY_PRIVACY_OPTIONS,
    });
    expect(RENDERER_SENTRY_PRIVACY_OPTIONS.dataCollection).toMatchObject({
      userInfo: false,
      httpBodies: [],
    });
  });
});
