import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { expectNoA11yViolations } from "../test/a11y";
import VoiceSettings from "./voice-settings";
import { VOICE_INPUT_STORAGE_KEY, VOICE_OUTPUT_STORAGE_KEY } from "../voice/settings";

type FakeMediaDevices = {
  enumerateDevices: ReturnType<typeof vi.fn>;
  getUserMedia: ReturnType<typeof vi.fn>;
  addEventListener: ReturnType<typeof vi.fn>;
  removeEventListener: ReturnType<typeof vi.fn>;
};

function fakeDevice(kind: MediaDeviceKind, deviceId: string, label: string): MediaDeviceInfo {
  return {
    deviceId,
    kind,
    label,
    groupId: "group",
    toJSON: () => ({}),
  } as MediaDeviceInfo;
}

function fakeStream(): MediaStream {
  const track = {
    stop: vi.fn(),
  } as unknown as MediaStreamTrack;
  return {
    getTracks: () => [track],
  } as unknown as MediaStream;
}

type AudioContextSpies = {
  createMediaStreamDestination: ReturnType<typeof vi.fn>;
};

let audioSpies: AudioContextSpies;

function installFakeAudioContext() {
  class FakeAnalyser {
    fftSize = 1024;
    getByteTimeDomainData(buf: Uint8Array) {
      buf.fill(128);
    }
  }
  audioSpies = {
    createMediaStreamDestination: vi.fn().mockReturnValue({ stream: {} as MediaStream }),
  };
  class FakeAudioContext {
    currentTime = 0;
    destination = {};
    createMediaStreamSource() {
      return { connect: () => {} };
    }
    createAnalyser() {
      return new FakeAnalyser();
    }
    createMediaStreamDestination() {
      return (audioSpies.createMediaStreamDestination as () => { stream: MediaStream })();
    }
    createOscillator() {
      return {
        type: "sine",
        frequency: { value: 0 },
        connect: () => ({ connect: () => ({}) }),
        start: () => {},
        stop: () => {},
      };
    }
    createGain() {
      return {
        gain: {
          setValueAtTime: () => {},
          linearRampToValueAtTime: () => {},
        },
        connect: (next: unknown) => next,
      };
    }
    close() {
      return Promise.resolve();
    }
  }
  (globalThis as unknown as { AudioContext: typeof FakeAudioContext }).AudioContext =
    FakeAudioContext;
}

const devices: MediaDeviceInfo[] = [
  fakeDevice("audioinput", "mic-a", "Built-in Microphone"),
  fakeDevice("audioinput", "mic-b", "USB Headset"),
  fakeDevice("audiooutput", "spk-a", "Built-in Speakers"),
  fakeDevice("audiooutput", "spk-b", "HDMI Output"),
];

let fakeMediaDevices: FakeMediaDevices;

function installMediaDevices() {
  fakeMediaDevices = {
    enumerateDevices: vi.fn().mockResolvedValue(devices),
    getUserMedia: vi.fn().mockResolvedValue(fakeStream()),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  };
  Object.defineProperty(navigator, "mediaDevices", {
    configurable: true,
    value: fakeMediaDevices,
  });
}

function removeMediaDevices() {
  Object.defineProperty(navigator, "mediaDevices", {
    configurable: true,
    value: undefined,
  });
}

