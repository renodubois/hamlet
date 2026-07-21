import { describe, expect, test, vi } from "vitest";
import {
  DEFAULT_VOICE_PREFERENCES,
  VOICE_CAMERA_STORAGE_KEY,
  VOICE_INPUT_GAIN_STORAGE_KEY,
  VOICE_INPUT_STORAGE_KEY,
  VOICE_NOISE_SUPPRESSION_STORAGE_KEY,
  VOICE_OUTPUT_STORAGE_KEY,
  VOICE_SHOW_SPEAKING_EVERYWHERE_KEY,
  loadVoicePreferences,
  parseInputGain,
  parseVoicePreferences,
  saveVoicePreferences,
  type VoicePreferencesStorage,
} from "./settings";

describe("voice preference parsing", () => {
  test("uses defaults for missing and malformed values", () => {
    expect(
      parseVoicePreferences({
        inputDeviceId: null,
        outputDeviceId: null,
        cameraDeviceId: null,
        noiseSuppression: null,
        inputGain: "not-a-number",
        showSpeakingEverywhere: null,
      }),
    ).toEqual(DEFAULT_VOICE_PREFERENCES);
  });

  test("parses saved values and clamps gain to the supported range", () => {
    expect(
      parseVoicePreferences({
        inputDeviceId: "mic-a",
        outputDeviceId: "speaker-a",
        cameraDeviceId: "camera-a",
        noiseSuppression: "off",
        inputGain: "4.25",
        showSpeakingEverywhere: "on",
      }),
    ).toEqual({
      inputDeviceId: "mic-a",
      outputDeviceId: "speaker-a",
      cameraDeviceId: "camera-a",
      noiseSuppression: false,
      inputGain: 2,
      showSpeakingEverywhere: true,
    });
    expect(parseInputGain("-0.5")).toBe(0);
  });
});

describe("pure voice preference persistence", () => {
  test("loads each supplied storage without retaining module-level state", () => {
    const storage = (inputGain: string): VoicePreferencesStorage => ({
      getItem: (key) => (key === VOICE_INPUT_GAIN_STORAGE_KEY ? inputGain : null),
      setItem: vi.fn(),
    });

    expect(loadVoicePreferences(storage("0.5")).inputGain).toBe(0.5);
    expect(loadVoicePreferences(storage("1.5")).inputGain).toBe(1.5);
  });

  test("loads and saves the complete snapshot", () => {
    const values = new Map<string, string>();
    const storage: VoicePreferencesStorage = {
      getItem: (key) => values.get(key) ?? null,
      setItem: (key, value) => values.set(key, value),
    };
    const preferences = {
      inputDeviceId: "mic-b",
      outputDeviceId: "speaker-b",
      cameraDeviceId: "camera-b",
      noiseSuppression: false,
      inputGain: 1.25,
      showSpeakingEverywhere: true,
    } as const;

    saveVoicePreferences(preferences, storage);

    expect(values).toEqual(
      new Map([
        [VOICE_INPUT_STORAGE_KEY, "mic-b"],
        [VOICE_OUTPUT_STORAGE_KEY, "speaker-b"],
        [VOICE_CAMERA_STORAGE_KEY, "camera-b"],
        [VOICE_NOISE_SUPPRESSION_STORAGE_KEY, "off"],
        [VOICE_INPUT_GAIN_STORAGE_KEY, "1.25"],
        [VOICE_SHOW_SPEAKING_EVERYWHERE_KEY, "on"],
      ]),
    );
    expect(loadVoicePreferences(storage)).toEqual(preferences);
  });

  test("falls back safely when storage access is unavailable", () => {
    const storage: VoicePreferencesStorage = {
      getItem: vi.fn(() => {
        throw new Error("blocked");
      }),
      setItem: vi.fn(() => {
        throw new Error("full");
      }),
    };

    expect(loadVoicePreferences(storage)).toEqual(DEFAULT_VOICE_PREFERENCES);
    expect(() => saveVoicePreferences(DEFAULT_VOICE_PREFERENCES, storage)).not.toThrow();
  });
});
