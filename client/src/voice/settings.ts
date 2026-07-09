export const VOICE_INPUT_STORAGE_KEY = "hamlet:voice:input_device";
export const VOICE_OUTPUT_STORAGE_KEY = "hamlet:voice:output_device";
export const VOICE_CAMERA_STORAGE_KEY = "hamlet:voice:camera_device";
export const VOICE_NOISE_SUPPRESSION_STORAGE_KEY = "hamlet:voice:noise_suppression";
export const VOICE_INPUT_GAIN_STORAGE_KEY = "hamlet:voice:input_gain";
export const VOICE_SHOW_SPEAKING_EVERYWHERE_KEY = "hamlet:voice:show_speaking_everywhere";

let showSpeakingEverywhere =
  typeof localStorage !== "undefined" &&
  localStorage.getItem(VOICE_SHOW_SPEAKING_EVERYWHERE_KEY) === "on";

export function showSpeakingIndicatorsEverywhere(): boolean {
  return showSpeakingEverywhere;
}

export function setShowSpeakingEverywhereSignal(value: boolean | ((current: boolean) => boolean)) {
  showSpeakingEverywhere = typeof value === "function" ? value(showSpeakingEverywhere) : value;
}

export function getNoiseSuppressionEnabled(): boolean {
  // Default on — matches getUserMedia default and is what most users expect.
  return localStorage.getItem(VOICE_NOISE_SUPPRESSION_STORAGE_KEY) !== "off";
}

/**
 * Returns the saved input gain in the range [0, 2]. 1.0 = unity. Applied by
 * the voice chat layer via a Web Audio GainNode inserted between the mic and
 * the published track.
 */
export function getInputGain(): number {
  const raw = localStorage.getItem(VOICE_INPUT_GAIN_STORAGE_KEY);
  const n = raw == null ? 1 : Number.parseFloat(raw);
  if (!Number.isFinite(n)) return 1;
  return Math.min(2, Math.max(0, n));
}