beforeEach(() => {
  installMediaDevices();
  installFakeAudioContext();
  // Prevent the rAF loop from running forever in tests.
  vi.stubGlobal("requestAnimationFrame", () => 0);
  vi.stubGlobal("cancelAnimationFrame", () => {});
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("<VoiceSettings>", () => {
  test("shows a loading overlay until the on-mount priming finishes", async () => {
    render(() => <VoiceSettings />);
    // Overlay is present synchronously on mount.
    expect(screen.getByRole("status")).toHaveTextContent(/loading audio devices/i);
    // It disappears once priming resolves.
    await waitFor(() => {
      expect(screen.queryByRole("status")).not.toBeInTheDocument();
    });
  });

  test("no loading overlay on unsupported platforms — fallback banner shows instead", async () => {
    removeMediaDevices();
    render(() => <VoiceSettings />);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent(/does not expose media device apis/i);
  });

  test("renders input and output selectors with System default entry", async () => {
    render(() => <VoiceSettings />);
    const input = screen.getByLabelText("Input device") as HTMLSelectElement;
    const output = screen.getByLabelText("Output device") as HTMLSelectElement;
    expect(input).toBeInTheDocument();
    expect(output).toBeInTheDocument();

    await waitFor(() => {
      expect(screen.getByRole("option", { name: "Built-in Microphone" })).toBeInTheDocument();
      expect(screen.getByRole("option", { name: "Built-in Speakers" })).toBeInTheDocument();
    });
    // "System default" appears once per select.
    expect(screen.getAllByRole("option", { name: "System default" }).length).toBe(2);
  });

  test("persists selected devices to localStorage", async () => {
    render(() => <VoiceSettings />);
    await waitFor(() =>
      expect(screen.getByRole("option", { name: "USB Headset" })).toBeInTheDocument(),
    );

    const input = screen.getByLabelText("Input device") as HTMLSelectElement;
    fireEvent.change(input, { target: { value: "mic-b" } });
    expect(localStorage.getItem(VOICE_INPUT_STORAGE_KEY)).toBe("mic-b");

    const output = screen.getByLabelText("Output device") as HTMLSelectElement;
    // The output select is disabled on platforms without setSinkId support (happy-dom),
    // so we read/write storage directly via the input path instead — persistence of
    // input selection is the behavior under test here.
    expect(output).toBeInTheDocument();
  });

  test("starts microphone test and shows a meter", async () => {
    render(() => <VoiceSettings />);
    // Wait for the on-mount priming call so we can assert the test-click in isolation.
    await waitFor(() => expect(fakeMediaDevices.getUserMedia).toHaveBeenCalledTimes(1));
    fakeMediaDevices.getUserMedia.mockClear();

    const btn = screen.getByRole("button", { name: /test microphone/i });
    fireEvent.click(btn);

    await waitFor(() => {
      // After mockClear, the click triggers exactly one fresh getUserMedia call.
      expect(fakeMediaDevices.getUserMedia).toHaveBeenCalledTimes(1);
    });
    expect(screen.getByRole("button", { name: /stop test/i })).toBeInTheDocument();
    expect(screen.getByRole("meter", { name: /microphone input level/i })).toBeInTheDocument();
  });

  test("primes device labels on mount by briefly opening a mic stream", async () => {
    // Without this, WebKitGTK (and other browsers) hide real labels and only
    // expose a single default device until permission has been granted.
    render(() => <VoiceSettings />);
    await waitFor(() => {
      expect(fakeMediaDevices.getUserMedia).toHaveBeenCalledWith({ audio: true });
    });
    // Labels come from enumerateDevices, which is refreshed after the prime.
    await waitFor(() =>
      expect(screen.getByRole("option", { name: "Built-in Microphone" })).toBeInTheDocument(),
    );
    // UI should remain in the non-testing state — priming must not leave the meter running.
    expect(screen.getByRole("button", { name: /test microphone/i })).toBeInTheDocument();
  });

  test("passes selected input deviceId as constraint to getUserMedia", async () => {
    render(() => <VoiceSettings />);
    await waitFor(() =>
      expect(screen.getByRole("option", { name: "USB Headset" })).toBeInTheDocument(),
    );
    // Clear the on-mount priming call so we can assert the constraint from the click alone.
    fakeMediaDevices.getUserMedia.mockClear();
    const input = screen.getByLabelText("Input device") as HTMLSelectElement;
    fireEvent.change(input, { target: { value: "mic-b" } });

    fireEvent.click(screen.getByRole("button", { name: /test microphone/i }));
    await waitFor(() => {
      expect(fakeMediaDevices.getUserMedia).toHaveBeenCalledWith({
        audio: { deviceId: { exact: "mic-b" } },
      });
    });
  });

  test("requests microphone access on mount to surface the OS permission prompt early", async () => {
    render(() => <VoiceSettings />);
    // The warm-up fires in onMount; wait for it to land.
    await waitFor(() => {
      expect(fakeMediaDevices.getUserMedia).toHaveBeenCalledWith({ audio: true });
    });
    // And no user-visible error when it succeeds — warm-up must stay silent.
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  test("warm-up denial stays silent (no alert before the user interacts)", async () => {
    fakeMediaDevices.getUserMedia.mockRejectedValue(new Error("Permission denied"));
    render(() => <VoiceSettings />);
    await waitFor(() => {
      expect(fakeMediaDevices.getUserMedia).toHaveBeenCalledWith({ audio: true });
    });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  test("shows an error when microphone permission is denied", async () => {
    // Reject every call: the silent on-mount warm-up AND the explicit click.
    // Only the click's rejection should surface a user-visible error.
    fakeMediaDevices.getUserMedia.mockRejectedValue(new Error("Permission denied"));
    render(() => <VoiceSettings />);
    // Wait for the silent on-mount prime to resolve, then arrange the denial
    // for the explicit Test microphone click.
    await waitFor(() => expect(fakeMediaDevices.getUserMedia).toHaveBeenCalled());
    fakeMediaDevices.getUserMedia.mockRejectedValueOnce(new Error("Permission denied"));
    fireEvent.click(screen.getByRole("button", { name: /test microphone/i }));
    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent(/permission denied/i);
    });
    expect(screen.getByRole("button", { name: /test microphone/i })).toBeInTheDocument();
  });

  test("restores the last selected input from localStorage", async () => {
    localStorage.setItem(VOICE_INPUT_STORAGE_KEY, "mic-b");
    render(() => <VoiceSettings />);
    await waitFor(() =>
      expect(screen.getByRole("option", { name: "USB Headset" })).toBeInTheDocument(),
    );
    const input = screen.getByLabelText("Input device") as HTMLSelectElement;
    expect(input.value).toBe("mic-b");
  });

  test("shows fallback message when mediaDevices is unsupported", async () => {
    removeMediaDevices();
    render(() => <VoiceSettings />);
    expect(screen.getByRole("alert")).toHaveTextContent(/does not expose media device apis/i);
    expect(screen.getByLabelText("Input device")).toBeDisabled();
  });

  test("routes test tone directly to destination when setSinkId is unsupported", async () => {
    // WebKitGTK (Tauri's Linux webview) does not expose setSinkId, so the tone
    // should skip the MediaStreamDestination → Audio element indirection and
    // play straight through the AudioContext destination.
    const originalSetSinkId = Object.getOwnPropertyDescriptor(
      HTMLAudioElement.prototype,
      "setSinkId",
    );
    delete (HTMLAudioElement.prototype as unknown as { setSinkId?: unknown }).setSinkId;
    try {
      render(() => <VoiceSettings />);
      fireEvent.click(screen.getByRole("button", { name: /play test sound/i }));
      await waitFor(() =>
        expect(screen.getByRole("button", { name: /play test sound/i })).not.toBeDisabled(),
      );
      // On WebKitGTK setSinkId is absent, so the tone must skip the
      // MediaStreamDestination → Audio element path (which does not play
      // reliably there) and connect straight to the AudioContext destination.
      expect(audioSpies.createMediaStreamDestination).not.toHaveBeenCalled();
    } finally {
      if (originalSetSinkId) {
        Object.defineProperty(HTMLAudioElement.prototype, "setSinkId", originalSetSinkId);
      }
    }
  });

  test("persists output device selection to localStorage", async () => {
    // setSinkId isn't present in happy-dom; force the output select enabled for this test
    // by defining setSinkId on the prototype, then verify we save the value on change.
    Object.defineProperty(HTMLAudioElement.prototype, "setSinkId", {
      configurable: true,
      value: () => Promise.resolve(),
    });
    try {
      render(() => <VoiceSettings />);
      await waitFor(() =>
        expect(screen.getByRole("option", { name: "HDMI Output" })).toBeInTheDocument(),
      );
      const output = screen.getByLabelText("Output device") as HTMLSelectElement;
      fireEvent.change(output, { target: { value: "spk-b" } });
      expect(localStorage.getItem(VOICE_OUTPUT_STORAGE_KEY)).toBe("spk-b");
    } finally {
      delete (HTMLAudioElement.prototype as unknown as { setSinkId?: unknown }).setSinkId;
    }
  });

  test("has no axe violations", async () => {
    const { container } = render(() => <VoiceSettings />);
    await waitFor(() =>
      expect(screen.getByRole("option", { name: "USB Headset" })).toBeInTheDocument(),
    );
    await expectNoA11yViolations(container, "VoiceSettings");
  });
});
