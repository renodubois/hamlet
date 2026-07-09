import { useRef } from "react";

import {
  useAfterRenderEffect,
  useSignalState,
  List,
  registerCleanup,
  useMountEffect,
  If,
} from "../hooks/react-state";
import {
  VOICE_INPUT_STORAGE_KEY,
  VOICE_OUTPUT_STORAGE_KEY,
  VOICE_CAMERA_STORAGE_KEY,
  VOICE_NOISE_SUPPRESSION_STORAGE_KEY,
  VOICE_INPUT_GAIN_STORAGE_KEY,
  VOICE_SHOW_SPEAKING_EVERYWHERE_KEY,
  showSpeakingIndicatorsEverywhere as showSpeakingEverywhereSignal,
  setShowSpeakingEverywhereSignal,
  getNoiseSuppressionEnabled,
  getInputGain,
} from "../voice/settings";

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

  const [inputDevices, setInputDevices] = useSignalState<MediaDeviceInfo[]>([]);
  const [outputDevices, setOutputDevices] = useSignalState<MediaDeviceInfo[]>([]);
  const [cameraDevices, setCameraDevices] = useSignalState<MediaDeviceInfo[]>([]);
  // `true` until audio warm-up resolves (success or silent failure). Stays `false`
  // on unsupported platforms so the fallback banner isn't obscured by the overlay.
  const [isLoading, setIsLoading] = useSignalState(supported);
  const [inputId, setInputId] = useSignalState<string>(
    localStorage.getItem(VOICE_INPUT_STORAGE_KEY) ?? DEFAULT_DEVICE_ID,
  );
  const [outputId, setOutputId] = useSignalState<string>(
    localStorage.getItem(VOICE_OUTPUT_STORAGE_KEY) ?? DEFAULT_DEVICE_ID,
  );
  const [cameraId, setCameraId] = useSignalState<string>(
    localStorage.getItem(VOICE_CAMERA_STORAGE_KEY) ?? DEFAULT_DEVICE_ID,
  );
  const [noiseSuppression, setNoiseSuppression] = useSignalState<boolean>(
    getNoiseSuppressionEnabled(),
  );
  const [inputGain, setInputGain] = useSignalState<number>(getInputGain());
  const [showSpeakingEverywhere, setShowSpeakingEverywhere] = useSignalState<boolean>(
    showSpeakingEverywhereSignal(),
  );
  const [micTesting, setMicTesting] = useSignalState(false);
  const [micLevel, setMicLevel] = useSignalState(0);
  const [micError, setMicError] = useSignalState<string | null>(null);
  const [outputError, setOutputError] = useSignalState<string | null>(null);
  const [playingTestSound, setPlayingTestSound] = useSignalState(false);
  const [cameraPreviewStream, setCameraPreviewStream] = useSignalState<MediaStream | null>(null);
  const [cameraPreviewStarting, setCameraPreviewStarting] = useSignalState(false);
  const [cameraError, setCameraError] = useSignalState<string | null>(null);

  let micStream: MediaStream | null = null;
  let micCtx: AudioContext | null = null;
  let micRaf: number | null = null;
  const cameraVideoRef = useRef<HTMLVideoElement | null>(null);
  const cameraPreviewRequestIdRef = useRef(0);
  const isDisposedRef = useRef(false);
  let inputSelectRef: HTMLSelectElement | null | undefined;
  let outputSelectRef: HTMLSelectElement | null | undefined;
  let cameraSelectRef: HTMLSelectElement | null | undefined;
  const setInputSelectRef = (el: HTMLSelectElement) => {
    inputSelectRef = el;
  };
  const setOutputSelectRef = (el: HTMLSelectElement) => {
    outputSelectRef = el;
  };
  const setCameraSelectRef = (el: HTMLSelectElement) => {
    cameraSelectRef = el;
  };
  const attachCameraPreview = () => {
    const video = cameraVideoRef.current;
    if (!video) return;
    const stream = cameraPreviewStream();
    try {
      if (video.srcObject !== stream) video.srcObject = stream;
      if (stream) {
        const playPromise = video.play();
        if (playPromise) void playPromise.catch(() => {});
      }
    } catch {
      // Assigning srcObject is enough for the local preview; autoPlay or test-DOM
      // media assignment failures should not replace actionable device errors.
    }
  };
  const setCameraVideoRef = (el: HTMLVideoElement | null) => {
    cameraVideoRef.current = el;
    attachCameraPreview();
  };

  const refreshDevices = async () => {
    if (!supported) return;
    try {
      const devices = await navigator.mediaDevices.enumerateDevices();
      setInputDevices(devices.filter((d) => d.kind === "audioinput"));
      setOutputDevices(devices.filter((d) => d.kind === "audiooutput"));
      setCameraDevices(devices.filter((d) => d.kind === "videoinput"));
    } catch {
      // Enumeration can fail in restricted contexts; leave lists empty and
      // the UI will fall through to "System default" only.
    }
  };

  // Browsers (especially WebKitGTK) hide real microphone/speaker labels and only
  // expose a single default input until the page has been granted microphone
  // access. Do a one-shot silent audio-only getUserMedia on mount so the audio
  // dropdowns show real device names, then drop the stream immediately —
  // startMicTest re-opens its own. Camera access is intentionally not requested
  // here; preview capture only starts from the explicit camera button.
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

  const clearCameraPreviewStream = () => {
    const stream = cameraPreviewStream();
    if (stream) {
      stream.getTracks().forEach((t) => t.stop());
    }
    if (cameraVideoRef.current) cameraVideoRef.current.srcObject = null;
    setCameraPreviewStream(null);
  };

  const stopCameraPreview = () => {
    cameraPreviewRequestIdRef.current += 1;
    setCameraPreviewStarting(false);
    clearCameraPreviewStream();
  };

  const startCameraPreview = async (selectedCameraId = cameraId()) => {
    const requestId = cameraPreviewRequestIdRef.current + 1;
    cameraPreviewRequestIdRef.current = requestId;
    setCameraError(null);
    setCameraPreviewStarting(true);
    clearCameraPreviewStream();
    try {
      const constraints: MediaStreamConstraints = {
        audio: false,
        video: selectedCameraId ? { deviceId: { exact: selectedCameraId } } : true,
      };
      const stream = await navigator.mediaDevices.getUserMedia(constraints);
      if (isDisposedRef.current || requestId !== cameraPreviewRequestIdRef.current) {
        stream.getTracks().forEach((t) => t.stop());
        return;
      }
      setCameraPreviewStream(stream);
      void refreshDevices();
    } catch (e) {
      if (!isDisposedRef.current && requestId === cameraPreviewRequestIdRef.current) {
        setCameraError(e instanceof Error ? e.message : "Could not access camera");
      }
    } finally {
      if (!isDisposedRef.current && requestId === cameraPreviewRequestIdRef.current) {
        setCameraPreviewStarting(false);
      }
    }
  };

  const toggleCameraPreview = () => {
    if (cameraPreviewStream() || cameraPreviewStarting()) stopCameraPreview();
    else void startCameraPreview();
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
            audio as HTMLAudioElement & {
              setSinkId?: (id: string) => Promise<void>;
            }
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

  useMountEffect(() => {
    if (!supported) return;
    void refreshDevices();
    void primeDeviceLabels();
    navigator.mediaDevices.addEventListener("devicechange", refreshDevices);
  });

  useAfterRenderEffect(attachCameraPreview);

  useAfterRenderEffect(() => localStorage.setItem(VOICE_INPUT_STORAGE_KEY, inputId()));
  useAfterRenderEffect(() => localStorage.setItem(VOICE_OUTPUT_STORAGE_KEY, outputId()));
  useAfterRenderEffect(() => localStorage.setItem(VOICE_CAMERA_STORAGE_KEY, cameraId()));
  useAfterRenderEffect(() =>
    localStorage.setItem(VOICE_NOISE_SUPPRESSION_STORAGE_KEY, noiseSuppression() ? "on" : "off"),
  );
  useAfterRenderEffect(() =>
    localStorage.setItem(VOICE_INPUT_GAIN_STORAGE_KEY, String(inputGain())),
  );
  useAfterRenderEffect(() => {
    const v = showSpeakingEverywhere();
    localStorage.setItem(VOICE_SHOW_SPEAKING_EVERYWHERE_KEY, v ? "on" : "off");
    setShowSpeakingEverywhereSignal(v);
  });

  // <select>'s `value` only applies if the matching <option> already exists. The
  // device list loads asynchronously, so we also reapply the value whenever the
  // options change.
  useAfterRenderEffect(() => {
    inputDevices();
    if (inputSelectRef) inputSelectRef.value = inputId();
  });
  useAfterRenderEffect(() => {
    outputDevices();
    if (outputSelectRef) outputSelectRef.value = outputId();
  });
  useAfterRenderEffect(() => {
    cameraDevices();
    if (cameraSelectRef) cameraSelectRef.value = cameraId();
  });

  registerCleanup(() => {
    isDisposedRef.current = true;
    stopMicTest();
    stopCameraPreview();
    if (supported) {
      navigator.mediaDevices.removeEventListener("devicechange", refreshDevices);
    }
  });

  const inputLabel = (d: MediaDeviceInfo, i: number) => d.label || `Microphone ${i + 1}`;
  const outputLabel = (d: MediaDeviceInfo, i: number) => d.label || `Speaker ${i + 1}`;
  const cameraLabel = (d: MediaDeviceInfo, i: number) => d.label || `Camera ${i + 1}`;

  return (
    <div className="relative flex flex-col gap-6">
      <If when={isLoading()}>
        <div
          role="status"
          aria-live="polite"
          className="absolute inset-0 z-10 flex items-center justify-center rounded-md bg-gray-800/60 backdrop-blur-sm"
        >
          <div className="flex items-center gap-3 text-sm text-gray-100">
            <span
              aria-hidden="true"
              className="inline-block h-4 w-4 animate-spin rounded-full border-2 border-gray-500 border-t-blue-400"
            />
            <span>Loading media devices…</span>
          </div>
        </div>
      </If>

      <If when={!supported}>
        <p className="text-red-400" role="alert">
          This platform does not expose media device APIs, so voice and video chat are unavailable
          here.
        </p>
      </If>

      <div className="flex flex-col gap-2">
        <label htmlFor="voice-input-select" className="font-medium text-gray-100">
          Input device
        </label>
        <select
          id="voice-input-select"
          ref={setInputSelectRef}
          className="bg-gray-700 text-gray-100 rounded-md px-3 py-2 text-sm border border-gray-600 focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
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
          <List each={inputDevices()}>
            {(d, i) => <option value={d.deviceId}>{inputLabel(d, i())}</option>}
          </List>
        </select>

        <div className="flex items-center gap-3 mt-1">
          <button
            type="button"
            className="bg-blue-600 hover:bg-blue-700 disabled:opacity-50 text-white rounded-md px-4 py-2 text-sm font-medium"
            onClick={toggleMicTest}
            disabled={!supported}
          >
            {micTesting() ? "Stop test" : "Test microphone"}
          </button>
          <div
            className="flex-1 h-2 bg-gray-700 rounded overflow-hidden"
            role="meter"
            aria-label="Microphone input level"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(micLevel() * 100)}
          >
            <div
              className="h-full bg-green-500 transition-[width] duration-75"
              style={{ width: `${Math.round(micLevel() * 100)}%` }}
            />
          </div>
        </div>
        <If when={micError()}>
          {(msg) => (
            <p className="text-red-400 text-sm" role="alert">
              {msg()}
            </p>
          )}
        </If>
        <If when={micTesting()}>
          <p className="text-xs text-gray-400">Speak into your mic — the bar should move.</p>
        </If>
      </div>

      <div className="flex flex-col gap-2">
        <label htmlFor="voice-camera-select" className="font-medium text-gray-100">
          Camera
        </label>
        <select
          id="voice-camera-select"
          ref={setCameraSelectRef}
          className="bg-gray-700 text-gray-100 rounded-md px-3 py-2 text-sm border border-gray-600 focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
          value={cameraId()}
          disabled={!supported}
          onChange={(e) => {
            const nextId = e.currentTarget.value;
            setCameraId(nextId);
            if (cameraPreviewStream()) void startCameraPreview(nextId);
          }}
        >
          <option value={DEFAULT_DEVICE_ID}>System default</option>
          <List each={cameraDevices()}>
            {(d, i) => <option value={d.deviceId}>{cameraLabel(d, i())}</option>}
          </List>
        </select>

        <div className="mt-1 flex items-center gap-3">
          <button
            type="button"
            className="bg-blue-600 hover:bg-blue-700 disabled:opacity-50 text-white rounded-md px-4 py-2 text-sm font-medium"
            onClick={toggleCameraPreview}
            disabled={!supported}
            aria-busy={cameraPreviewStarting()}
          >
            <If
              when={cameraPreviewStarting()}
              fallback={cameraPreviewStream() ? "Stop preview" : "Preview camera"}
            >
              Starting preview…
            </If>
          </button>
          <If when={cameraPreviewStream()}>
            <p className="text-xs text-gray-400">Your camera preview stays local to this device.</p>
          </If>
        </div>
        <If when={cameraPreviewStream()}>
          <video
            ref={setCameraVideoRef}
            aria-label="Camera preview"
            autoPlay
            muted
            playsInline
            className="mt-2 aspect-video w-full max-w-sm rounded-md bg-black object-cover"
          />
        </If>
        <If when={cameraError()}>
          {(msg) => (
            <p className="text-red-400 text-sm" role="alert">
              {msg()}
            </p>
          )}
        </If>
      </div>

      <div className="flex flex-col gap-2">
        <label htmlFor="voice-output-select" className="font-medium text-gray-100">
          Output device
        </label>
        <select
          id="voice-output-select"
          ref={setOutputSelectRef}
          className="bg-gray-700 text-gray-100 rounded-md px-3 py-2 text-sm border border-gray-600 focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50"
          value={outputId()}
          disabled={!supported || !canPickOutput}
          onChange={(e) => setOutputId(e.currentTarget.value)}
        >
          <option value={DEFAULT_DEVICE_ID}>System default</option>
          <List each={outputDevices()}>
            {(d, i) => <option value={d.deviceId}>{outputLabel(d, i())}</option>}
          </List>
        </select>
        <If when={supported && !canPickOutput}>
          <p className="text-xs text-gray-400">
            This platform plays audio through the system default device; per-app output selection is
            not supported here.
          </p>
        </If>

        <div className="mt-1">
          <button
            type="button"
            className="bg-blue-600 hover:bg-blue-700 disabled:opacity-50 text-white rounded-md px-4 py-2 text-sm font-medium"
            onClick={() => void playTestSound()}
            disabled={!supported || playingTestSound()}
          >
            {playingTestSound() ? "Playing..." : "Play test sound"}
          </button>
        </div>
        <If when={outputError()}>
          {(msg) => (
            <p className="text-red-400 text-sm" role="alert">
              {msg()}
            </p>
          )}
        </If>
      </div>

      <div className="flex flex-col gap-3 border-t border-gray-700 pt-4">
        <label className="flex items-start gap-3 cursor-pointer">
          <input
            type="checkbox"
            className="mt-0.5"
            checked={noiseSuppression()}
            disabled={!supported}
            onChange={(e) => setNoiseSuppression(e.currentTarget.checked)}
          />
          <span className="flex flex-col gap-0.5">
            <span className="font-medium text-gray-100">Noise suppression</span>
            <span className="text-xs text-gray-400">
              Filters steady background noise (fans, typing). Uses the browser's built-in processor.
            </span>
          </span>
        </label>

        <label className="flex items-start gap-3 cursor-pointer">
          <input
            type="checkbox"
            className="mt-0.5"
            checked={showSpeakingEverywhere()}
            onChange={(e) => setShowSpeakingEverywhere(e.currentTarget.checked)}
          />
          <span className="flex flex-col gap-0.5">
            <span className="font-medium text-gray-100">
              If speaking indicators for voice channels I'm not in
            </span>
            <span className="text-xs text-gray-400">
              Highlights speakers in the sidebar even when you haven't joined the channel.
            </span>
          </span>
        </label>

        <div className="flex flex-col gap-1">
          <label htmlFor="voice-input-gain" className="flex items-center justify-between">
            <span className="font-medium text-gray-100">Input volume</span>
            <span className="text-xs text-gray-400" aria-hidden="true">
              {Math.round(inputGain() * 100)}%
            </span>
          </label>
          <input
            id="voice-input-gain"
            type="range"
            min="0"
            max="2"
            step="0.05"
            value={inputGain()}
            disabled={!supported}
            className="w-full"
            onInput={(e) => setInputGain(Number.parseFloat(e.currentTarget.value))}
          />
          <p className="text-xs text-gray-400">
            Adjusts how loud your voice is to other participants.
          </p>
        </div>
      </div>
    </div>
  );
}
