export const VOICE_INPUT_STORAGE_KEY = "hamlet:voice:input_device";
export const VOICE_OUTPUT_STORAGE_KEY = "hamlet:voice:output_device";
export const VOICE_CAMERA_STORAGE_KEY = "hamlet:voice:camera_device";
export const VOICE_NOISE_SUPPRESSION_STORAGE_KEY = "hamlet:voice:noise_suppression";
export const VOICE_INPUT_GAIN_STORAGE_KEY = "hamlet:voice:input_gain";
export const VOICE_SHOW_SPEAKING_EVERYWHERE_KEY = "hamlet:voice:show_speaking_everywhere";

export interface VoicePreferencesSnapshot {
  readonly inputDeviceId: string;
  readonly outputDeviceId: string;
  readonly cameraDeviceId: string;
  readonly noiseSuppression: boolean;
  readonly inputGain: number;
  readonly showSpeakingEverywhere: boolean;
}

export interface SerializedVoicePreferences {
  readonly inputDeviceId: string | null;
  readonly outputDeviceId: string | null;
  readonly cameraDeviceId: string | null;
  readonly noiseSuppression: string | null;
  readonly inputGain: string | null;
  readonly showSpeakingEverywhere: string | null;
}

export type VoicePreferencesStorage = Pick<Storage, "getItem" | "setItem">;

export const DEFAULT_VOICE_PREFERENCES: VoicePreferencesSnapshot = Object.freeze({
  inputDeviceId: "",
  outputDeviceId: "",
  cameraDeviceId: "",
  // Default on, matching getUserMedia and what most users expect.
  noiseSuppression: true,
  inputGain: 1,
  showSpeakingEverywhere: false,
});

export function parseInputGain(raw: string | null): number {
  const value = raw == null ? DEFAULT_VOICE_PREFERENCES.inputGain : Number.parseFloat(raw);
  if (!Number.isFinite(value)) return DEFAULT_VOICE_PREFERENCES.inputGain;
  return Math.min(2, Math.max(0, value));
}

export function parseVoicePreferences(
  serialized: SerializedVoicePreferences,
): VoicePreferencesSnapshot {
  return {
    inputDeviceId: serialized.inputDeviceId ?? DEFAULT_VOICE_PREFERENCES.inputDeviceId,
    outputDeviceId: serialized.outputDeviceId ?? DEFAULT_VOICE_PREFERENCES.outputDeviceId,
    cameraDeviceId: serialized.cameraDeviceId ?? DEFAULT_VOICE_PREFERENCES.cameraDeviceId,
    noiseSuppression: serialized.noiseSuppression !== "off",
    inputGain: parseInputGain(serialized.inputGain),
    showSpeakingEverywhere: serialized.showSpeakingEverywhere === "on",
  };
}

function browserStorage(): VoicePreferencesStorage | undefined {
  try {
    return typeof localStorage === "undefined" ? undefined : localStorage;
  } catch {
    return undefined;
  }
}

function read(storage: VoicePreferencesStorage, key: string): string | null {
  try {
    return storage.getItem(key);
  } catch {
    return null;
  }
}

export function loadVoicePreferences(
  storage: VoicePreferencesStorage | undefined = browserStorage(),
): VoicePreferencesSnapshot {
  if (!storage) return { ...DEFAULT_VOICE_PREFERENCES };
  return parseVoicePreferences({
    inputDeviceId: read(storage, VOICE_INPUT_STORAGE_KEY),
    outputDeviceId: read(storage, VOICE_OUTPUT_STORAGE_KEY),
    cameraDeviceId: read(storage, VOICE_CAMERA_STORAGE_KEY),
    noiseSuppression: read(storage, VOICE_NOISE_SUPPRESSION_STORAGE_KEY),
    inputGain: read(storage, VOICE_INPUT_GAIN_STORAGE_KEY),
    showSpeakingEverywhere: read(storage, VOICE_SHOW_SPEAKING_EVERYWHERE_KEY),
  });
}

export function saveVoicePreferences(
  preferences: VoicePreferencesSnapshot,
  storage: VoicePreferencesStorage | undefined = browserStorage(),
): void {
  if (!storage) return;
  const values: ReadonlyArray<readonly [string, string]> = [
    [VOICE_INPUT_STORAGE_KEY, preferences.inputDeviceId],
    [VOICE_OUTPUT_STORAGE_KEY, preferences.outputDeviceId],
    [VOICE_CAMERA_STORAGE_KEY, preferences.cameraDeviceId],
    [VOICE_NOISE_SUPPRESSION_STORAGE_KEY, preferences.noiseSuppression ? "on" : "off"],
    [VOICE_INPUT_GAIN_STORAGE_KEY, String(preferences.inputGain)],
    [VOICE_SHOW_SPEAKING_EVERYWHERE_KEY, preferences.showSpeakingEverywhere ? "on" : "off"],
  ];
  for (const [key, value] of values) {
    try {
      storage.setItem(key, value);
    } catch {
      // Preferences remain usable for this session when storage is unavailable
      // or full; a later action will attempt persistence again.
    }
  }
}
