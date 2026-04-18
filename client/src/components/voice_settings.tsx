import { createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";

export const VOICE_INPUT_STORAGE_KEY = "hamlet:voice:input_device";
export const VOICE_OUTPUT_STORAGE_KEY = "hamlet:voice:output_device";

// "" means "let the browser pick the system default".
const DEFAULT_DEVICE_ID = "";

// `setSinkId` is only available on Chromium-based WebViews. We detect it once
// so we can disable the output selector with a helpful note on other platforms.
function sinkIdSupported(): boolean {
  if (typeof HTMLAudioElement === "undefined") return false;
  return "setSinkId" in HTMLAudioElement.prototype;
}

function mediaDevicesSupported(): boolean {
  return (
    typeof navigator !== "undefined" &&
    !!navigator.mediaDevices &&
    typeof navigator.mediaDevices.enumerateDevices === "function" &&
    typeof navigator.mediaDevices.getUserMedia === "function"
  );
}

export default function VoiceSettings() {
  const supported = mediaDevicesSupported();
  const canPickOutput = sinkIdSupported();

  const [inputDevices, setInputDevices] = createSignal<MediaDeviceInfo[]>([]);
  const [outputDevices, setOutputDevices] = createSignal<MediaDeviceInfo[]>([]);
  // `true` until warm-up resolves (success or silent failure). Stays `false` on
  // unsupported platforms so the fallback banner isn't obscured by the overlay.
  const [isLoading, setIsLoading] = createSignal(supported);
  const [inputId, setInputId] = createSignal<string>(
    localStorage.getItem(VOICE_INPUT_STORAGE_KEY) ?? DEFAULT_DEVICE_ID,
  );
  const [outputId, setOutputId] = createSignal<string>(
    localStorage.getItem(VOICE_OUTPUT_STORAGE_KEY) ?? DEFAULT_DEVICE_ID,
  );
  const [micTesting, setMicTesting] = createSignal(false);
  const [micLevel, setMicLevel] = createSignal(0);
  const [micError, setMicError] = createSignal<string | null>(null);
  const [outputError, setOutputError] = createSignal<string | null>(null);
  const [playingTestSound, setPlayingTestSound] = createSignal(false);

  let micStream: MediaStream | null = null;
  let micCtx: AudioContext | null = null;
  let micRaf: number | null = null;
  let inputSelectRef: HTMLSelectElement | undefined;
  let outputSelectRef: HTMLSelectElement | undefined;
  const setInputSelectRef = (el: HTMLSelectElement) => {
    inputSelectRef = el;
  };
  const setOutputSelectRef = (el: HTMLSelectElement) => {
    outputSelectRef = el;
  };

  const refreshDevices = async () => {
    if (!supported) return;
    try {
      const devices = await navigator.mediaDevices.enumerateDevices();
      setInputDevices(devices.filter((d) => d.kind === "audioinput"));
      setOutputDevices(devices.filter((d) => d.kind === "audiooutput"));
    } catch {
      // Enumeration can fail in restricted contexts; leave lists empty and
      // the UI will fall through to "System default" only.
    }
  };

  // Browsers (especially WebKitGTK) hide real device labels and only expose a
  // single default input until the page has been granted microphone access. Do
  // a one-shot silent getUserMedia on mount so the dropdowns show real device
  // names, then drop the stream immediately — startMicTest re-opens its own.
  // The loading overlay stays up until this resolves (either outcome).
  const primeDeviceLabels = async () => {
    if (!supported) return;
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      stream.getTracks().forEach((t) => t.stop());
      await refreshDevices();
    } catch {
      // User/OS denied permission — keep the "System default" fallback.
    } finally {
      setIsLoading(false);
    }
  };

  const stopMicTest = () => {
    if (micRaf != null) {
      cancelAnimationFrame(micRaf);
      micRaf = null;
    }
    if (micStream) {
      micStream.getTracks().forEach((t) => t.stop());
      micStream = null;
    }
    if (micCtx) {
      void micCtx.close();
      micCtx = null;
    }
    setMicLevel(0);
    setMicTesting(false);
  };

  const startMicTest = async () => {
    setMicError(null);
    try {
      const id = inputId();
      const constraints: MediaStreamConstraints = {
        audio: id ? { deviceId: { exact: id } } : true,
      };
      micStream = await navigator.mediaDevices.getUserMedia(constraints);
      // Once permission is granted, device labels become available.
      void refreshDevices();

      micCtx = new AudioContext();
      const source = micCtx.createMediaStreamSource(micStream);
      const analyser = micCtx.createAnalyser();
      analyser.fftSize = 1024;
      source.connect(analyser);
      const buf = new Uint8Array(analyser.fftSize);

      const tick = () => {
        analyser.getByteTimeDomainData(buf);
        let sumSquares = 0;
        for (let i = 0; i < buf.length; i++) {
          const v = (buf[i] - 128) / 128;
          sumSquares += v * v;
        }
        const rms = Math.sqrt(sumSquares / buf.length);
        // Scale RMS up so normal speech fills most of the bar.
        setMicLevel(Math.min(1, rms * 2.5));
        micRaf = requestAnimationFrame(tick);
      };
      tick();
      setMicTesting(true);
    } catch (e) {
      stopMicTest();
      setMicError(e instanceof Error ? e.message : "Could not access microphone");
    }
  };

  const toggleMicTest = () => {
    if (micTesting()) stopMicTest();
    else void startMicTest();
  };

  const playTestSound = async () => {
    setOutputError(null);
    setPlayingTestSound(true);
    const ctx = new AudioContext();
    try {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = "sine";
      osc.frequency.value = 440;
      const now = ctx.currentTime;
      gain.gain.setValueAtTime(0, now);
      gain.gain.linearRampToValueAtTime(0.2, now + 0.05);
      gain.gain.linearRampToValueAtTime(0, now + 0.75);

      const id = outputId();
      // The MediaStreamDestination → Audio element hop only buys us the ability
      // to steer the tone at a chosen output via setSinkId. When we can't or
      // don't need to steer, route straight to the AudioContext destination —
      // that path works in plain WebAudio (incl. WebKitGTK) without the extra
      // MediaStream plumbing which isn't reliable everywhere.
      if (id && canPickOutput) {
        const dest = ctx.createMediaStreamDestination();
        osc.connect(gain).connect(dest);
        const audio = new Audio();
        audio.srcObject = dest.stream;
        try {
          await (
            audio as HTMLAudioElement & { setSinkId?: (id: string) => Promise<void> }
          ).setSinkId?.(id);
        } catch (e) {
          setOutputError(
            e instanceof Error ? e.message : "Could not route audio to selected device",
          );
        }
        osc.start(now);
        osc.stop(now + 0.8);
        await audio.play();
        await new Promise((r) => setTimeout(r, 850));
        audio.pause();
      } else {
        osc.connect(gain).connect(ctx.destination);
        osc.start(now);
        osc.stop(now + 0.8);
        await new Promise((r) => setTimeout(r, 850));
      }
    } catch (e) {
      setOutputError(e instanceof Error ? e.message : "Could not play test sound");
    } finally {
      void ctx.close();
      setPlayingTestSound(false);
    }
  };

  onMount(() => {
    if (!supported) return;
    void refreshDevices();
    void primeDeviceLabels();
    navigator.mediaDevices.addEventListener("devicechange", refreshDevices);
  });

  createEffect(() => localStorage.setItem(VOICE_INPUT_STORAGE_KEY, inputId()));
  createEffect(() => localStorage.setItem(VOICE_OUTPUT_STORAGE_KEY, outputId()));

  // <select>'s `value` only applies if the matching <option> already exists. The
  // device list loads asynchronously, so we also reapply the value whenever the
  // options change.
  createEffect(() => {
    inputDevices();
    if (inputSelectRef) inputSelectRef.value = inputId();
  });
  createEffect(() => {
    outputDevices();
    if (outputSelectRef) outputSelectRef.value = outputId();
  });

  onCleanup(() => {
    stopMicTest();
    if (supported) {
      navigator.mediaDevices.removeEventListener("devicechange", refreshDevices);
    }
  });

  const inputLabel = (d: MediaDeviceInfo, i: number) => d.label || `Microphone ${i + 1}`;
  const outputLabel = (d: MediaDeviceInfo, i: number) => d.label || `Speaker ${i + 1}`;

  return (
    <div class="relative flex flex-col gap-6">
      <Show when={isLoading()}>
        <div
          role="status"
          aria-live="polite"
          class="absolute inset-0 z-10 flex items-center justify-center rounded-md bg-gray-800/60 backdrop-blur-sm"
        >
          <div class="flex items-center gap-3 text-sm text-gray-100">
            <span
              aria-hidden="true"
              class="inline-block h-4 w-4 animate-spin rounded-full border-2 border-gray-500 border-t-blue-400"
            />
            <span>Loading audio devices…</span>
          </div>
        </div>
      </Show>

      <Show when={!supported}>
        <p class="text-red-400" role="alert">
          This platform does not expose media device APIs, so voice chat is unavailable here.
        </p>
      </Show>

      <div class="flex flex-col gap-2">
        <label for="voice-input-select" class="font-medium text-gray-100">
          Input device
        </label>
        <select
          id="voice-input-select"
          ref={setInputSelectRef}
          class="bg-gray-700 text-gray-100 rounded-md px-3 py-2 text-sm border border-gray-600 focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
          value={inputId()}
          disabled={!supported}
          onChange={(e) => {
            setInputId(e.currentTarget.value);
            if (micTesting()) {
              stopMicTest();
              void startMicTest();
            }
          }}
        >
          <option value={DEFAULT_DEVICE_ID}>System default</option>
          <For each={inputDevices()}>
            {(d, i) => <option value={d.deviceId}>{inputLabel(d, i())}</option>}
          </For>
        </select>

        <div class="flex items-center gap-3 mt-1">
          <button
            type="button"
            class="bg-blue-600 hover:bg-blue-700 disabled:opacity-50 text-white rounded-md px-4 py-2 text-sm font-medium"
            onClick={toggleMicTest}
            disabled={!supported}
          >
            {micTesting() ? "Stop test" : "Test microphone"}
          </button>
          <div
            class="flex-1 h-2 bg-gray-700 rounded overflow-hidden"
            role="meter"
            aria-label="Microphone input level"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(micLevel() * 100)}
          >
            <div
              class="h-full bg-green-500 transition-[width] duration-75"
              style={{ width: `${Math.round(micLevel() * 100)}%` }}
            />
          </div>
        </div>
        <Show when={micError()}>
          {(msg) => (
            <p class="text-red-400 text-sm" role="alert">
              {msg()}
            </p>
          )}
        </Show>
        <Show when={micTesting()}>
          <p class="text-xs text-gray-400">Speak into your mic — the bar should move.</p>
        </Show>
      </div>

      <div class="flex flex-col gap-2">
        <label for="voice-output-select" class="font-medium text-gray-100">
          Output device
        </label>
        <select
          id="voice-output-select"
          ref={setOutputSelectRef}
          class="bg-gray-700 text-gray-100 rounded-md px-3 py-2 text-sm border border-gray-600 focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
          value={outputId()}
          disabled={!supported || !canPickOutput}
          onChange={(e) => setOutputId(e.currentTarget.value)}
        >
          <option value={DEFAULT_DEVICE_ID}>System default</option>
          <For each={outputDevices()}>
            {(d, i) => <option value={d.deviceId}>{outputLabel(d, i())}</option>}
          </For>
        </select>
        <Show when={supported && !canPickOutput}>
          <p class="text-xs text-gray-400">
            This platform plays audio through the system default device; per-app output selection is
            not supported here.
          </p>
        </Show>

        <div class="mt-1">
          <button
            type="button"
            class="bg-blue-600 hover:bg-blue-700 disabled:opacity-50 text-white rounded-md px-4 py-2 text-sm font-medium"
            onClick={() => void playTestSound()}
            disabled={!supported || playingTestSound()}
          >
            {playingTestSound() ? "Playing..." : "Play test sound"}
          </button>
        </div>
        <Show when={outputError()}>
          {(msg) => (
            <p class="text-red-400 text-sm" role="alert">
              {msg()}
            </p>
          )}
        </Show>
      </div>
    </div>
  );
}
