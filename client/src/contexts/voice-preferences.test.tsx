import { act, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, test } from "vitest";
import { captureReactDiagnostics } from "../test/setup";
import { renderNative } from "../test/render";
import {
  VOICE_CAMERA_STORAGE_KEY,
  VOICE_INPUT_GAIN_STORAGE_KEY,
  VOICE_INPUT_STORAGE_KEY,
  VOICE_NOISE_SUPPRESSION_STORAGE_KEY,
  VOICE_OUTPUT_STORAGE_KEY,
  VOICE_SHOW_SPEAKING_EVERYWHERE_KEY,
} from "../voice/settings";
import {
  VoicePreferencesProvider,
  useVoicePreferences,
  type VoicePreferencesContextValue,
} from "./voice-preferences";

let currentPreferences: VoicePreferencesContextValue;
const observedValues: VoicePreferencesContextValue[] = [];

function Probe(props: { label: string }) {
  const preferences = useVoicePreferences();
  currentPreferences = preferences;
  observedValues.push(preferences);
  return (
    <output data-testid={props.label}>
      {[
        preferences.inputDeviceId,
        preferences.outputDeviceId,
        preferences.cameraDeviceId,
        String(preferences.noiseSuppression),
        String(preferences.inputGain),
        String(preferences.showSpeakingEverywhere),
      ].join("|")}
    </output>
  );
}

function mount() {
  return renderNative(
    <VoicePreferencesProvider>
      <Probe label="first" />
      <Probe label="second" />
    </VoicePreferencesProvider>,
  );
}

afterEach(() => {
  observedValues.length = 0;
  localStorage.clear();
});

describe("VoicePreferencesProvider", () => {
  test("loads direct values and exposes a stable VoiceSession snapshot", () => {
    localStorage.setItem(VOICE_INPUT_STORAGE_KEY, "mic-a");
    localStorage.setItem(VOICE_OUTPUT_STORAGE_KEY, "speaker-a");
    localStorage.setItem(VOICE_CAMERA_STORAGE_KEY, "camera-a");
    localStorage.setItem(VOICE_NOISE_SUPPRESSION_STORAGE_KEY, "off");
    localStorage.setItem(VOICE_INPUT_GAIN_STORAGE_KEY, "1.5");
    localStorage.setItem(VOICE_SHOW_SPEAKING_EVERYWHERE_KEY, "on");

    mount();

    expect(screen.getByTestId("first")).toHaveTextContent(
      "mic-a|speaker-a|camera-a|false|1.5|true",
    );
    expect(currentPreferences.snapshot).toEqual({
      inputDeviceId: "mic-a",
      outputDeviceId: "speaker-a",
      cameraDeviceId: "camera-a",
      noiseSuppression: false,
      inputGain: 1.5,
      showSpeakingEverywhere: true,
    });
    expect(currentPreferences.snapshot).not.toBe(currentPreferences);
  });

  test("actions update every same-tab consumer and persist all preferences", async () => {
    mount();

    await act(async () => {
      currentPreferences.setInputDeviceId("mic-b");
      currentPreferences.setOutputDeviceId("speaker-b");
      currentPreferences.setCameraDeviceId("camera-b");
      currentPreferences.setNoiseSuppression(false);
      currentPreferences.setInputGain(1.25);
      currentPreferences.setShowSpeakingEverywhere(true);
    });

    const expected = "mic-b|speaker-b|camera-b|false|1.25|true";
    expect(screen.getByTestId("first")).toHaveTextContent(expected);
    expect(screen.getByTestId("second")).toHaveTextContent(expected);
    await waitFor(() => {
      expect(localStorage.getItem(VOICE_INPUT_STORAGE_KEY)).toBe("mic-b");
      expect(localStorage.getItem(VOICE_OUTPUT_STORAGE_KEY)).toBe("speaker-b");
      expect(localStorage.getItem(VOICE_CAMERA_STORAGE_KEY)).toBe("camera-b");
      expect(localStorage.getItem(VOICE_NOISE_SUPPRESSION_STORAGE_KEY)).toBe("off");
      expect(localStorage.getItem(VOICE_INPUT_GAIN_STORAGE_KEY)).toBe("1.25");
      expect(localStorage.getItem(VOICE_SHOW_SPEAKING_EVERYWHERE_KEY)).toBe("on");
    });
  });

  test("action identities stay stable and no-op updates retain the snapshot", async () => {
    mount();
    const initial = currentPreferences;
    const initialSnapshot = initial.snapshot;

    await act(async () => initial.setInputDeviceId(""));
    expect(currentPreferences.snapshot).toBe(initialSnapshot);

    await act(async () => initial.setInputDeviceId("mic-c"));
    expect(currentPreferences.snapshot).not.toBe(initialSnapshot);
    expect(currentPreferences.setInputDeviceId).toBe(initial.setInputDeviceId);
    expect(currentPreferences.setOutputDeviceId).toBe(initial.setOutputDeviceId);
    expect(currentPreferences.setCameraDeviceId).toBe(initial.setCameraDeviceId);
    expect(currentPreferences.setNoiseSuppression).toBe(initial.setNoiseSuppression);
    expect(currentPreferences.setInputGain).toBe(initial.setInputGain);
    expect(currentPreferences.setShowSpeakingEverywhere).toBe(initial.setShowSpeakingEverywhere);
  });

  test("remains reactive under Strict Mode replay without React diagnostics", async () => {
    const diagnostics = captureReactDiagnostics();
    const view = mount();

    await act(async () => currentPreferences.setInputGain(4));
    expect(screen.getByTestId("first")).toHaveTextContent("|||true|2|false");
    await waitFor(() => expect(localStorage.getItem(VOICE_INPUT_GAIN_STORAGE_KEY)).toBe("2"));

    view.unmount();
    diagnostics.stop();
    expect(diagnostics.diagnostics).toEqual([]);
  });
});
