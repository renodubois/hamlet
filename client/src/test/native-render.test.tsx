import { useEffect } from "react";
import { describe, expect, it } from "vitest";
import { captureExpectedConsoleDiagnostics, captureReactDiagnostics } from "./setup";
import { renderNative } from "./render";

describe("native React test guardrails", () => {
  it("replays effect setup and cleanup in Strict Mode", () => {
    const lifecycle: string[] = [];

    function EffectProbe() {
      useEffect(() => {
        lifecycle.push("setup");
        return () => {
          lifecycle.push("cleanup");
        };
      }, []);
      return null;
    }

    const view = renderNative(<EffectProbe />);
    expect(lifecycle).toEqual(["setup", "cleanup", "setup"]);

    view.unmount();
    expect(lifecycle).toEqual(["setup", "cleanup", "setup", "cleanup"]);
  });

  it("explicitly captures and consumes expected React errors", () => {
    const capture = captureReactDiagnostics();
    try {
      console.error("A test update was not wrapped in act");
      expect(capture.diagnostics).toEqual([["A test update was not wrapped in act"]]);
    } finally {
      capture.stop();
    }
  });

  it("explicitly captures and consumes expected warnings", () => {
    const capture = captureExpectedConsoleDiagnostics("warn");
    try {
      console.warn("A test warning");
      expect(capture.diagnostics).toEqual([["A test warning"]]);
    } finally {
      capture.stop();
    }
  });
});
