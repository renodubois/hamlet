import { useEffect, useState } from "react";
import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderNative } from "../render";
import { captureReactDiagnostics } from "../setup";
import { FakeEventSource, latestFakeEventSource } from "./sse";

function SseProbe() {
  const [status, setStatus] = useState("waiting");

  useEffect(() => {
    const source = new FakeEventSource("/messages/subscribe");
    source.onmessage = (event) => setStatus(event.data);
    return () => source.close();
  }, []);

  return <output>{status}</output>;
}

describe("FakeEventSource", () => {
  it("delivers externally driven updates inside act", () => {
    renderNative(<SseProbe />);
    const source = latestFakeEventSource();
    expect(source).toBeDefined();

    const capture = captureReactDiagnostics();
    try {
      source?.pushConnected();
      expect(screen.getByText("connected")).toBeInTheDocument();
      expect(capture.diagnostics).toEqual([]);
    } finally {
      capture.stop();
    }
  });
});
